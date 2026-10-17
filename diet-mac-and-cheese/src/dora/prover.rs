use eyre::Result;

use scuttlebutt::{field::FiniteField, AbstractChannel};
use swanky_field::IsSubFieldOf;

use crate::{
    dora::{
        comm::{CommittedEvals, CommittedWitness},
        r1cs::lagrange_coeffs,
        tx::TxChannel,
        COMPACT_MIN, COMPACT_MUL,
    },
    homcom::MacProver,
    DietMacAndCheeseProver,
};

use super::{
    acc::{self, collapse_trace, Accumulator, ComittedAcc, Trace},
    disjunction::Disjunction,
    fiat_shamir,
    perm::permutation,
};

pub struct DoraProver<V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    calls: usize,
    tx: blake3::Hasher,
    _ph: std::marker::PhantomData<(F, C)>,
    accs: Vec<acc::Accumulator<V>>, // current state of accumulator
    disj: Disjunction<V>,
    init: Vec<ComittedAcc<DietMacAndCheeseProver<V, F, C>>>,
    trace: Vec<Trace<DietMacAndCheeseProver<V, F, C>>>,
    max_trace: usize, // maximum trace len before compactification
    // test only: corrupt the witness of the given call (to test soundness)
    #[cfg(test)]
    pub(crate) cheat_at: Option<usize>,
}

