use std::{
    cmp::max,
    ops::{Add, AddAssign},
};

use group::{Group, GroupEncoding};
use subtle::Choice;

use crate::poly::{powers, BivariatePolynomial, Polynomial};

use super::VerificationVector;

/// Verification matrix for a bivariate polynomial.
///
/// The verification matrix `M` is computed as the element-wise scalar product
/// of the coefficients of a bivariate polynomial `B(x,y)` and a group
/// generator `G`.
///
/// Verification matrix:
/// ```text
///     M = [b_{i,j} * G]
/// ```
///
/// Bivariate polynomial:
/// ```text
///     B(x,y) = \sum_{i=0}^{deg_x} \sum_{j=0}^{deg_y} b_{i,j} x^i y^j
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationMatrix<G: Group> {
    /// The number of rows in the verification matrix, determined by
    /// the degree of the bivariate polynomial in the `x` variable from
    /// which the matrix was constructed.
    pub(crate) rows: usize,
    /// The number of columns in the verification matrix, determined by
    /// the degree of the bivariate polynomial in the `y` variable from
    /// which the matrix was constructed.
    pub(crate) cols: usize,
    /// The verification matrix elements, where `m[i][j]` represents
    /// the element `b_{i,j} * G`.
    pub(crate) m: Vec<Vec<G>>,
}

