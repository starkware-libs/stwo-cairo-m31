use std::ops::{Add, Mul};

use num_traits::One;
use stwo_prover::constraint_framework::EvalAtRow;
use stwo_prover::core::backend::simd::m31::LOG_N_LANES;
use stwo_prover::core::fields::m31::{BaseField, M31};

/// Decodes an opcode to its base and flags. Returns the opcode.
/// `flags` is a list of pairs `(flag, n_variants)`, where `flag` is the flag value and
/// `n_variants` is the number of variants that the flag can have.
pub fn decode_opcode<T>(opcode_base: T, flags: &[(T, u32)]) -> T
where
    T: Clone + One + Mul<Output = T> + Add<Output = T> + From<M31>,
{
    let mut opcode = opcode_base;
    let mut flag_shift = M31::one();
    for (flag, n_variants) in flags {
        opcode = opcode + T::from(flag_shift) * flag.clone();
        flag_shift *= M31(*n_variants);
    }
    opcode
}

pub fn log_size(num: usize) -> u32 {
    std::cmp::max(num.next_power_of_two().ilog2(), LOG_N_LANES)
}

/// Create a constraint asserting that `flag` is a bit.
pub fn is_bit<E: EvalAtRow>(flag: &E::F) -> E::F {
    let f = || flag.clone();
    // f^2 - f
    f() * f() - f()
}

/// Create a constraint asserting that `flag` is a trit (a digit in {0,1,2}).
pub fn is_trit<E: EvalAtRow>(flag: &E::F) -> E::F {
    let two = E::F::from(BaseField::from_u32_unchecked(2));
    let three = E::F::from(BaseField::from_u32_unchecked(3));
    let f = || flag.clone();

    // is_trit(f) := (f - 2) * (f - 1) * (f)  ==expands into==>  f^3 - 3*f^2 + 2*f.
    f() * f() * f() - three * f() * f() + two * f()
}

#[cfg(test)]
mod tests {
    use stwo_prover::core::fields::m31::M31;

    use crate::utils::component::decode_opcode;

    #[test]
    fn test_component_decode() {
        let base = M31(123);
        let flags = [(M31(1), 2), (M31(2), 3), (M31(3), 4), (M31(2), 3)];

        let opcode = base + M31(1 + (2 * 2) + (6 * 3) + (24 * 2));

        assert_eq!(decode_opcode(base, &flags), opcode);
    }
}
