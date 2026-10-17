//! Accumulators for Interstellar-style folding (circuit interpolation).
//!
//! An accumulator for a clause is a triple (tau, w, y) satisfying
//!
//!   G(tau, w) = sum_i eq(tau, i) * F_i(w) = y
//!
//! where F_i(w) = l_i(w) * r_i(w) - o_i(w) are the constraints of the clause.
//! A fresh instance is (tau2, w2, 0) with tau2 a random challenge.
//! This replaces Dora's relaxed R1CS accumulator (w, u, e):
//! there are no cross terms and no error vector, only the k = log(#rows)
//! coordinates of tau and the single scalar y.
use eyre::Result;

use scuttlebutt::{field::FiniteField, ring::FiniteRing, AbstractChannel};
use swanky_field::IsSubFieldOf;

use crate::{backend_trait::BackendT, DietMacAndCheeseProver};

use super::{
    comm::{CommittedEvals, CommittedWitness},
    disjunction::Disjunction,
    r1cs::R1CS,
};

pub(super) struct ComittedAcc<B: BackendT> {
    pub tau: Vec<B::Wire>, // commitment to tau (k elements)
    pub wit: Vec<B::Wire>, // commitment to witness
    pub y: B::Wire,        // commitment to y
}

pub(super) struct Trace<B: BackendT> {
    pub old: ComittedAcc<B>,
    pub new: ComittedAcc<B>,
}

pub(super) fn collapse_trace<B: BackendT>(
    backend: &mut B,
    trace: &[Trace<B>],
    x: B::FieldElement,
) -> Result<(Vec<B::Wire>, Vec<B::Wire>)> {
    let mut lhs = Vec::with_capacity(trace.len() + 1);
    let mut rhs = Vec::with_capacity(trace.len() + 1);
    for tr in trace {
        lhs.push(tr.old.combine(backend, x)?);
        rhs.push(tr.new.combine(backend, x)?);
    }
    Ok((lhs, rhs))
}

impl<B: BackendT> ComittedAcc<B> {
    /// Verify a committed accumulator using the underlaying proof system:
    /// checks sum_i eq(tau, i) * F_i(w) = y on the commitments.
    ///
    /// The witness may have junk at the end (which is not verfied)
    /// for an honest prover this junk will be zero, however it is
    /// not required to enforce this for soundness.
    pub fn verify(&self, backend: &mut B, r1cs: &R1CS<B::FieldElement>) -> Result<()> {
        if self.wit.len() < r1cs.dim() {
            return Err(eyre::eyre!("witness dimension too small"));
        }

        if self.tau.len() != r1cs.log_rows() {
            return Err(eyre::eyre!("tau dimension mismatch"));
        }

        // compute the table eq(tau, i) on the commitments
        // (same bit order as r1cs::eq_table)
        let one = <B::FieldElement as FiniteRing>::ONE;
        let mut table = vec![backend.input_public(one)?];
        for t in self.tau.iter() {
            let neg = backend.mul_constant(t, -one)?;
            let omt = backend.add_constant(&neg, one)?;
            let len = table.len();
            for i in 0..len {
                let hi = backend.mul(&table[i], t)?;
                let lo = backend.mul(&table[i], &omt)?;
                table.push(hi);
                table[i] = lo;
            }
        }
        debug_assert!(table.len() >= r1cs.rows());

        // G(tau, w) - y
        let mut sum = backend.copy(&self.y)?;
        sum = backend.mul_constant(&sum, -one)?;
        for (row, eq) in r1cs.rows.iter().zip(table.iter()) {
            let (l, r, o) = row.eval_commit(backend, &self.wit)?;
            let m = backend.mul(&l, &r)?;
            let f = backend.sub(&m, &o)?;
            let t = backend.mul(eq, &f)?;
            sum = backend.add(&sum, &t)?;
        }
        backend.assert_zero(&sum)
    }

    /// Compress the accumulator into a single element (for the permutation proof)
    pub fn combine(&self, backend: &mut B, x: B::FieldElement) -> Result<B::Wire> {
        let mut cs = self
            .tau
            .iter()
            .chain(self.wit.iter())
            .chain(std::iter::once(&self.y));
        let mut y: B::Wire = backend.copy(cs.next().unwrap())?;
        for c in cs {
            y = backend.mul_constant(&y, x)?;
            y = backend.add(&y, c)?;
        }
        Ok(y)
    }