impl<V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel> DoraProver<V, F, C>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn new(disj: Disjunction<V>) -> Self {
        let accs = disj
            .clauses()
            .iter()
            .map(|rel| Accumulator::init(rel))
            .collect();
        let max_trace = std::cmp::max(disj.clauses().len() * COMPACT_MUL, COMPACT_MIN);
        Self {
            _ph: std::marker::PhantomData,
            tx: blake3::Hasher::new(),
            max_trace,
            accs,
            trace: Vec::with_capacity(max_trace),
            init: vec![],
            calls: 0,
            disj,
            #[cfg(test)]
            cheat_at: None,
        }
    }

    // execute a mux over a secret clause
    //
    // returns commitments to the output
    pub fn mux(
        &mut self,
        prover: &mut DietMacAndCheeseProver<V, F, C>,
        input: &[MacProver<V, F>],
        opt: usize,
    ) -> Result<Vec<MacProver<V, F>>> {
        // check if we should compact the trace first
        if self.trace.len() >= self.max_trace {
            self.compact(prover)?;
        }

        // retrieve R1CS for active clause
        let clause = self.disj.clause(opt);

        // compute extended witness for the clause
        #[allow(unused_mut)]
        let mut wit = clause.compute_witness(input.iter().map(|inp| inp.value()));
        debug_assert!(wit.check(clause));

        #[cfg(test)]
        if self.cheat_at == Some(self.calls) {
            // claim a wrong value for the last intermediate wire
            let last = wit.wit.len() - 1;
            wit.wit[last] += V::ONE;
        }

        // wrap in transcript hasher (if FS enabled)
        let mut ch = TxChannel::new(prover.channel.clone(), &mut self.tx);

        // 1. commit to extended witness w2
        let comm_wit = CommittedWitness::commit_prover(
            &mut ch,
            prover,
            &self.disj,
            input.iter().copied(),
            &wit,
        )?;
        debug_assert_eq!(comm_wit.value(clause), wit);

        // 2. commit to old accumulator (tau1, w1, y1)
        let comm_acc = ComittedAcc::commit_prover(&mut ch, prover, &self.disj, &self.accs[opt])?;
        debug_assert_eq!(&comm_acc.value(clause), &self.accs[opt]);

        // 3. challenge tau2 for the fresh instance (after w2 is fixed)
        let tau2: Vec<V> = (0..self.disj.log_rows()).map(|_| ch.challenge()).collect();

        // 4. commit to y(2), ..., y(k + 2) along the line between the instances
        let evals = clause.fold_evals(&self.accs[opt], &tau2, &wit);
        let comm_evals = CommittedEvals::commit_prover(&mut ch, prover, &self.disj, &evals)?;

        // 5. fold at a random point r
        let r: V = ch.challenge();
        let lag = lagrange_coeffs(self.disj.num_evals() + 2, r);
        let comm_acc_new = comm_acc.fold(prover, r, &tau2, &comm_evals, &comm_wit, &lag)?;

        // store new accumulator
        self.accs[opt] = comm_acc_new.value(clause);
        #[cfg(not(test))]
        debug_assert!(self.accs[opt].check(clause));
        #[cfg(test)]
        assert!(self.cheat_at.is_some() || self.accs[opt].check(clause));

        // add to trace (for permutation proof)
        self.trace.push(Trace {
            old: comm_acc,
            new: comm_acc_new,
        });

        self.calls += 1;

        // returns the outputs from the extended witness
        Ok(comm_wit.outputs().to_owned())
    }

    /// Simply verify all the final accumulators using MacAndCheese
    pub fn finalize(mut self, prover: &mut DietMacAndCheeseProver<V, F, C>) -> Result<()>
    where
        F::PrimeField: IsSubFieldOf<V>,
    {
        log::info!(
            "finalizing Dora proof: {} calls (of dimension {}) to disjunction of {} clauses",
            self.calls,
            self.disj.dim_ext(),
            self.disj.clauses().len()
        );

        // compact trace into single set of accumulators
        self.compact(prover)?;

        // verify accumulors
        for (acc, r1cs) in self.init.into_iter().zip(self.disj.clauses()) {
            acc.verify(prover, r1cs)?;
        }
        Ok(())
    }

    // "compact" the disjunction trace (without verification)
    //
    // Commits to all the accumulators and executes the permutation proof.
    // This reduces the trace to a single element per branch.
    fn compact(&mut self, prover: &mut DietMacAndCheeseProver<V, F, C>) -> Result<()> {
        // commit to all accumulators
        let mut ch = TxChannel::new(prover.channel.clone(), &mut self.tx);
        let mut accs = Vec::with_capacity(self.disj.clauses().len());
        for acc in self.accs.iter() {
            accs.push(ComittedAcc::commit_prover(
                &mut ch, prover, &self.disj, acc,
            )?);
        }

        // obtain challenges for permutation proof
        let (chal_perm, chal_cmbn) = if fiat_shamir::<V>() {
            (ch.challenge(), ch.challenge())
        } else {
            ch.flush()?;
            (
                prover.channel.read_serializable::<V>()?,
                prover.channel.read_serializable::<V>()?,
            )
        };

        // combine elements from trace into single field elements
        let (mut lhs, mut rhs) = collapse_trace(prover, &self.trace, chal_cmbn)?;

        // add initial / final accumulator to permutation proof
        for (i, (acc, r1cs)) in accs.iter().zip(self.disj.clauses()).enumerate() {
            lhs.push(acc.combine(prover, chal_cmbn)?);
            rhs.push(match self.init.get(i) {
                Some(acc) => acc.combine(prover, chal_cmbn)?,
                None => Accumulator::init(r1cs).combine(prover, &self.disj, chal_cmbn)?,
            });
        }

        // execute permutation proof
        self.trace.clear();
        self.init = accs;
        permutation(prover, chal_perm, &lhs, &rhs)
    }
}

#[cfg(test)]
mod tests {
    use std::{
        io::{BufReader, BufWriter},
        os::unix::net::UnixStream,
    };

    use ocelot::svole::{LPN_EXTEND_SMALL, LPN_SETUP_SMALL};
    use scuttlebutt::{field::F61p, ring::FiniteRing, AesRng, Channel};

    use crate::backend_trait::BackendT;
    use crate::{
        dora::{verifier::DoraVerifier, Clause, DisjGate},
        DietMacAndCheeseVerifier,
    };
    use rand::{Rng, SeedableRng};

    use super::*;

