use std::ops::{Add, Mul, Sub};

use eyre::Result;
use scuttlebutt::field::FiniteField;

use crate::{backend_trait::BackendT, circuit_ir::WireCount};

use super::{acc::Accumulator, DisjGate};

// position in extended witness fixed to 1
const CONSTANT_IDX: usize = 0;

#[derive(Debug, Clone, Default)]
struct LinComb<F: FiniteField>(Vec<(usize, F)>);

// Invariant: shorted by index
fn merge<F: FiniteField, OP: Fn(F, F) -> F>(
    lhs: &LinComb<F>,
    rhs: &LinComb<F>,
    f: OP,
) -> LinComb<F> {
    let mut comb = Vec::new();
    let mut i = 0;
    let mut j = 0;

    // merge terms
    while i < lhs.0.len() && j < rhs.0.len() {
        let (idx_a, a) = lhs.0[i];
        let (idx_b, b) = rhs.0[j];
        if idx_a == idx_b {
            comb.push((idx_a, f(a, b)));
            i += 1;
            j += 1;
        } else if idx_a < idx_b {
            comb.push((idx_a, a));
            i += 1;
        } else {
            comb.push((idx_b, b));
            j += 1;
        }
    }

    // copy remaining terms
    for i in i..lhs.0.len() {
        comb.push(lhs.0[i])
    }

    // copy remaining terms
    for j in j..rhs.0.len() {
        comb.push(rhs.0[j])
    }

    comb.shrink_to_fit();
    LinComb(comb)
}

impl<F: FiniteField> Add for LinComb<F> {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        merge(&self, &rhs, |a, b| a + b)
    }
}

impl<F: FiniteField> Sub for LinComb<F> {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        merge(&self, &rhs, |a, b| a - b)
    }
}

impl<F: FiniteField> Mul<F> for LinComb<F> {
    type Output = Self;

    fn mul(self, rhs: F) -> Self::Output {
        let mut comb = Vec::new();
        for (idx, coeff) in self.0 {
            comb.push((idx, coeff * rhs));
        }
        LinComb(comb)
    }
}

impl<F: FiniteField> LinComb<F> {
    pub fn constant(c: F) -> Self {
        LinComb(vec![(CONSTANT_IDX, c)])
    }

    pub fn eval(&self, w: &[F]) -> F {
        let mut comb = F::zero();
        for (idx, coeff) in self.0.iter().copied() {
            comb += coeff * w[idx];
        }
        comb
    }

    pub fn eval_commit<B: BackendT<FieldElement = F>>(
        &self,
        backend: &mut B,
        w: &[B::Wire],
    ) -> Result<B::Wire> {
        // first term
        let mut entries = self.0.iter().copied();
        let (idx, coeff) = match entries.next() {
            Some(entry) => entry,
            None => {
                return backend.input_public(F::ZERO);
            }
        };
        let mut comb = backend.mul_constant(&w[idx], coeff)?;

        // all other terms
        for (idx, coeff) in entries {
            let prod = backend.mul_constant(&w[idx], coeff)?;
            comb = backend.add(&comb, &prod)?;
        }

        Ok(comb)
    }
}

#[derive(Debug, Clone)]
pub(super) struct Row<F: FiniteField> {
    l: LinComb<F>,
    r: LinComb<F>,
    o: LinComb<F>,
}

impl<F: FiniteField> Row<F> {
    pub(crate) fn eval(&self, wit: &[F]) -> (F, F, F) {
        (self.l.eval(wit), self.r.eval(wit), self.o.eval(wit))
    }

    pub(crate) fn eval_commit<B: BackendT<FieldElement = F>>(
        &self,
        backend: &mut B,
        wit: &[B::Wire],
    ) -> Result<(B::Wire, B::Wire, B::Wire)> {
        let l = self.l.eval_commit(backend, wit)?;
        let r = self.r.eval_commit(backend, wit)?;
        let o = self.o.eval_commit(backend, wit)?;
        Ok((l, r, o))
    }
}