impl<G> VerificationMatrix<G>
where
    G: Group,
{
    /// Returns the dimensions (number of rows and columns) of the verification
    /// matrix.
    pub fn dimensions(&self) -> (usize, usize) {
        (self.rows, self.cols)
    }

    /// Returns the element `m_{i,j}` of the verification matrix.
    pub fn element(&self, i: usize, j: usize) -> Option<&G> {
        self.m.get(i).and_then(|bi| bi.get(j))
    }

    /// Returns true if and only if `M_{0,0}` is the identity element
    /// of the group.
    pub fn is_zero_hole(&self) -> bool {
        self.m[0][0].is_identity().into()
    }

    /// Verifies whether the underlying bivariate polynomial evaluates
    /// to the given value, i.e., if it holds `B(x,y) == v`.
    pub fn verify(&self, x: &G::Scalar, y: &G::Scalar, v: &G::Scalar) -> bool {
        let mut diff = G::generator().neg() * v;
        let xpows = powers(x, self.rows - 1); // [x^i]
        let ypows = powers(y, self.cols - 1); // [y^j]
        for (i, xpow) in xpows.into_iter().enumerate() {
            for (j, ypow) in ypows.iter().enumerate() {
                diff += self.m[i][j] * (xpow * ypow); // x^i * y^j * M_{i,j} = b_{i,j} x^i * y^j * G
            }
        }

        diff.is_identity().into()
    }

    /// Returns a verification vector for the univariate polynomial resulting
    /// from the evaluation of the underlying bivariate polynomial `B(x,y)`
    /// at the given `y` value.
    pub fn verification_vector_for_x(&self, y: &G::Scalar) -> VerificationVector<G> {
        let mut v = Vec::with_capacity(self.rows);
        let ypows = powers(y, self.cols - 1); // [y^i]
        for i in 0..self.rows {
            let mut vi = G::identity();
            for (j, ypow) in ypows.iter().enumerate() {
                vi += self.m[i][j] * ypow;
            }
            v.push(vi);
        }

        VerificationVector::new(v)
    }

    /// Returns a verification vector for the univariate polynomial resulting
    /// from the evaluation of the underlying bivariate polynomial `B(x,y)`
    /// at the given `x` value.
    pub fn verification_vector_for_y(&self, x: &G::Scalar) -> VerificationVector<G> {
        let mut v = Vec::with_capacity(self.cols);
        let xpows = powers(x, self.rows - 1); // [x^i]
        for j in 0..self.cols {
            let mut vj = G::identity();
            for (i, xpow) in xpows.iter().enumerate() {
                vj += self.m[i][j] * xpow;
            }
            v.push(vj);
        }

        VerificationVector::new(v)
    }

    /// Verifies coefficients of the polynomial resulting from the evaluation
    /// of the bivariate polynomial with respect to the indeterminate x against
    /// the verification matrix.
    ///
    /// The polynomial
    /// ```text
    /// A(y) = \sum_{j=0}^{deg_y} a_j y^j
    /// ```
    /// where
    /// ```text
    /// a_j = \sum_{i=0}^{deg_x} b_{i,j} x^i
    /// ```
    /// is valid iff the following holds:
    /// ```text
    /// a_j * G = \sum_{i=0}^{deg_x} b_{i,j} x^i * G
    ///         = \sum_{i=0}^{deg_x} x^i * M_{i,j}
    /// ```
    ///
    /// This method is not constant time if the size of the polynomial
    /// is invalid.
    pub fn verify_x(&self, x: &G::Scalar, polynomial: &Polynomial<G::Scalar>) -> bool {
        // Short-circuit on the size of the polynomial, not its contents.
        if polynomial.size() != self.cols {
            return false;
        }

        // Don't short-circuit this loop to avoid revealing which coefficient
        // failed to verify.
        let xpows = powers(x, self.rows - 1); // [x^i]
        let mut verified = Choice::from(1);

        for j in 0..self.cols {
            // Verify if the following difference is the identity element (zero)
            // of the group: a_j * G - \sum_{i=0}^{deg_x} x^i * M_{i,j}.
            let aj = polynomial.coefficient(j).expect("size checked above");
            let mut diff = G::generator() * aj; // a_j * G
            for (i, xpow) in xpows.iter().enumerate() {
                diff -= self.m[i][j] * xpow; // x^i * M_{i,j} = b_{i,j} x^i * G
            }

            verified &= diff.is_identity();
        }

        verified.into()
    }

    /// Verifies coefficients of the polynomial resulting from the evaluation
    /// of the bivariate polynomial with respect to the indeterminate y against
    /// the verification matrix in non-constant time.
    ///
    ///
    /// The polynomial
    /// ```text
    /// A(x) = \sum_{i=0}^{deg_x} a_i y^i
    /// ```
    /// where
    /// ```text
    /// a_i = \sum_{j=0}^{deg_y} b_{i,j} y^j
    /// ```
    /// is valid iff the following holds:
    /// ```text
    /// a_i * G = \sum_{j=0}^{deg_y} b_{i,j} y^j * G
    ///         = \sum_{j=0}^{deg_y} y^j * M_{i,j}
    /// ```
    ///
    /// This method is not constant time if the size of the polynomial
    /// is invalid.
    pub fn verify_y(&self, y: &G::Scalar, polynomial: &Polynomial<G::Scalar>) -> bool {
        // Short-circuit on the size of the polynomial, not its contents.
        if polynomial.size() != self.rows {
            return false;
        }

        // Don't short-circuit this loop to avoid revealing which coefficient
        // failed to verify.
        let ypows = powers(y, self.cols - 1); // [y^j]
        let mut verified = Choice::from(1);

        for i in 0..self.rows {
            // Verify if the following difference is the identity element (zero)
            // of the group: a_i * G - \sum_{j=0}^{deg_y} y^j * M_{i,j}.
            let ai = polynomial.coefficient(i).expect("size checked above");
            let mut diff = G::generator() * ai; // a_i * G
            for (j, ypow) in ypows.iter().enumerate() {
                diff -= self.m[i][j] * ypow; // y^j * M_{i,j} = b_{i,j} y^j * G
            }

            verified &= diff.is_identity();
        }

        verified.into()
    }
}

