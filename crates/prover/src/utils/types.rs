use serde::{Deserialize, Serialize};
use stwo_prover::core::backend::simd::m31::PackedM31;

use crate::input::vm_import::TraceEntry;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct CasmState {
    pub pc: u32,
    pub ap: u32,
    pub fp: u32,
}
impl From<TraceEntry> for CasmState {
    fn from(entry: TraceEntry) -> Self {
        Self {
            pc: entry.pc as u32,
            ap: entry.ap as u32,
            fp: entry.fp as u32,
        }
    }
}

#[derive(Debug, Clone)]
pub struct PackedCasmState {
    pub pc: PackedM31,
    pub ap: PackedM31,
    pub fp: PackedM31,
}
