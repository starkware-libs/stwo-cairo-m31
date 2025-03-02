use constraints::{
    lookup_sum_valid, CairoClaim, CairoComponents, CairoInteractionClaim, CairoRelations,
};
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

    claim.mix_into(channel);
    commitment_scheme_verifier.commit(stark_proof.commitments[0], &claim.log_sizes()[0], channel);
    let interaction_elements = CairoRelations::draw(channel);
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
