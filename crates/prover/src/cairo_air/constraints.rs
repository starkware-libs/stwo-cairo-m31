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
use crate::components::memory;
use crate::input::instructions::VmState;
use crate::relations::MemoryRelation;

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

    pub memory: memory::Claim,
    // pub ret: Vec<ret_opcode::Claim>,
    // ...
}

impl CairoClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        // TODO(spapini): Add common values.
        // self.ret.iter().for_each(|c| c.mix_into(channel));
        self.memory.mix_into(channel);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let mut log_sizes = TreeVec::concat_cols(chain!([self.memory.log_sizes()],));
        log_sizes[PREPROCESSED_TRACE_IDX] = PreProcessedTrace::new().log_sizes();
        log_sizes
    }
}

pub struct CairoRelations {
    pub memory: MemoryRelation,
    // ...
}
impl CairoRelations {
    pub fn draw(channel: &mut impl Channel) -> CairoRelations {
        CairoRelations {
            memory: MemoryRelation::draw(channel),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct CairoInteractionClaim {
    pub addr_to_value: memory::InteractionClaim,
    // pub ret: Vec<ret_opcode::InteractionClaim>,
    // ...
}

impl CairoInteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        // self.ret.iter().for_each(|c| c.mix_into(channel));
        self.addr_to_value.mix_into(channel);
    }
}

pub fn lookup_sum_valid(
    claim: &CairoClaim,
    elements: &CairoRelations,
    interaction_claim: &CairoInteractionClaim,
) -> bool {
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
    // sum += interaction_claim.ret[0].logup_sums.1.unwrap().0;
    sum += interaction_claim.addr_to_value.claimed_sum;
    sum == SecureField::zero()
}

pub struct CairoComponents {
    memory: memory::Component,
    // ret: Vec<ret_opcode::Component>,
    // ...
}

impl CairoComponents {
    pub fn new(
        cairo_claim: &CairoClaim,
        interaction_elements: &CairoRelations,
        interaction_claim: &CairoInteractionClaim,
    ) -> Self {
        let tree_span_provider = &mut TraceLocationAllocator::new_with_preproccessed_columns(
            &PreProcessedTrace::new().ids(),
        );

        let addr_to_value_component = memory::Component::new(
            tree_span_provider,
            memory::Eval::new(
                cairo_claim.memory.clone(),
                interaction_elements.memory.clone(),
                interaction_claim.addr_to_value.clone(),
            ),
            interaction_claim.addr_to_value.clone().claimed_sum,
        );
        Self {
            // ret: ret_components,
            memory: addr_to_value_component,
        }
    }

    pub fn provers(&self) -> Vec<&dyn ComponentProver<SimdBackend>> {
        vec![&self.memory]
    }

    pub fn components(&self) -> Vec<&dyn Component> {
        self.provers()
            .into_iter()
            .map(|component| component as &dyn Component)
            .collect()
    }
}