    /// Fold the fresh instance (tau2, w2, 0) into the accumulator at the point r:
    ///
    ///   tau' = (1 - r) * tau1 + r * tau2
    ///   w'   = (1 - r) * w1   + r * w2
    ///   y'   = L_0(r) * y1 + L_1(r) * 0 + sum_{j >= 2} L_j(r) * y(j)
    ///
    /// All coefficients are public and do not depend on the clause,
    /// so this is a linear (local) operation on the commitments.
    pub fn fold(
        &self,
        backend: &mut B,
        r: B::FieldElement,
        tau2: &[B::FieldElement],
        evals: &CommittedEvals<B>,
        wit: &CommittedWitness<B>,
        lag: &[B::FieldElement],
    ) -> Result<Self> {
        debug_assert_eq!(self.tau.len(), tau2.len());
        debug_assert_eq!(self.wit.len(), wit.wit.len());
        debug_assert_eq!(lag.len(), evals.evals.len() + 2);

        let omr = <B::FieldElement as FiniteRing>::ONE - r;

        let mut ntau = Vec::with_capacity(self.tau.len());
        for (t1, t2) in self.tau.iter().zip(tau2.iter().copied()) {
            let a = backend.mul_constant(t1, omr)?;
            ntau.push(backend.add_constant(&a, r * t2)?);
        }

        let mut nwit = Vec::with_capacity(self.wit.len());
        for (w1, w2) in self.wit.iter().zip(wit.wit.iter()) {
            let a = backend.mul_constant(w1, omr)?;
            let b = backend.mul_constant(w2, r)?;
            nwit.push(backend.add(&a, &b)?);
        }

        let mut ny = backend.mul_constant(&self.y, lag[0])?;
        for (e, l) in evals.evals.iter().zip(lag[2..].iter().copied()) {
            let a = backend.mul_constant(e, l)?;
            ny = backend.add(&ny, &a)?;
        }

        Ok(Self {
            tau: ntau,
            wit: nwit,
            y: ny,
        })
    }
}

impl<V: IsSubFieldOf<F>, F: FiniteField, C: AbstractChannel>
    ComittedAcc<DietMacAndCheeseProver<V, F, C>>
where
    F::PrimeField: IsSubFieldOf<V>,
{
    pub fn value(&self, clause: &R1CS<V>) -> Accumulator<V> {
        debug_assert!(self.wit.len() >= clause.dim());
        debug_assert_eq!(self.tau.len(), clause.log_rows());
        Accumulator {
            tau: self.tau.iter().map(|t| t.value()).collect(),
            wit: self.wit[..clause.dim()].iter().map(|w| w.value()).collect(),
            y: self.y.value(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
// Interstellar-style accumulator
pub(super) struct Accumulator<F: FiniteField> {
    pub tau: Vec<F>, // point for eq(tau, .)
    pub wit: Vec<F>, // extended witness
    pub y: F,        // claimed value of G(tau, w)
}

impl<F: FiniteField> Accumulator<F> {
    // Computes an arbitrary fixed accumulator in the accumulation language:
    // with w = 0 every constraint is zero (the constraints have no constant term),
    // hence G(tau, 0) = 0 = y for any tau.
    pub(crate) fn init(rel: &R1CS<F>) -> Self {
        let acc = Self {
            tau: vec![F::ZERO; rel.log_rows()],
            wit: vec![F::ZERO; rel.dim()],
            y: F::ZERO,
        };
        debug_assert!(acc.check(rel));
        acc
    }

    // Checks membership of the accumulator language.
    pub(crate) fn check(&self, r1cs: &R1CS<F>) -> bool {
        if r1cs.dim() != self.wit.len() || r1cs.log_rows() != self.tau.len() {
            return false;
        }
        r1cs.eval_g(&self.tau, &self.wit) == self.y
    }

    /// Same as ComittedAcc::combine on the padded commitment
    /// (the committed witness is padded with zeros to the disjunction dimension).
    pub fn combine<B: BackendT<FieldElement = F>>(
        &self,
        backend: &mut B,
        disj: &Disjunction<F>,
        x: B::FieldElement,
    ) -> Result<B::Wire> {
        let pad = disj.dim_ext() - self.wit.len();
        let mut cs = self
            .tau
            .iter()
            .copied()
            .chain(self.wit.iter().copied())
            .chain(std::iter::repeat(F::ZERO).take(pad))
            .chain(std::iter::once(self.y));
        let mut y = cs.next().unwrap();
        for c in cs {
            y = y * x;
            y = y + c;
        }
        backend.input_public(y)
    }
}
