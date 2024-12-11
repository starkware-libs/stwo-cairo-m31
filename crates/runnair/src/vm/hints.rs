use serde::{Deserialize, Serialize};

use crate::memory::relocatable::{MaybeRelocatable, Relocatable};
use crate::memory::Memory;
use crate::utils::{qm31_from_hex_str_array, usize_from_u32};
use crate::vm::{Input, State};

// TODO: add custom (de)serialization.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Hint {
    FibonacciIndex,
}

impl Hint {
    fn execute(&self, memory: &mut Memory, state: &State, input: &Input) {
        match self {
            Self::FibonacciIndex => {
                let index =
                    Deserialize::deserialize(input.get("fibonacci_claim_index").unwrap()).unwrap();
                memory.insert(state.fp, qm31_from_hex_str_array(index));
            }
        }
    }
}

pub(crate) type Hints = Vec<Option<Hint>>;

#[derive(Debug)]
pub(crate) struct HintRunner {
    pc_to_hint: Hints,
    input: Input,
}

impl HintRunner {
    pub(crate) fn new(pc_to_hint: Hints, input: Input) -> Self {
        Self { pc_to_hint, input }
    }

    pub(crate) fn maybe_execute_hint(&self, memory: &mut Memory, state: &State) {
        let MaybeRelocatable::Relocatable(Relocatable {
            segment: _,
            offset: pc,
        }) = state.pc
        else {
            panic!("`pc` must be a relocatable value.");
        };

        let pc = usize_from_u32(pc.0);
        if let Some(Some(hint)) = self.pc_to_hint.get(pc) {
            hint.execute(memory, state, &self.input);
        }
    }
}
