use serde::{Deserialize, Serialize};

use super::mem::MemoryBuilder;
use super::vm_import::TraceEntry;

// TODO(spapini): Move this:
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct VmState {
    pub pc: u32,
    pub ap: u32,
    pub fp: u32,
}
impl From<TraceEntry> for VmState {
    fn from(entry: TraceEntry) -> Self {
        Self {
            pc: entry.pc as u32,
            ap: entry.ap as u32,
            fp: entry.fp as u32,
        }
    }
}

// TODO(yuval/alonT): consider making the indexing mechanism more explicit in the code).
/// The instructions usage in the input, split to Stwo opcodes.
///
/// For each opcode with flags, the array describes the different flag combinations. The index
/// refers to the flag combination in bit-reverse/little-endian. For example, jnz_imm at index 1
/// (100 in bit-reverse/little-endian) is for: fp (1=true), not taken (0=false), no ap++ (0=false).
/// Note: for the flag "fp/ap", true means fp-based and false means ap-based.
#[derive(Debug, Default)]
pub struct Instructions {
    pub initial_state: VmState,
    pub final_state: VmState,

    pub addap_jmp: Vec<VmState>,
}
impl Instructions {
    pub fn from_iter(mut iter: impl Iterator<Item = TraceEntry>, mem: &mut MemoryBuilder) -> Self {
        let mut res = Self::default();

        let Some(first) = iter.next() else {
            return res;
        };
        res.initial_state = first.into();
        res.push_instr(mem, first.into());

        for entry in iter {
            res.final_state = entry.into();
            res.push_instr(mem, entry.into());
        }
        res
    }

    #[allow(unused)]
    fn push_instr(&mut self, mem: &mut MemoryBuilder, state: VmState) {
        let VmState { pc, .. } = state;
        let instruction = mem.get_inst(pc);
        // Decode.
        todo!()
    }

    pub fn counts(&self) -> InstructionCounts {
        todo!()
    }
}

/// The counts of the instructions usage in the input, split to Stwo opcodes.
///
/// See the documentation of `Instructions` for more details about the indexing mechanism.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct InstructionCounts {}
