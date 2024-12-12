use num_traits::One;
use stwo_prover::constraint_framework::EvalAtRow;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::FieldExpOps;

pub(crate) fn get_bit_constraint<E: EvalAtRow>(bit: E::F) -> E::F {
    bit.clone() * bit.clone() - bit.clone()
}

pub(crate) fn get_trit_constraint<E: EvalAtRow>(trit: E::F) -> E::F {
    (trit.clone() - E::F::from(M31(2))) * (trit.clone() - E::F::one()) * trit.clone()
}

// TODO(alont) document this!!
pub(crate) fn select_trit<E: EvalAtRow>(trit: E::F, a: &E::F, b: &E::F, c: &E::F) -> E::F {
    let trit_minus_one = trit.clone() - E::F::one();
    let trit_minus_two = trit.clone() - E::F::from(M31(2));
    let two_inv = E::F::from(M31(2).inverse());

    (two_inv.clone() * trit_minus_one.clone() * trit_minus_two.clone() * a.clone())
        + (two_inv * trit.clone() * trit_minus_one * b.clone())
        - (trit * trit_minus_two * c.clone())
}
