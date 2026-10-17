use eyre::Result;
use scuttlebutt::{field::FiniteField, ring::FiniteRing, AbstractChannel, AesRng};
use std::iter;
use swanky_field::IsSubFieldOf;

use crate::{
    backend_trait::BackendT,
    homcom::{FComProver, FComVerifier, MacProver, MacVerifier},
    DietMacAndCheeseProver, DietMacAndCheeseVerifier,
};

use super::{
    acc::{Accumulator, ComittedAcc},
    disjunction::Disjunction,
    r1cs::{ExtendedWitness, R1CS},
};

#[derive(Debug)]
pub(super) struct CommittedWitness<'a, B: BackendT> {
    disj: &'a Disjunction<B::FieldElement>,
    pub wit: Vec<B::Wire>,
}

// Low-level commit to vector
// (allows control of the channel for Fiat-Shamir)
//
// Idealy there would be a nicer way to do this.
fn prover_commit_vec<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>(
    backend: &mut FComProver<V, F>,
    channel: &mut C,
    rng: &mut AesRng,
    sec: impl IntoIterator<Item = V>, // secret values
    len: usize,                       // padded length
) -> Result<impl Iterator<Item = MacProver<V, F>>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    // commit to remaining (padded)
    let mut pad = Vec::with_capacity(len);
    pad.extend(sec.into_iter().chain(iter::repeat(V::ZERO)).take(len));

    // mac vector
    let tag = backend.input(channel, rng, &pad)?;

    // combine
    Ok(tag
        .into_iter()
        .zip(pad.into_iter())
        .map(|(t, v)| MacProver::new(v, t)))
}

fn verifier_commit_vec<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>(
    backend: &mut FComVerifier<V, F>,
    channel: &mut C,
    rng: &mut AesRng,
    len: usize, // padded length
) -> Result<impl Iterator<Item = MacVerifier<F>>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    let inp = backend.input(channel, rng, len)?;
    Ok(inp.into_iter())
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    CommittedWitness<'a, DietMacAndCheeseProver<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn commit_prover<
        'b,
        I: Iterator<Item = <DietMacAndCheeseProver<V, F, C> as BackendT>::Wire>,
    >(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseProver<V, F, C>,
        disj: &'a Disjunction<V>,
        input: I,
        witness: &'b ExtendedWitness<V>,
    ) -> Result<Self> {
        // commit to witness
        let out = witness.outputs().copied();
        let int = witness.intermediate().copied();
        let free = prover_commit_vec(
            &mut backend.prover,
            channel,
            &mut backend.rng,
            out.chain(int),
            disj.dim_output() + disj.dim_intermediate(),
        )?;
        Self::from_parts(backend, disj, input, free)
    }
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    CommittedWitness<'a, DietMacAndCheeseVerifier<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn commit_verifer<
        'b,
        I: Iterator<Item = <DietMacAndCheeseVerifier<V, F, C> as BackendT>::Wire>,
    >(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseVerifier<V, F, C>,
        disj: &'a Disjunction<V>,
        input: I,
    ) -> Result<Self> {
        // commit to witness
        let free = verifier_commit_vec(
            &mut backend.verifier,
            channel,
            &mut backend.rng,
            disj.dim_output() + disj.dim_intermediate(),
        )?;
        Self::from_parts(backend, disj, input, free)
    }
}

impl<'a, B: BackendT> CommittedWitness<'a, B> {
    fn from_parts(
        backend: &mut B,
        disj: &'a Disjunction<B::FieldElement>,
        input: impl Iterator<Item = B::Wire>,
        mut free: impl Iterator<Item = B::Wire>,
    ) -> Result<Self> {
        // constant
        let mut wit = Vec::with_capacity(disj.dim_ext());
        wit.push(backend.input_public(<B as BackendT>::FieldElement::ONE)?);

        // output
        for _ in 0..disj.outputs() {
            wit.push(free.next().unwrap());
        }

        // input
        wit.extend(input);

        // intermediate wires
        wit.extend(free);

        debug_assert_eq!(wit.len(), disj.dim_ext());

        Ok(Self { disj, wit })
    }

