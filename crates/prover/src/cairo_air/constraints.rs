use itertools::chain;
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{Relation, TraceLocationAllocator, PREPROCESSED_TRACE_IDX};
use stwo_prover::core::air::{Component, ComponentProver};
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::{SecureField, QM31};
use stwo_prover::core::fields::FieldExpOps;
use stwo_prover::core::pcs::TreeVec;
use stwo_prover::core::prover::StarkProof;
use stwo_prover::core::vcs::ops::MerkleHasher;

use super::preprocessed::PreProcessedTrace;
use crate::components::{addap_jmpabs_jmprel_opcode, memory};
use crate::input::instructions::VmState;
use crate::relations::{MemoryRelation, StateRelation};

#[derive(Serialize, Deserialize)]
pub struct CairoProof<H: MerkleHasher> {
    pub claim: CairoClaim,
    pub interaction_claim: CairoInteractionClaim,
    pub stark_proof: StarkProof<H>,
}

#[derive(Serialize, Deserialize)]
pub struct CairoClaim {
    // Common claim values.
    pub public_memory: Vec<(M31, QM31)>,
    pub initial_state: VmState,
    pub final_state: VmState,

    pub addap_jmp: Option<addap_jmpabs_jmprel_opcode::Claim>,
    pub memory: memory::Claim,
    // pub ret: Vec<ret_opcode::Claim>,
    // ...
}

impl CairoClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        // TODO(spapini): Add common values.
        if let Some(claim) = self.addap_jmp {
            claim.mix_into(channel);
        }
        self.memory.mix_into(channel);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let mut log_sizes = TreeVec::concat_cols(chain!([
            self.addap_jmp
                .map_or_else(Default::default, |c| c.log_sizes()),
            self.memory.log_sizes()
        ],));
        log_sizes[PREPROCESSED_TRACE_IDX] = PreProcessedTrace::new().log_sizes();
        log_sizes
    }
}

pub struct CairoRelationElements {
    pub vm: StateRelation,
    pub memory: MemoryRelation,
    // ...
}
impl CairoRelationElements {
    pub fn draw(channel: &mut impl Channel) -> CairoRelationElements {
        CairoRelationElements {
            vm: StateRelation::draw(channel),
            memory: MemoryRelation::draw(channel),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct CairoInteractionClaim {
    pub addap_jmp: Option<addap_jmpabs_jmprel_opcode::InteractionClaim>,
    pub memory: memory::InteractionClaim,
    // pub ret: Vec<ret_opcode::InteractionClaim>,
    // ...
}

impl CairoInteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        // self.ret.iter().for_each(|c| c.mix_into(channel));
        self.memory.mix_into(channel);
    }
}

pub fn lookup_sum(
    claim: &CairoClaim,
    elements: &CairoRelationElements,
    interaction_claim: &CairoInteractionClaim,
) -> QM31 {
    let mut sum = QM31::zero();
    // Public memory.
    // TODO(spapini): Optimized inverse.
    sum += claim
        .public_memory
        .iter()
        .map(|(addr, val)| {
            let denom: SecureField = elements
                .memory
                .combine(&[[*addr].as_slice(), val.to_m31_array().as_slice()].concat());
            denom.inverse()
        })
        .sum::<SecureField>();
    // TODO: include initial and final state.
    if let Some(ref claim) = interaction_claim.addap_jmp {
        sum += claim.claimed_sum;
    }
    sum += interaction_claim.memory.claimed_sum;
    sum
}

pub struct CairoComponents {
    addap_jmp: Option<addap_jmpabs_jmprel_opcode::Component>,
    memory: memory::Component,
    // ...
}

impl CairoComponents {
    pub fn new(
        cairo_claim: &CairoClaim,
        relation_elements: &CairoRelationElements,
        interaction_claim: &CairoInteractionClaim,
    ) -> Self {
        let tree_span_provider = &mut TraceLocationAllocator::new_with_preproccessed_columns(
            &PreProcessedTrace::new().ids(),
        );

        let addap_jmp = cairo_claim.addap_jmp.map(|claim| {
            addap_jmpabs_jmprel_opcode::Component::new(
                tree_span_provider,
                addap_jmpabs_jmprel_opcode::Eval {
                    claim,
                    memory_relation_elemnts: relation_elements.memory.clone(),
                    state_lookup: relation_elements.vm.clone(),
                },
                interaction_claim.addap_jmp.as_ref().unwrap().claimed_sum,
            )
        });

        let memory = memory::Component::new(
            tree_span_provider,
            memory::Eval::new(
                cairo_claim.memory.clone(),
                relation_elements.memory.clone(),
                interaction_claim.memory.clone(),
            ),
            interaction_claim.memory.clone().claimed_sum,
        );
        Self { addap_jmp, memory }
    }

    pub fn provers(&self) -> Vec<&dyn ComponentProver<SimdBackend>> {
        let mut provers = vec![];
        if let Some(prover) = &self.addap_jmp {
            provers.push(prover as &dyn ComponentProver<_>);
        }
        provers.push(&self.memory);
        provers
    }

    pub fn components(&self) -> Vec<&dyn Component> {
        self.provers()
            .into_iter()
            .map(|component| component as &dyn Component)
            .collect()
    }
}
