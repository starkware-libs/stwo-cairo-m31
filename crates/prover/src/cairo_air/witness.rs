use std::mem::transmute;

use itertools::Itertools;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::constraints::{CairoClaim, CairoInteractionClaim, CairoRelationElements};
use crate::components::{addap_jmpabs_jmprel_opcode, memory};
use crate::input::instructions::VmState;
use crate::input::CairoInput;
use crate::utils::types::CasmState;

pub struct CairoWitnessGen {
    input: CairoInput,
    addap_jmp: Option<addap_jmpabs_jmprel_opcode::ClaimGenerator>,
    memory: memory::ClaimGenerator,
    //
}
impl CairoWitnessGen {
    pub fn new(input: CairoInput) -> Self {
        Self {
            addap_jmp: addap_jmpabs_jmprel_opcode::ClaimGenerator::new(unsafe {
                transmute::<Vec<VmState>, Vec<CasmState>>(input.instructions.addap_jmp.clone())
            }),
            memory: memory::ClaimGenerator::new(&input.memory),
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
            .map(|a| (M31(a), self.input.memory.get(a).0))
            .collect_vec();

        // Add public memory.
        // TODO(ShaharS): fix the use of public memory to support memory ids.
        for addr in &self.input.public_mem_addresses {
            self.memory.add_inputs(*addr as usize);
        }

        let (addap_jmp_claim, addap_jmp_interaction) = self
            .addap_jmp
            .map(|c| c.write_trace(tree_builder, &mut self.memory))
            .unzip();
        let (memory_claim, memory_interaction) = self.memory.write_trace(tree_builder);
        (
            CairoClaim {
                public_memory,
                initial_state,
                final_state,
                addap_jmp: addap_jmp_claim,
                memory: memory_claim,
            },
            CairoInteractionGen {
                addap_jmp: addap_jmp_interaction,
                memory: memory_interaction,
            },
        )
    }
}

pub struct CairoInteractionGen {
    addap_jmp: Option<addap_jmpabs_jmprel_opcode::InteractionClaimGenerator>,
    memory: memory::InteractionClaimGenerator,
}
impl CairoInteractionGen {
    pub fn write_interaction_trace(
        self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        interaction_elements: &CairoRelationElements,
    ) -> CairoInteractionClaim {
        let addap_jmp = self.addap_jmp.map(|c| {
            c.write_interaction_trace(
                tree_builder,
                &interaction_elements.memory,
                &interaction_elements.vm,
            )
        });
        let memory = self
            .memory
            .write_interaction_trace(tree_builder, &interaction_elements.memory);
        CairoInteractionClaim { addap_jmp, memory }
    }
}
