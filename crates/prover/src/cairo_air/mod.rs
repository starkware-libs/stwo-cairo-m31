use itertools::{chain, Itertools};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::preprocessed_columns::gen_is_first;
use stwo_prover::constraint_framework::{Relation, TraceLocationAllocator};
use stwo_prover::core::air::{Component, ComponentProver};
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::channel::{Blake2sChannel, Channel};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::{SecureField, QM31};
use stwo_prover::core::fields::FieldExpOps;
use stwo_prover::core::pcs::{
    CommitmentSchemeProver, CommitmentSchemeVerifier, PcsConfig, TreeVec,
};
use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};
use stwo_prover::core::prover::{prove, verify, ProvingError, StarkProof, VerificationError};
use stwo_prover::core::vcs::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use stwo_prover::core::vcs::ops::MerkleHasher;
use thiserror::Error;
use tracing::{span, Level};

use crate::components::memory::addr_to_f31;
use crate::components::ret_opcode;
use crate::input::instructions::VmState;
use crate::input::CairoInput;

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

    pub ret: Vec<ret_opcode::Claim>,
    pub memory_id_to_value: addr_to_f31::Claim,
    // ...
}

impl CairoClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        // TODO(spapini): Add common values.
        self.ret.iter().for_each(|c| c.mix_into(channel));
        self.memory_id_to_value.mix_into(channel);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        TreeVec::concat_cols(chain!(
            self.ret.iter().map(|c| c.log_sizes()),
            [self.memory_id_to_value.log_sizes()],
        ))
    }
}

pub struct CairoInteractionElements {
    pub memory_id_to_value_lookup: addr_to_f31::MemoryRelation,
    // ...
}
impl CairoInteractionElements {
    pub fn draw(channel: &mut impl Channel) -> CairoInteractionElements {
        CairoInteractionElements {
            memory_id_to_value_lookup: addr_to_f31::MemoryRelation::draw(channel),
        }
    }
}

#[derive(Serialize, Deserialize)]
pub struct CairoInteractionClaim {
    pub ret: Vec<ret_opcode::InteractionClaim>,
    pub memory_id_to_value: addr_to_f31::InteractionClaim,
    // ...
}

impl CairoInteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        self.ret.iter().for_each(|c| c.mix_into(channel));
        self.memory_id_to_value.mix_into(channel);
    }
}

pub fn lookup_sum_valid(
    claim: &CairoClaim,
    elements: &CairoInteractionElements,
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
                .memory_id_to_value_lookup
                .combine(&[[*addr].as_slice(), val.to_m31_array().as_slice()].concat());
            denom.inverse()
        })
        .sum::<SecureField>();
    // TODO: include initial and final state.
    sum += interaction_claim.ret[0].claimed_sum;
    sum += interaction_claim.memory_id_to_value.claimed_sum;
    sum == SecureField::zero()
}

pub struct CairoComponents {
    ret: Vec<ret_opcode::Component>,
    memory_id_to_value: addr_to_f31::Component,
    // ...
}

impl CairoComponents {
    pub fn new(
        cairo_claim: &CairoClaim,
        interaction_elements: &CairoInteractionElements,
        interaction_claim: &CairoInteractionClaim,
    ) -> Self {
        let tree_span_provider = &mut TraceLocationAllocator::default();

        let ret_components = cairo_claim
            .ret
            .iter()
            .zip(interaction_claim.ret.iter())
            .map(|(claim, interaction_claim)| {
                ret_opcode::Component::new(
                    tree_span_provider,
                    ret_opcode::Eval::new(
                        claim.clone(),
                        interaction_elements.memory_id_to_value_lookup.clone(),
                        interaction_claim.clone(),
                    ),
                    (interaction_claim.claimed_sum, None),
                )
            })
            .collect_vec();

        let memory_id_to_value_component = addr_to_f31::Component::new(
            tree_span_provider,
            addr_to_f31::Eval::new(
                cairo_claim.memory_id_to_value.clone(),
                interaction_elements.memory_id_to_value_lookup.clone(),
                interaction_claim.memory_id_to_value.clone(),
            ),
            (
                interaction_claim.memory_id_to_value.clone().claimed_sum,
                None,
            ),
        );
        Self {
            ret: ret_components,
            memory_id_to_value: memory_id_to_value_component,
        }
    }

