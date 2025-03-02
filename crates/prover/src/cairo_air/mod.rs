use constraints::{
    lookup_sum_valid, CairoClaim, CairoComponents, CairoInteractionClaim, CairoRelations,
};
use preprocessed::PreProcessedTrace;
use serde::{Deserialize, Serialize};
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::channel::Blake2sChannel;
use stwo_prover::core::pcs::{CommitmentSchemeProver, CommitmentSchemeVerifier, PcsConfig};
use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};
use stwo_prover::core::prover::{prove, verify, ProvingError, StarkProof, VerificationError};
use stwo_prover::core::vcs::blake2_merkle::{Blake2sMerkleChannel, Blake2sMerkleHasher};
use thiserror::Error;
use tracing::{span, Level};
use witness::CairoWitnessGen;

use crate::input::CairoInput;

mod constraints;
pub mod preprocessed;
mod witness;

#[derive(Serialize, Deserialize)]
pub struct CairoProof {
    pub claim: CairoClaim,
    pub interaction_claim: CairoInteractionClaim,
    pub stark_proof: StarkProof<Blake2sMerkleHasher>,
}

const LOG_MAX_ROWS: u32 = 20;
pub fn prove_cairo(input: CairoInput) -> Result<CairoProof, ProvingError> {
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

    // PP trace.
    let mut tree_builder = commitment_scheme.tree_builder();
    let pp_trace = PreProcessedTrace::new().gen_trace();
    tree_builder.extend_evals(pp_trace);
    tree_builder.commit(channel);

    let mut tree_builder = commitment_scheme.tree_builder();
    let witness_gen = CairoWitnessGen::new(input);
    let (claim, interaction_generator) = witness_gen.write_trace(&mut tree_builder);
    claim.mix_into(channel);
    tree_builder.commit(channel);

    // Draw interaction elements.
    let interaction_elements = CairoRelations::draw(channel);

    // Interaction trace.
    let mut tree_builder = commitment_scheme.tree_builder();
    let interaction_claim =
        interaction_generator.write_interaction_trace(&mut tree_builder, &interaction_elements);
    debug_assert!(lookup_sum_valid(
        &claim,
        &interaction_elements,
        &interaction_claim
    ));
    interaction_claim.mix_into(channel);
    tree_builder.commit(channel);

    // Constraint evaluators.
    let component_builder = CairoComponents::new(&claim, &interaction_elements, &interaction_claim);
    let components = component_builder.provers();

    // STARKs.
    let stark_proof = prove::<SimdBackend, _>(&components, channel, commitment_scheme)?;

    Ok(CairoProof {
        claim,
        interaction_claim,
        stark_proof,
    })
}

pub fn verify_cairo(
    CairoProof {
        claim,
        interaction_claim,
        stark_proof,
    }: CairoProof,
) -> Result<(), CairoVerificationError> {
    // Verify.
    let config = PcsConfig::default();
    let channel = &mut Blake2sChannel::default();
    let commitment_scheme_verifier =
        &mut CommitmentSchemeVerifier::<Blake2sMerkleChannel>::new(config);

    commitment_scheme_verifier.commit(stark_proof.commitments[0], &claim.log_sizes()[0], channel);
    claim.mix_into(channel);
    commitment_scheme_verifier.commit(stark_proof.commitments[1], &claim.log_sizes()[1], channel);
    let interaction_elements = CairoRelations::draw(channel);
    if !lookup_sum_valid(&claim, &interaction_elements, &interaction_claim) {
        return Err(CairoVerificationError::InvalidLogupSum);
    }
    interaction_claim.mix_into(channel);
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
    use super::{prove_cairo, verify_cairo};
    use crate::input::instructions::{Instructions, VmState};
    use crate::input::mem::{MemConfig, MemoryBuilder};
    use crate::input::vm_import::MemEntry;
    use crate::input::CairoInput;

    #[test]
    fn dummmy_test() {
        let [first, last] = [VmState {
            pc: 0,
            ap: 0,
            fp: 0,
        }; 2];
        let instructions = Instructions {
            initial_state: first,
            final_state: last,
            ..Default::default()
        };
        let memory = MemoryBuilder::from_iter(
            MemConfig::default(),
            (0..10).map(|i| MemEntry {
                addr: i,
                val: [i; 4],
            }),
        )
        .build();
        let public_mem_addresses = vec![0, 1, 2];
        let input = CairoInput {
            instructions,
            memory,
            public_mem_addresses,
        };

        let proof = prove_cairo(input).unwrap();
        verify_cairo(proof).unwrap();
    }
}
