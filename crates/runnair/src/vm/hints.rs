use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use stwo_prover::core::fields::m31::M31;

use crate::memory::relocatable::{MaybeRelocatable, Relocatable};
use crate::memory::Memory;
use crate::vm::{qm31_from_hex_str_array, Input, State};

// TODO: add custom (de)serialization.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum Hint {
    FibonacciIndex,
}

impl Hint {
    // TODO: reconsider input parsing.
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

#[derive(Debug)]
pub(crate) struct HintRunner {
    pc_to_hint: HashMap<M31, Hint>,
    input: Input,
}

impl HintRunner {
    pub(crate) fn new(pc_to_hint: HashMap<M31, Hint>, input: Input) -> Self {
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

        if let Some(hint) = self.pc_to_hint.get(&pc) {
            hint.execute(memory, state, &self.input);
        }
    }
}