    pub fn provers(&self) -> Vec<&dyn ComponentProver<SimdBackend>> {
        let mut vec: Vec<&dyn ComponentProver<SimdBackend>> = vec![];
        for ret in self.ret.iter() {
            vec.push(ret);
        }
        vec.push(&self.memory_id_to_value);
        vec
    }

    pub fn components(&self) -> Vec<&dyn Component> {
        let mut vec: Vec<&dyn Component> = vec![];
        for ret in self.ret.iter() {
            vec.push(ret);
        }
        vec.push(&self.memory_id_to_value);
        vec
    }
}

const LOG_MAX_ROWS: u32 = 20;
pub fn prove_cairo(input: CairoInput) -> Result<CairoProof<Blake2sMerkleHasher>, ProvingError> {
    let _span = span!(Level::INFO, "prove_cairo").entered();
    let config = PcsConfig::default();
    let twiddles = SimdBackend::precompute_twiddles(
        CanonicCoset::new(LOG_MAX_ROWS + config.fri_config.log_blowup_factor + 2)
            .circle_domain()
            .half_coset,
    );

    // Setup protocol.
    let channel = &mut Blake2sChannel::default();
    let mut commitment_scheme = CommitmentSchemeProver::new(config, &twiddles);

    // Extract public memory.
    let public_memory = input
        .public_mem_addresses
        .iter()
        .copied()
        .map(|a| (M31::from(a), input.mem.get(a).0))
        .collect_vec();

    // TODO: Table interaction.

    // Base trace.
    // TODO(Ohad): change to OpcodeClaimProvers, and integrate padding.
    let ret_trace_generator = ret_opcode::ClaimGenerator::new(input.instructions.ret);
    let mut memory_id_to_value_trace_generator = addr_to_f31::ClaimGenerator::new(&input.mem);

    // Add public memory.
    // TODO(ShaharS): fix the use of public memory to support memory ids.
    for addr in &input.public_mem_addresses {
        memory_id_to_value_trace_generator.add_inputs(*addr as usize);
    }

    let mut tree_builder = commitment_scheme.tree_builder();

    let (ret_claim, ret_interaction_prover) =
        ret_trace_generator.write_trace(&mut tree_builder, &mut memory_id_to_value_trace_generator);
    let (memory_id_to_value_claim, memory_id_to_value_interaction_prover) =
        memory_id_to_value_trace_generator.write_trace(&mut tree_builder);
    // Commit to the claim and the trace.
    let claim = CairoClaim {
        public_memory,
        initial_state: input.instructions.initial_state,
        final_state: input.instructions.final_state,
        ret: vec![ret_claim],
        memory_id_to_value: memory_id_to_value_claim.clone(),
    };
    claim.mix_into(channel);
    tree_builder.commit(channel);

    // Draw interaction elements.
    let interaction_elements = CairoInteractionElements::draw(channel);

    // Interaction trace.
    let mut tree_builder = commitment_scheme.tree_builder();
    let ret_interaction_claim = ret_interaction_prover.write_interaction_trace(
        &mut tree_builder,
        &interaction_elements.memory_id_to_value_lookup,
    );
    let memory_id_to_value_interaction_claim = memory_id_to_value_interaction_prover
        .write_interaction_trace(
            &mut tree_builder,
            &interaction_elements.memory_id_to_value_lookup,
        );

    // Commit to the interaction claim and the interaction trace.
    let interaction_claim = CairoInteractionClaim {
        ret: vec![ret_interaction_claim.clone()],
        memory_id_to_value: memory_id_to_value_interaction_claim.clone(),
    };
    debug_assert!(lookup_sum_valid(
        &claim,
        &interaction_elements,
        &interaction_claim
    ));
    interaction_claim.mix_into(channel);
    tree_builder.commit(channel);

    // Fixed trace.
    let mut tree_builder = commitment_scheme.tree_builder();
    let ret_constant_traces = claim
        .ret
        .iter()
        .map(|ret_claim| gen_is_first::<SimdBackend>(ret_claim.log_sizes()[2][0]))
        .collect_vec();
    let memory_id_to_value_constant_trace =
        gen_is_first::<SimdBackend>(claim.memory_id_to_value.log_sizes()[2][0]);
    let range_check9_9_constant_trace = gen_is_first::<SimdBackend>(18);
    tree_builder.extend_evals(
        [
            ret_constant_traces,
            vec![memory_id_to_value_constant_trace],
            vec![range_check9_9_constant_trace],
        ]
        .into_iter()
        .flatten(),
    );
    tree_builder.commit(channel);

    // Component provers.
    let component_builder = CairoComponents::new(&claim, &interaction_elements, &interaction_claim);
    let components = component_builder.provers();

    // Prove stark.
    let proof = prove::<SimdBackend, _>(&components, channel, commitment_scheme)?;

    Ok(CairoProof {
        claim,
        interaction_claim,
        stark_proof: proof,
    })
}

