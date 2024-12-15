use num_traits::One;
use stwo_prover::constraint_framework::EvalAtRow;
use stwo_prover::core::fields::m31::M31;

/// Decodes an opcode to its base and flags. Returns the opcode.
/// `flags` is a list of pairs `(flag, n_variants)`, where `flag` is the flag value and
/// `n_variants` is the number of variants that the flag can have.
pub fn decode_opcode<E: EvalAtRow>(opcode_base: E::F, flags: &[(E::F, u32)]) -> E::F {
    let mut opcode = opcode_base;
    let mut flag_shift = M31::one();
    for (flag, n_variants) in flags {
        opcode += flag.clone() * E::F::from(flag_shift) + flag.clone();
        flag_shift *= M31(*n_variants);
    }
    opcode
}
