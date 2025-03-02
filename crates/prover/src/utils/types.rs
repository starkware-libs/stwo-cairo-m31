use stwo_prover::core::backend::simd::conversion::{Pack, Unpack};
use stwo_prover::core::backend::simd::m31::{PackedM31, N_LANES};
use stwo_prover::core::fields::m31::M31;

// TODO(Ohad): take from prover_types and remove.
#[derive(Debug, Clone, Copy)]
pub struct CasmState {
    pub pc: M31,
    pub ap: M31,
    pub fp: M31,
}

// TODO(Ohad): take from prover_types and remove.
#[derive(Debug, Clone, Copy)]
pub struct PackedCasmState {
    pub pc: PackedM31,
    pub ap: PackedM31,
    pub fp: PackedM31,
}

impl Pack for CasmState {
    type SimdType = PackedCasmState;

    fn pack(inputs: [Self; N_LANES]) -> Self::SimdType {
        PackedCasmState {
            pc: PackedM31::from_array(std::array::from_fn(|i| inputs[i].pc)),
            ap: PackedM31::from_array(std::array::from_fn(|i| inputs[i].ap)),
            fp: PackedM31::from_array(std::array::from_fn(|i| inputs[i].fp)),
        }
    }
}

impl Unpack for PackedCasmState {
    type CpuType = CasmState;

    fn unpack(self) -> [Self::CpuType; N_LANES] {
        let (pc, ap, fp) = (self.pc.to_array(), self.ap.to_array(), self.fp.to_array());
        std::array::from_fn(|i| CasmState {
            pc: pc[i],
            ap: ap[i],
            fp: fp[i],
        })
    }
}