pub fn verify_cairo(
    CairoProof {
        claim,
        interaction_claim,
        stark_proof,
    }: CairoProof<Blake2sMerkleHasher>,
) -> Result<(), CairoVerificationError> {
    // Verify.
    let config = PcsConfig::default();
    let channel = &mut Blake2sChannel::default();
    let commitment_scheme_verifier =
        &mut CommitmentSchemeVerifier::<Blake2sMerkleChannel>::new(config);

    claim.mix_into(channel);
    commitment_scheme_verifier.commit(stark_proof.commitments[0], &claim.log_sizes()[0], channel);
    let interaction_elements = CairoInteractionElements::draw(channel);
    if !lookup_sum_valid(&claim, &interaction_elements, &interaction_claim) {
        return Err(CairoVerificationError::InvalidLogupSum);
    }
    interaction_claim.mix_into(channel);
    commitment_scheme_verifier.commit(stark_proof.commitments[1], &claim.log_sizes()[1], channel);

    // Fixed trace.
    commitment_scheme_verifier.commit(stark_proof.commitments[2], &claim.log_sizes()[2], channel);

    let component_generator =
        CairoComponents::new(&claim, &interaction_elements, &interaction_claim);
    let components = component_generator.components();

    verify(
        &components,
        channel,
        commitment_scheme_verifier,
        stark_proof,
    )
    .map_err(CairoVerificationError::Stark)
}

#[derive(Error, Debug)]
pub enum CairoVerificationError {
    #[error("Invalid logup sum")]
    InvalidLogupSum,
    #[error("Stark verification error: {0}")]
    Stark(#[from] VerificationError),
}

#[cfg(test)]
mod tests {
    use cairo_lang_casm::casm;

    use crate::cairo_air::{prove_cairo, verify_cairo, CairoInput};
    use crate::input::plain::input_from_plain_casm;
    use crate::input::vm_import::tests::small_cairo_input;

    fn test_input() -> CairoInput {
        let u128_max = u128::MAX;
        let instructions = casm! {
            // TODO(AlonH): Add actual range check segment.
            // Manually writing range check builtin segment of size 40 to memory.
            [ap] = u128_max, ap++;
            [ap + 38] = 1, ap++;
            ap += 38;

            [ap] = 10, ap++;
            call rel 4;
            jmp rel 11;

            jmp rel 4 if [fp-3] != 0;
            jmp rel 6;
            [ap] = [fp-3] + (-1), ap++;
            call rel (-6);
            ret;
        }
        .instructions;

        input_from_plain_casm(instructions)
    }

    #[test]
    #[ignore]
    fn test_basic_cairo_air() {
        let cairo_proof = prove_cairo(test_input()).unwrap();
        verify_cairo(cairo_proof).unwrap();
    }

    #[ignore]
    #[test]
    fn test_full_cairo_air() {
        let cairo_proof = prove_cairo(small_cairo_input()).unwrap();
        verify_cairo(cairo_proof).unwrap();
    }
}
