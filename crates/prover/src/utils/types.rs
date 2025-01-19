use stwo_prover::core::backend::simd::m31::PackedM31;
use stwo_prover::core::fields::m31::M31;

// TODO(Ohad): take from prover_types and remove.
#[derive(Debug, Clone)]
pub struct CasmState {
    pub pc: M31,
    pub ap: M31,
    pub fp: M31,
}

// TODO(Ohad): take from prover_types and remove.
#[derive(Debug, Clone)]
pub struct PackedCasmState {
    pub pc: PackedM31,
    pub ap: PackedM31,
    pub fp: PackedM31,
}