    // a very simple example using a disjunction to implement a range check
    #[test]
    fn test_range_example() {
        const RANGE_SIZE: usize = 256;
        const NUM_RANGE_PROOFS: usize = 10_000;

        // a range check
        let clauses = (0..RANGE_SIZE)
            .map(|i| Clause {
                gates: vec![
                    DisjGate::AddConstant(1, 0, -F61p::try_from(i as u128).unwrap()),
                    DisjGate::AssertZero(1),
                ],
            })
            .collect::<Vec<_>>();

        let range_check = Disjunction::new(clauses.iter().cloned(), 1, 0);

        let (sender, receiver) = UnixStream::pair().unwrap();

        let range_check_clone = range_check.clone();

        let handle = std::thread::spawn(move || {
            let rng = AesRng::from_seed(Default::default());
            let reader = BufReader::new(sender.try_clone().unwrap());
            let writer = BufWriter::new(sender);
            let mut channel = Channel::new(reader, writer);

            let mut prover: DietMacAndCheeseProver<F61p, F61p, _> = DietMacAndCheeseProver::init(
                &mut channel,
                rng,
                LPN_SETUP_SMALL,
                LPN_EXTEND_SMALL,
                false,
            )
            .unwrap();

            let mut disj = DoraProver::<F61p, F61p, _>::new(range_check);

            println!("warm up");
            prover.input_private(Some(F61p::ONE)).unwrap();

            // do a number of range checks
            println!("do {} range proofs", NUM_RANGE_PROOFS);
            for _ in 0..NUM_RANGE_PROOFS {
                let wi: usize = prover.rng.gen::<usize>() % RANGE_SIZE;
                let wf = F61p::try_from(wi as u128).unwrap();
                let v = prover.input_private(Some(wf)).unwrap();
                disj.mux(&mut prover, &[v], wi).unwrap(); // because it is assigned 1
            }
            disj.finalize(&mut prover).unwrap();

            println!("finalize");
            prover.finalize().unwrap();

            println!("done");
        });

        let rng = AesRng::from_seed(Default::default());
        let reader = BufReader::new(receiver.try_clone().unwrap());
        let writer = BufWriter::new(receiver);
        let mut channel = Channel::new(reader, writer);

        let mut dmc: DietMacAndCheeseVerifier<F61p, F61p, _> = DietMacAndCheeseVerifier::init(
            &mut channel,
            rng,
            LPN_SETUP_SMALL,
            LPN_EXTEND_SMALL,
            false,
        )
        .unwrap();

        dmc.input_private(None).unwrap();

        let mut disj = DoraVerifier::new(range_check_clone);
        for _ in 0..NUM_RANGE_PROOFS {
            let v = dmc.input_private(None).unwrap();
            disj.mux(&mut dmc, &[v]).unwrap();
        }
        disj.finalize(&mut dmc).unwrap();
        dmc.finalize().unwrap();

        handle.join().unwrap();
    }

    // clauses with multiplications: out = x * y + c_i * x, plus a few extra
    // multiplication rows (so there are intermediate wires to cheat on)
    fn mul_clauses(n: usize) -> Vec<Clause<F61p>> {
        (0..n)
            .map(|i| {
                let c = F61p::try_from((i + 3) as u128).unwrap();
                Clause {
                    gates: vec![
                        DisjGate::Mul(3, 1, 2),
                        DisjGate::MulConstant(4, 1, c),
                        DisjGate::Add(0, 3, 4),
                        DisjGate::Mul(5, 3, 3),
                        DisjGate::Mul(6, 5, 1),
                        DisjGate::Mul(7, 6, 2),
                    ],
                }
            })
            .collect()
    }

