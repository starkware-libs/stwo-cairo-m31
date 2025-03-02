use itertools::Itertools;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::constraints::{CairoClaim, CairoInteractionClaim, CairoRelations};
use crate::components::memory;
use crate::input::CairoInput;

pub struct CairoWitnessGen {
    input: CairoInput,
    memory: memory::ClaimGenerator,
}
impl CairoWitnessGen {
    pub fn new(input: CairoInput) -> Self {
        Self {
            memory: memory::ClaimGenerator::new(&input.mem),
            input,
        }
    }
    pub fn write_trace(
        mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
    ) -> (CairoClaim, CairoInteractionGen) {
        let (initial_state, final_state) = (
            self.input.instructions.initial_state,
            self.input.instructions.final_state,
        );
        // Extract public memory.
        let public_memory = self
            .input
            .public_mem_addresses
            .iter()
            .copied()
            .map(|a| (M31(a), self.input.mem.get(a).0))
            .collect_vec();

        // Add public memory.
        // TODO(ShaharS): fix the use of public memory to support memory ids.
        for addr in &self.input.public_mem_addresses {
            self.memory.add_inputs(*addr as usize);
        }

        let (memory_claim, memory_interaction) = self.memory.write_trace(tree_builder);
        (
            CairoClaim {
                public_memory,
                initial_state,
                final_state,
                memory: memory_claim,
            },
            CairoInteractionGen {
                memory: memory_interaction,
            },
        )
    }
}

pub struct CairoInteractionGen {
    memory: memory::InteractionClaimGenerator,
}
impl CairoInteractionGen {
    pub fn write_interaction_trace(
        self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        interaction_elements: &CairoRelations,
    ) -> CairoInteractionClaim {
        let addr_to_value = self
            .memory
            .write_interaction_trace(tree_builder, &interaction_elements.memory);
        CairoInteractionClaim { addr_to_value }
    }
}