#[derive(Debug, Clone)]
pub(super) struct R1CS<F: FiniteField> {
    pub dim: usize,
    pub cells: usize,
    pub input: usize,
    pub output: usize,
    pub gates: Vec<DisjGate<F>>, // used for witness computation
    pub rows: Vec<Row<F>>,
    // k = log2 of the (padded) number of rows, shared by every clause of the
    // disjunction so that the folding transcript does not depend on the clause
    pub log_rows: usize,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ExtendedWitness<F: FiniteField> {
    pub(super) inputs: usize,
    pub(super) outputs: usize,
    // [1 || output || input || inp ]
    pub(super) wit: Vec<F>,
}

impl<F: FiniteField> ExtendedWitness<F> {
    pub(super) fn outputs(&self) -> impl Iterator<Item = &F> {
        self.wit[1..=self.outputs].iter()
    }

    pub(super) fn intermediate(&self) -> impl Iterator<Item = &F> {
        self.wit[1 + self.outputs + self.inputs..].iter()
    }

    pub(super) fn check(&self, r1cs: &R1CS<F>) -> bool {
        if r1cs.dim() != self.wit.len() {
            return false;
        }
        for row in r1cs.rows.iter() {
            let (l, r, o) = row.eval(&self.wit);
            if l * r != o {
                return false;
            }
        }
        true
    }
}

impl<F: FiniteField> R1CS<F> {
    pub(crate) fn rows(&self) -> usize {
        self.rows.len()
    }

    pub(crate) fn dim(&self) -> usize {
        self.dim
    }

    /// Dimension of the witness (dimension excluding the constant term and input)
    pub(crate) fn dim_wit(&self) -> usize {
        assert!(self.dim > 0);
        assert!(self.dim > self.input);
        self.dim - self.input - 1
    }

    // converts a circuit into a R1CS relation
    pub(crate) fn new(input: WireCount, output: WireCount, gates: &[DisjGate<F>]) -> Self {
        assert!(input > 0, "no inputs");

        // compute the number of cells required for witness generation
        let max_cell = gates
            .iter()
            .copied()
            .map(|gate| match gate {
                DisjGate::Mul(dst, ..)
                | DisjGate::Add(dst, ..)
                | DisjGate::Sub(dst, ..)
                | DisjGate::AddConstant(dst, ..)
                | DisjGate::MulConstant(dst, ..)
                | DisjGate::AssertZero(dst, ..)
                | DisjGate::AssertConstant(dst, ..)
                | DisjGate::Copy(dst, ..)
                | DisjGate::Witness(dst)
                | DisjGate::Constant(dst, ..) => dst,
            })
            .max()
            .unwrap_or(0);

        let num_cells = std::cmp::max(max_cell + 1, (input + output) as usize);

        // allocate cells for symbolic evaluation of circuit
        let mut cells = Vec::with_capacity(num_cells);
        cells.extend((0..output).map(|_| LinComb::default()));
        cells.extend((0..input).map(|i| LinComb(vec![((1 + i + output) as usize, F::ONE)])));
        cells.resize(num_cells, LinComb::default());

        // compute constraints
        let mut cs = Vec::with_capacity(max_cell);
        let mut dim = (1 + input + output) as usize;
        for gate in gates.iter().copied() {
            match gate {
                DisjGate::Mul(dst, l, r) => {
                    let l = cells[l].clone();
                    let r = cells[r].clone();
                    let o = LinComb(vec![(dim, F::ONE)]);
                    cells[dst] = o.clone();
                    cs.push(Row { l, r, o });
                    dim += 1;
                }
                DisjGate::Copy(dst, src) => {
                    cells[dst] = cells[src].clone();
                }
                DisjGate::Witness(dst) => {
                    cells[dst] = LinComb(vec![(dim, F::ONE)]);
                    dim += 1;
                }
                DisjGate::Constant(dst, val) => {
                    cells[dst] = LinComb(vec![(CONSTANT_IDX, val)]);
                }
                DisjGate::Add(dst, l, r) => {
                    cells[dst] = cells[l].clone() + cells[r].clone();
                }
                DisjGate::Sub(dst, l, r) => {
                    cells[dst] = cells[l].clone() - cells[r].clone();
                }
                DisjGate::AddConstant(dst, l, c) => {
                    cells[dst] = cells[l].clone() + LinComb::constant(c);
                }
                DisjGate::MulConstant(dst, l, c) => {
                    cells[dst] = cells[l].clone() * c;
                }
                DisjGate::AssertZero(src) => {
                    cs.push(Row {
                        l: cells[src].clone(),
                        r: LinComb(vec![(CONSTANT_IDX, F::ONE)]),
                        o: LinComb(vec![]),
                    });
                }
                // used to easily implement the guards for clauses
                DisjGate::AssertConstant(src, val) => {
                    cs.push(Row {
                        l: cells[src].clone(),
                        r: LinComb(vec![(CONSTANT_IDX, F::ONE)]),
                        o: LinComb(vec![(CONSTANT_IDX, val)]),
                    });
                }
            }
        }

        // add output asserts
        // (if an output is unassigned it is set to 0)
        for src in 0..(output as usize) {
            cs.push(Row {
                l: cells[src].clone(),
                r: LinComb(vec![(CONSTANT_IDX, F::ONE)]),
                o: LinComb(vec![(src + 1, F::ONE)]),
            });
        }

        cs.shrink_to_fit();

        Self {
            dim,
            input: input as usize,
            output: output as usize,
            cells: num_cells,
            gates: gates.to_vec(),
            rows: cs,
            log_rows: 0, // set by Disjunction::new
        }
    }

