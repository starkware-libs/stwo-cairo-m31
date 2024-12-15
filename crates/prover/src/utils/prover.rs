use std::simd::Simd;

use num_traits::Zero;
use stwo_prover::core::backend::simd::m31::PackedM31;
use stwo_prover::core::fields::m31::M31;

pub fn divmod(x: PackedM31, divisor: u32) -> (PackedM31, PackedM31) {
    unsafe {
        let simd_x = x.into_simd();
        (
            PackedM31::from_simd_unchecked(simd_x / Simd::splat(divisor)),
            PackedM31::from_simd_unchecked(simd_x % Simd::splat(divisor)),
        )
    }
}

pub fn decode_opcode<const N: usize>(
    opcode_base: M31,
    opcode: PackedM31,
    n_variants: [u32; N],
) -> [PackedM31; N] {
    let mut flags = opcode - PackedM31::broadcast(opcode_base);
    let res = std::array::from_fn(|i| {
        let (new_flags, flag) = divmod(flags, n_variants[i]);
        flags = new_flags;
        flag
    });
    assert!(flags.is_zero(), "Too many flags.");
    res
}