impl<G> VerificationMatrix<G>
where
    G: Group + GroupEncoding,
{
    /// Returns the byte representation of the verification matrix.
    pub fn to_bytes(&self) -> Vec<u8> {
        let cap = Self::byte_size(self.rows, self.cols);
        let mut bytes = Vec::with_capacity(cap);
        let deg_x = (self.rows - 1) as u8;
        let deg_y = (self.cols - 1) as u8;
        bytes.extend([deg_x, deg_y].iter());
        for mi in &self.m {
            for mij in mi {
                bytes.extend_from_slice(mij.to_bytes().as_ref());
            }
        }

        bytes
    }

    /// Attempts to create a verification matrix from its byte representation.
    ///
    /// This method is not constant time since the verification matrix doesn't
    /// contain sensitive information.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 2 {
            return None;
        }

        let deg_x = bytes[0] as usize;
        let deg_y = bytes[1] as usize;
        let rows = deg_x + 1;
        let cols = deg_y + 1;
        let expected_len = Self::byte_size(rows, cols);

        if bytes.len() != expected_len {
            return None;
        }

        let element_size = Self::element_byte_size();
        let mut m = Vec::with_capacity(rows);

        for chunks in bytes[2..].chunks(element_size * cols) {
            let mut mi = Vec::with_capacity(cols);

            for chunk in chunks.chunks(element_size) {
                let mut repr: G::Repr = Default::default();
                repr.as_mut().copy_from_slice(chunk);

                #[allow(clippy::question_mark)]
                let mij = match G::from_bytes(&repr).into() {
                    None => return None,
                    Some(mij) => mij,
                };

                mi.push(mij);
            }
            m.push(mi);
        }

        Some(Self { cols, rows, m })
    }

    /// Returns the size of the byte representation of a matrix element.
    pub fn element_byte_size() -> usize {
        // Is there a better way?
        G::Repr::default().as_ref().len()
    }

    /// Returns the size of the byte representation of the verification matrix.
    pub fn byte_size(rows: usize, cols: usize) -> usize {
        2 + rows * cols * Self::element_byte_size()
    }
}

impl<G> From<&BivariatePolynomial<G::Scalar>> for VerificationMatrix<G>
where
    G: Group,
{
    /// Constructs a new verification matrix from the given bivariate
    /// polynomial.
    fn from(bp: &BivariatePolynomial<G::Scalar>) -> Self {
        let rows = bp.deg_x + 1;
        let cols = bp.deg_y + 1;
        let mut m = Vec::new();
        for bi in bp.b.iter() {
            let mut mi = Vec::new();
            for bij in bi.iter() {
                mi.push(G::generator() * bij) // b_{i,j} * G
            }
            m.push(mi)
        }

        Self { rows, cols, m }
    }
}

impl<G> From<BivariatePolynomial<G::Scalar>> for VerificationMatrix<G>
where
    G: Group,
{
    /// Constructs a new verification matrix from the given bivariate
    /// polynomial.
    fn from(bp: BivariatePolynomial<G::Scalar>) -> Self {
        (&bp).into()
    }
}

impl<G> Add for VerificationMatrix<G>
where
    G: Group,
{
    type Output = VerificationMatrix<G>;

    #[inline]
    fn add(self, rhs: Self) -> VerificationMatrix<G> {
        &self + &rhs
    }
}

impl<G> Add<&VerificationMatrix<G>> for VerificationMatrix<G>
where
    G: Group,
{
    type Output = VerificationMatrix<G>;

    #[inline]
    fn add(self, rhs: &VerificationMatrix<G>) -> VerificationMatrix<G> {
        &self + rhs
    }
}

impl<G> Add<VerificationMatrix<G>> for &VerificationMatrix<G>
where
    G: Group,
{
    type Output = VerificationMatrix<G>;

    #[inline]
    fn add(self, rhs: VerificationMatrix<G>) -> VerificationMatrix<G> {
        self + &rhs
    }
}

impl<G> Add for &VerificationMatrix<G>
where
    G: Group,
{
    type Output = VerificationMatrix<G>;

    fn add(self, rhs: Self) -> Self::Output {
        let rows = max(self.rows, rhs.rows);
        let cols = max(self.cols, rhs.cols);
        let mut m = Vec::with_capacity(rows);

        for i in 0..rows {
            let mut mi = Vec::with_capacity(cols);

            for j in 0..cols {
                let a = self.m.get(i).and_then(|mi| mi.get(j));
                let b = rhs.m.get(i).and_then(|mi| mi.get(j));

                let s = match (a, b) {
                    (Some(a), Some(b)) => *a + *b,
                    (Some(a), None) => *a,
                    (None, Some(b)) => *b,
                    (None, None) => G::identity(),
                };

                mi.push(s);
            }

            m.push(mi);
        }

        VerificationMatrix { rows, cols, m }
    }
}

impl<G> AddAssign for VerificationMatrix<G>
where
    G: Group,
{
    #[inline]
    fn add_assign(&mut self, rhs: VerificationMatrix<G>) {
        *self += &rhs
    }
}