    pub fn compute_witness<I: Iterator<Item = F>>(&self, input: I) -> ExtendedWitness<F> {
        // copy input to cells (for evaluation)
        // [1 || output || input]
        let mut cells = Vec::with_capacity(self.cells);
        cells.extend((0..self.output).map(|_| F::ZERO));
        cells.extend(input);
        debug_assert_eq!(cells.len(), self.input + self.output);

        // alloc space for [ output || input ]
        let mut wit = Vec::with_capacity(self.dim);
        wit.push(F::ONE);
        wit.extend_from_slice(&cells[..]);

        // allocate temp cells for evaluation
        cells.resize(self.cells, F::ZERO);
        debug_assert_eq!(cells.len(), self.cells);

        // compute output (and intermediate witness values)
        for gate in self.gates.iter().copied() {
            match gate {
                DisjGate::Mul(dst, lhs, rhs) => {
                    let res = cells[lhs] * cells[rhs];
                    wit.push(res);
                    cells[dst] = res;
                }
                DisjGate::Copy(dst, src) => {
                    cells[dst] = cells[src];
                }
                DisjGate::Constant(dst, val) => {
                    cells[dst] = val;
                }
                DisjGate::Add(dst, lhs, rhs) => {
                    cells[dst] = cells[lhs] + cells[rhs];
                }
                DisjGate::Sub(dst, l, r) => {
                    cells[dst] = cells[l] - cells[r];
                }
                DisjGate::AddConstant(dst, src, c) => {
                    cells[dst] = cells[src] + c;
                }
                DisjGate::MulConstant(dst, src, c) => {
                    cells[dst] = cells[src] * c;
                }
                DisjGate::AssertConstant(src, val) => {
                    assert_eq!(
                        cells[src], val,
                        "assert equal in disjunction not satisified"
                    );
                }
                DisjGate::AssertZero(src) => {
                    assert_eq!(cells[src], F::ZERO, "assert zero in disjunction failed");
                }
                DisjGate::Witness(_dst) => {
                    // this should just assign a cell freely
                    // but we need to pass in the witness tape somehow.
                    unimplemented!("not implemented (yet)")
                }
            }
        }

        // copy outputs to witness
        wit[1..=self.output].copy_from_slice(&cells[..self.output]);

        // wrap in extended witness
        let ext = ExtendedWitness {
            wit,
            inputs: self.input,
            outputs: self.output,
        };

        // sanity check: R1CS relation is sat.
        debug_assert!(ext.check(self), "{:#?}", ext);
        ext
    }


    pub(crate) fn log_rows(&self) -> usize {
        self.log_rows
    }

    /// Number of evaluations of y(t) the prover commits to per fold:
    /// y(t) has degree k + 2 (eq(tau(t), i) has degree k, each constraint degree 2),
    /// so k + 3 points determine it. y(0) and y(1) are known, which leaves k + 1.
    pub(crate) fn num_evals(&self) -> usize {
        self.log_rows + 1
    }