    // folding on plain values: the invariant G(tau, w) = y is kept by honest folds,
    // and a single bad witness makes the final accumulator invalid
    #[test]
    fn test_fold_plain() {
        use super::super::r1cs::lagrange_coeffs;
        let disj = Disjunction::new(mul_clauses(3).into_iter(), 2, 1);
        let mut rng = AesRng::from_seed(Default::default());
        for cheat in [None, Some(7)] {
            let mut accs: Vec<_> = disj.clauses().iter().map(Accumulator::init).collect();
            for step in 0..20 {
                let opt = step % 3;
                let clause = disj.clause(opt);
                let x = F61p::random(&mut rng);
                let y = F61p::random(&mut rng);
                let mut wit = clause.compute_witness([x, y].into_iter());
                if cheat == Some(step) {
                    let last = wit.wit.len() - 1;
                    wit.wit[last] += F61p::ONE;
                }
                let tau2: Vec<F61p> = (0..disj.log_rows()).map(|_| F61p::random(&mut rng)).collect();
                let evals = clause.fold_evals(&accs[opt], &tau2, &wit);
                let r = F61p::random(&mut rng);
                let lag = lagrange_coeffs(disj.num_evals() + 2, r);
                let acc = &accs[opt];
                let omr = F61p::ONE - r;
                let tau = acc.tau.iter().zip(tau2.iter()).map(|(a, b)| omr * *a + r * *b).collect();
                let w = acc.wit.iter().zip(wit.wit.iter()).map(|(a, b)| omr * *a + r * *b).collect();
                let mut ny = lag[0] * acc.y;
                for (e, l) in evals.iter().zip(lag[2..].iter()) {
                    ny += *e * *l;
                }
                accs[opt] = Accumulator { tau, wit: w, y: ny };
                if cheat.map_or(true, |c| step < c) {
                    assert!(accs[opt].check(clause), "honest fold broke the invariant");
                }
            }
            let all_ok = accs.iter().zip(disj.clauses()).all(|(a, c)| a.check(c));
            assert_eq!(all_ok, cheat.is_none());
        }
    }

    // run the full prover/verifier protocol, returns the verifier's result
    fn run_protocol(clauses: Vec<Clause<F61p>>, steps: usize, cheat_at: Option<usize>) -> Result<()> {
        let nclauses = clauses.len();
        let disj_p = Disjunction::new(clauses.iter().cloned(), 2, 1);
        let disj_v = disj_p.clone();
        let (sender, receiver) = UnixStream::pair().unwrap();

        let handle = std::thread::spawn(move || -> Result<()> {
            let rng = AesRng::from_seed(Default::default());
            let reader = BufReader::new(sender.try_clone().unwrap());
            let writer = BufWriter::new(sender);
            let mut channel = Channel::new(reader, writer);
            let mut prover: DietMacAndCheeseProver<F61p, F61p, _> = DietMacAndCheeseProver::init(
                &mut channel,
                rng,
                LPN_SETUP_SMALL,
                LPN_EXTEND_SMALL,
                false,
            )?;
            let mut disj = DoraProver::<F61p, F61p, _>::new(disj_p);
            disj.cheat_at = cheat_at;
            for i in 0..steps {
                let x = prover.input_private(Some(F61p::try_from((i + 1) as u128).unwrap()))?;
                let y = prover.input_private(Some(F61p::try_from((7 * i + 2) as u128).unwrap()))?;
                disj.mux(&mut prover, &[x, y], (3 * i + 1) % nclauses)?;
            }
            disj.finalize(&mut prover)?;
            prover.finalize()?;
            Ok(())
        });

        let rng = AesRng::from_seed(Default::default());
        let reader = BufReader::new(receiver.try_clone().unwrap());
        let writer = BufWriter::new(receiver);
        let mut channel = Channel::new(reader, writer);
        let res = (|| -> Result<()> {
            let mut dmc: DietMacAndCheeseVerifier<F61p, F61p, _> =
                DietMacAndCheeseVerifier::init(
                    &mut channel,
                    rng,
                    LPN_SETUP_SMALL,
                    LPN_EXTEND_SMALL,
                    false,
                )?;
            let mut disj = DoraVerifier::new(disj_v);
            for _ in 0..steps {
                let x = dmc.input_private(None)?;
                let y = dmc.input_private(None)?;
                disj.mux(&mut dmc, &[x, y])?;
            }
            disj.finalize(&mut dmc)?;
            dmc.finalize()?;
            Ok(())
        })();
        drop(channel);
        let _ = handle.join();
        res
    }

    #[test]
    fn test_mul_clauses_honest() {
        run_protocol(mul_clauses(4), 200, None).expect("honest prover rejected");
    }

    #[test]
    fn test_mul_clauses_cheat_detected() {
        assert!(
            { let r = run_protocol(mul_clauses(4), 200, Some(50)); println!("cheat result: {:?}", r.as_ref().err().map(|e| e.to_string())); r.is_err() },
            "cheating prover accepted"
        );
    }
}
