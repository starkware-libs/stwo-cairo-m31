use std::simd::Simd;

use num_traits::{One, Zero};
use stwo_prover::core::backend::simd::m31::{PackedM31, N_LANES};
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

/// The enabler column is a column of length `padding_offset.next_power_of_two()` where
/// 1. The first `padding_offset` elements are set to 1;
/// 2. The rest are set to 0.
#[derive(Debug, Clone)]
pub struct Enabler {
    pub padding_offset: usize,
}
impl Enabler {
    pub const fn new(padding_offset: usize) -> Self {
        Self { padding_offset }
    }

    pub fn packed_at(&self, vec_row: usize) -> PackedM31 {
        let row_offset = vec_row * N_LANES;
        if self.padding_offset <= row_offset {
            return PackedM31::zero();
        }
        if self.padding_offset >= row_offset + N_LANES {
            return PackedM31::one();
        }

        // The row is partially enabled.
        let mut res = [M31::zero(); N_LANES];
        for v in res.iter_mut().take(self.padding_offset - row_offset) {
            *v = M31::one();
        }
        PackedM31::from_array(res)
    }
}

#[cfg(test)]
mod tests {
    use num_traits::Zero;
    use stwo_prover::core::backend::simd::m31::PackedM31;
    use stwo_prover::core::fields::m31::M31;

    use crate::utils::prover::decode_opcode;

    #[test]
    fn test_prover_decode() {
        let base = M31(123);
        let flags = [M31(1), M31(2), M31(3), M31(2)].map(PackedM31::broadcast);

        let opcode = PackedM31::broadcast(base + M31(1 + (2 * 2) + (6 * 3) + (24 * 2)));

        assert!(decode_opcode(base, opcode, [2, 3, 4, 3])
            .into_iter()
            .zip(flags)
            .all(|(x, y)| (x - y).is_zero()));
    }
}