impl<G> AddAssign<&VerificationMatrix<G>> for VerificationMatrix<G>
where
    G: Group,
{
    fn add_assign(&mut self, rhs: &VerificationMatrix<G>) {
        if self.rows < rhs.rows || self.cols < rhs.cols {
            *self = &*self + rhs;
            return;
        }

        for i in 0..rhs.rows {
            for j in 0..rhs.cols {
                self.m[i][j] += rhs.m[i][j];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use group::Group as _;
    use rand::{rngs::StdRng, SeedableRng};

    use crate::{poly, vss};

    type PrimeField = p384::Scalar;
    type Group = p384::ProjectivePoint;
    type BivariatePolynomial = poly::BivariatePolynomial<PrimeField>;
    type VerificationMatrix = vss::VerificationMatrix<Group>;

    fn scalar(value: i64) -> PrimeField {
        scalars(&vec![value])[0]
    }

    fn scalars(values: &[i64]) -> Vec<PrimeField> {
        values
            .iter()
            .map(|&w| match w.is_negative() {
                false => PrimeField::from_u64(w as u64),
                true => PrimeField::from_u64(-w as u64).neg(),
            })
            .collect()
    }

    #[test]
    fn test_from() {
        // Two non-zero coefficients (fast).
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let mut bp = BivariatePolynomial::zero(2, 3);
        assert!(bp.set_coefficient(0, 0, PrimeField::ONE));
        assert!(bp.set_coefficient(1, 2, PrimeField::ONE.double()));

        let vm = VerificationMatrix::from(&bp);
        assert_eq!(vm.m.len(), 3);
        for (i, mi) in vm.m.iter().enumerate() {
            assert_eq!(mi.len(), 4);
            for (j, mij) in mi.iter().enumerate() {
                match (i, j) {
                    (0, 0) => assert_eq!(mij, &Group::GENERATOR),
                    (1, 2) => assert_eq!(mij, &Group::GENERATOR.double()),
                    _ => assert_eq!(mij, &Group::IDENTITY),
                }
            }
        }

        // Random bivariate polynomial (slow).
        let bp = BivariatePolynomial::random(5, 10, &mut rng);
        let _ = VerificationMatrix::from(&bp);
    }

    #[test]
    fn test_dimensions() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let bp = BivariatePolynomial::random(5, 10, &mut rng);

        let vm = VerificationMatrix::from(&bp);
        assert_eq!((6, 11), vm.dimensions());
    }

    #[test]
    fn test_element() {
        let c = scalar(42);
        let e = Group::GENERATOR * c;

        let mut bp = BivariatePolynomial::zero(2, 3);
        assert!(bp.set_coefficient(1, 2, c));

        let vm = VerificationMatrix::from(&bp);
        assert_eq!(&e, vm.element(1, 2).unwrap());
    }

    #[test]
    fn test_verify() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let x2 = scalar(2);
        let x3 = scalar(3);

        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let s = bp.eval_x(&x2).eval(&x3);
        let vm = VerificationMatrix::from(&bp);
        assert!(vm.verify(&x2, &x3, &s));
        assert!(!vm.verify(&x3, &x2, &s));
    }

    #[test]
    fn test_verification_polynomial_for_x() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let x2 = scalar(2);
        let x3 = scalar(3);

        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let vm = VerificationMatrix::from(&bp);
        let p = bp.eval_y(&x2);

        let vv = vm.verification_vector_for_x(&x2);
        assert!(vv.is_from(&p));

        let vv = vm.verification_vector_for_x(&x3);
        assert!(!vv.is_from(&p));
    }

    #[test]
    fn test_verification_polynomial_for_y() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let x2 = scalar(2);
        let x3 = scalar(3);

        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let vm = VerificationMatrix::from(&bp);
        let p = bp.eval_x(&x2);

        let vv = vm.verification_vector_for_y(&x2);
        assert!(vv.is_from(&p));

        let vv = vm.verification_vector_for_y(&x3);
        assert!(!vv.is_from(&p));
    }

    #[test]
    fn test_verify_x() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let x2 = scalar(2);

        // Asymmetric bivariate polynomial.
        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let p = bp.eval_x(&x2);
        let vm = VerificationMatrix::from(&bp);
        assert!(vm.verify_x(&x2, &p));
        assert!(!vm.verify_y(&x2, &p)); // Invalid degree.

        // Symmetric bivariate polynomial.
        let bp = BivariatePolynomial::random(2, 2, &mut rng);
        let p = bp.eval_x(&x2);
        let vm = VerificationMatrix::from(&bp);
        assert!(vm.verify_x(&x2, &p));
        assert!(!vm.verify_y(&x2, &p)); // Valid degree, but verification failed.
    }

    #[test]
    fn test_verify_y() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let y2 = scalar(2);

        // Asymmetric bivariate polynomial.
        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let p = bp.eval_y(&y2);
        let vm = VerificationMatrix::from(&bp);
        assert!(!vm.verify_x(&y2, &p)); // Invalid degree.
        assert!(vm.verify_y(&y2, &p));

        // Symmetric bivariate polynomial.
        let bp = BivariatePolynomial::random(2, 2, &mut rng);
        let p = bp.eval_y(&y2);
        let vm = VerificationMatrix::from(&bp);
        assert!(!vm.verify_x(&y2, &p)); // Valid degree, but verification failed.
        assert!(vm.verify_y(&y2, &p));
    }

    #[test]
    fn test_serialization() {
        let mut rng: StdRng = SeedableRng::from_seed([1u8; 32]);
        let bp = BivariatePolynomial::random(2, 3, &mut rng);
        let vm = VerificationMatrix::from(&bp);
        let restored =
            VerificationMatrix::from_bytes(&vm.to_bytes()).expect("deserialization should succeed");

        assert_eq!(vm, restored);
    }

    #[test]
    fn test_element_byte_size() {
        let size = VerificationMatrix::element_byte_size();
        assert_eq!(size, 49);
    }

    #[test]
    fn test_byte_size() {
        let size = VerificationMatrix::byte_size(2, 3);
        assert_eq!(size, 2 + 2 * 3 * 49);
    }

    #[test]
    pub fn test_add() {
        let test_cases = vec![
            // Same size.
            (
                vec![scalars(&[0, 1, 2]), scalars(&[3, 4, 5])],
                vec![scalars(&[1, 3, 5]), scalars(&[0, 2, 4])],
                vec![scalars(&[1, 4, 7]), scalars(&[3, 6, 9])],
            ),
            // LHS smaller.
            (
                vec![scalars(&[0, 1]), scalars(&[3, 4])],
                vec![scalars(&[1, 3, 5]), scalars(&[0, 2, 4])],
                vec![scalars(&[1, 4, 5]), scalars(&[3, 6, 4])],
            ),
            // RHS smaller.
            (
                vec![scalars(&[0, 1, 2]), scalars(&[3, 4, 5])],
                vec![scalars(&[1, 3]), scalars(&[0, 2])],
                vec![scalars(&[1, 4, 2]), scalars(&[3, 6, 5])],
            ),
            // Mixed size.
            (
                vec![scalars(&[1, 2, 3, 4]), scalars(&[5, 6, 7, 8])],
                vec![scalars(&[1, 2]), scalars(&[3, 4]), scalars(&[5, 6])],
                vec![
                    scalars(&[2, 4, 3, 4]),
                    scalars(&[8, 10, 7, 8]),
                    scalars(&[5, 6, 0, 0]),
                ],
            ),
        ];

        for (c1, c2, c3) in test_cases {
            let bp1 = BivariatePolynomial::with_coefficients(c1);
            let bp2 = BivariatePolynomial::with_coefficients(c2);
            let bp3 = BivariatePolynomial::with_coefficients(c3);
            let vm1 = VerificationMatrix::from(&bp1);
            let vm2 = VerificationMatrix::from(&bp2);
            let vm3 = VerificationMatrix::from(&bp3);

            // Test add.
            let sum = vm1.clone() + vm2.clone();
            assert_eq!(sum, vm3);

            let sum = vm1.clone() + &vm2.clone();
            assert_eq!(sum, vm3);

            let sum = &vm1.clone() + vm2.clone();
            assert_eq!(sum, vm3);

            let sum = &vm1.clone() + &vm2.clone();
            assert_eq!(sum, vm3);

            // Test add assign.
            let mut sum = vm1.clone();
            sum += vm2.clone();
            assert_eq!(sum, vm3);

            let mut sum = vm1.clone();
            sum += &vm2.clone();
            assert_eq!(sum, vm3);
        }
    }
}