    pub fn outputs(&self) -> &[B::Wire] {
        let out = self.wit[1..=self.disj.dim_output()].as_ref();
        debug_assert_eq!(out.len(), self.disj.dim_output());
        out
    }
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    CommittedWitness<'a, DietMacAndCheeseProver<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn value(&self, clause: &R1CS<V>) -> ExtendedWitness<V> {
        let mut wit = Vec::with_capacity(clause.dim());
        debug_assert!(self.wit.len() >= clause.dim());
        for i in 0..clause.dim() {
            wit.push(self.wit[i].value());
        }
        ExtendedWitness {
            inputs: clause.input,
            outputs: clause.output,
            wit,
        }
    }
}

/// Commitments to y(2), ..., y(k + 2) (see R1CS::fold_evals).
///
/// The number of evaluations is fixed by the disjunction (not the clause),
/// and the values are committed rather than sent in the clear:
/// both are needed to keep the active clause hidden.
pub(super) struct CommittedEvals<B: BackendT> {
    pub evals: Vec<B::Wire>,
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    CommittedEvals<DietMacAndCheeseProver<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn commit_prover(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseProver<V, F, C>,
        disj: &'a Disjunction<V>,
        evals: &[V],
    ) -> Result<Self> {
        debug_assert_eq!(evals.len(), disj.num_evals());
        let evals = prover_commit_vec(
            &mut backend.prover,
            channel,
            &mut backend.rng,
            evals.iter().copied(),
            disj.num_evals(),
        )?
        .collect();
        Ok(Self { evals })
    }
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    CommittedEvals<DietMacAndCheeseVerifier<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn commit_verifier(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseVerifier<V, F, C>,
        disj: &'a Disjunction<V>,
    ) -> Result<Self> {
        let evals = verifier_commit_vec(
            &mut backend.verifier,
            channel,
            &mut backend.rng,
            disj.num_evals(),
        )?
        .collect();
        Ok(Self { evals })
    }
}

impl<V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    ComittedAcc<DietMacAndCheeseProver<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    /// Commit to (tau || wit || y) in a single vector, padded to the disjunction size
    pub fn commit_prover<'a>(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseProver<V, F, C>,
        disj: &'a Disjunction<V>,
        acc: &Accumulator<V>,
    ) -> Result<Self> {
        let k = disj.log_rows();
        let n = disj.dim_ext();
        debug_assert_eq!(acc.tau.len(), k);
        let vals = acc
            .tau
            .iter()
            .copied()
            .chain(acc.wit.iter().copied().chain(std::iter::repeat(V::ZERO)).take(n))
            .chain(iter::once(acc.y));
        let all: Vec<_> =
            prover_commit_vec(&mut backend.prover, channel, &mut backend.rng, vals, k + n + 1)?
                .collect();
        Ok(Self::split(all, k, n))
    }
}

impl<'a, V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    ComittedAcc<DietMacAndCheeseVerifier<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn commit_verifier<'b>(
        channel: &mut impl AbstractChannel,
        backend: &mut DietMacAndCheeseVerifier<V, F, C>,
        disj: &'a Disjunction<V>,
    ) -> Result<Self> {
        let k = disj.log_rows();
        let n = disj.dim_ext();
        let all: Vec<_> =
            verifier_commit_vec(&mut backend.verifier, channel, &mut backend.rng, k + n + 1)?
                .collect();
        Ok(Self::split(all, k, n))
    }
}

impl<B: BackendT> ComittedAcc<B> {
    fn split(mut all: Vec<B::Wire>, k: usize, n: usize) -> Self {
        debug_assert_eq!(all.len(), k + n + 1);
        let y = all.pop().unwrap();
        let wit = all.split_off(k);
        Self { tau: all, wit, y }
    }
}