    /// The constraint polynomial F_i(w) = l_i(w) * r_i(w) - o_i(w)
    fn constraint(row: &Row<F>, w: &[F]) -> F {
        let (l, r, o) = row.eval(w);
        l * r - o
    }

    /// G(tau, w) = sum_i eq(tau, i) * F_i(w)
    ///
    /// The accumulator relation (Interstellar-style circuit interpolation)
    /// is G(tau, w) = y.
    pub(crate) fn eval_g(&self, tau: &[F], w: &[F]) -> F {
        debug_assert_eq!(tau.len(), self.log_rows);
        let eq = eq_table(tau);
        debug_assert!(eq.len() >= self.rows());
        let mut y = F::ZERO;
        for (row, e) in self.rows.iter().zip(eq.iter().copied()) {
            y += e * Self::constraint(row, w);
        }
        y
    }

    /// Evaluations of y(t) = G(tau(t), w(t)) at t = 2, ..., k + 2, where
    ///
    ///   tau(t) = (1 - t) * tau1 + t * tau2
    ///   w(t)   = (1 - t) * w1   + t * w2
    ///
    /// with (tau1, w1, y1) the accumulator and (tau2, w2, 0) the fresh instance.
    /// Note that y(0) = y1 and (for an honest prover) y(1) = 0.
    pub(crate) fn fold_evals(
        &self,
        acc: &Accumulator<F>,
        tau2: &[F],
        wit: &ExtendedWitness<F>,
    ) -> Vec<F> {
        debug_assert_eq!(acc.tau.len(), self.log_rows);
        debug_assert_eq!(tau2.len(), self.log_rows);
        debug_assert_eq!(acc.wit.len(), self.dim);
        debug_assert_eq!(wit.wit.len(), self.dim);

        let mut evals = Vec::with_capacity(self.num_evals());
        let mut tau_t = vec![F::ZERO; self.log_rows];
        let mut w_t = vec![F::ZERO; self.dim];
        for j in 0..self.num_evals() {
            let t = small::<F>(j + 2);
            let omt = F::ONE - t;
            for (dst, (a, b)) in tau_t.iter_mut().zip(acc.tau.iter().zip(tau2.iter())) {
                *dst = omt * *a + t * *b;
            }
            for (dst, (a, b)) in w_t.iter_mut().zip(acc.wit.iter().zip(wit.wit.iter())) {
                *dst = omt * *a + t * *b;
            }
            evals.push(self.eval_g(&tau_t, &w_t));
        }
        evals
    }
}

/// The table eq(tau, i) for i = 0, ..., 2^k - 1,
/// where bit j of i is matched against tau[j]:
///
///   eq(tau, i) = prod_j (i_j ? tau_j : 1 - tau_j)
pub(super) fn eq_table<F: FiniteField>(tau: &[F]) -> Vec<F> {
    let mut table = Vec::with_capacity(1 << tau.len());
    table.push(F::ONE);
    for t in tau.iter().copied() {
        let len = table.len();
        let omt = F::ONE - t;
        for i in 0..len {
            let e = table[i];
            table.push(e * t);
            table[i] = e * omt;
        }
    }
    table
}

/// The field element n (for small n)
pub(super) fn small<F: FiniteField>(n: usize) -> F {
    let mut x = F::ZERO;
    for _ in 0..n {
        x += F::ONE;
    }
    x
}

/// Lagrange coefficients L_0(r), ..., L_{n-1}(r) for the points 0, 1, ..., n - 1,
/// i.e. p(r) = sum_j L_j(r) * p(j) for every polynomial p of degree < n.
pub(super) fn lagrange_coeffs<F: FiniteField>(n: usize, r: F) -> Vec<F> {
    let pts: Vec<F> = (0..n).map(small::<F>).collect();
    (0..n)
        .map(|j| {
            let mut num = F::ONE;
            let mut den = F::ONE;
            for m in 0..n {
                if m != j {
                    num *= r - pts[m];
                    den *= pts[j] - pts[m];
                }
            }
            num * den.inverse()
        })
        .collect()
}
