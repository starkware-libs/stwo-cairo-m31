pub mod component;
pub mod prover;

pub use component::{Claim, Component, Eval, InteractionClaim};
pub use prover::ClaimGenerator;

#[cfg(test)]
mod tests {
    use std::simd::Simd;

    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};
    use stwo_prover::constraint_framework::preprocessed_columns::gen_is_first;
    use stwo_prover::core::backend::simd::m31::PackedM31;
    use stwo_prover::core::backend::simd::SimdBackend;
    use stwo_prover::core::channel::Blake2sChannel;
    use stwo_prover::core::pcs::{CommitmentSchemeProver, PcsConfig};
    use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};
    use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

    use super::*;

    #[test]
    fn test_add_mul_opcode() {
        const LOG_HEIGHT: u32 = 14;
        const LOG_BLOWUP_FACTOR: u32 = 1;

        let mut rng = SmallRng::seed_from_u64(0);
        let mut claim_generator = ClaimGenerator::new();

        let twiddles = SimdBackend::precompute_twiddles(
            CanonicCoset::new(LOG_HEIGHT + LOG_BLOWUP_FACTOR)
                .circle_domain()
                .half_coset,
        );

        let channel = &mut Blake2sChannel::default();
        let config = PcsConfig::default();
        let commitment_scheme =
            &mut CommitmentSchemeProver::<SimdBackend, Blake2sMerkleChannel>::new(
                config, &twiddles,
            );

        // Preprocessed trace.
        let mut tree_builder = commitment_scheme.tree_builder();
        let range_check_preprocessed_trace = gen_is_first::<SimdBackend>(LOG_HEIGHT);
        tree_builder.extend_evals([range_check_preprocessed_trace]);
        tree_builder.commit(channel);

        let inputs: [[PackedM31; 3]; 30] = std::array::from_fn(|_| {
            let values = Simd::from_array(std::array::from_fn(|_| {
                rng.gen::<u32>() & ((1 << LOG_HEIGHT) - 1)
            }));
            let partitions = partition_into_bit_segments(values, log_ranges);
            std::array::from_fn(|i| unsafe { PackedM31::from_simd_unchecked(partitions[i]) })
        });

        inputs.into_iter().for_each(|input| {
            claim_generator.add_packed_m31(&input);
        });

        let mut tree_builder = commitment_scheme.tree_builder();
        let (_, interaction_claim_generator) = claim_generator.write_trace(&mut tree_builder);

        tree_builder.commit(channel);
        let mut tree_builder = commitment_scheme.tree_builder();

        let lookup_elements = relations::RangeCheck_7_2_5::draw(channel);
        let interaction_claim = interaction_claim_generator
            .write_interaction_trace(&mut tree_builder, &lookup_elements);
        tree_builder.commit(channel);

        let tree_span_provider = &mut TraceLocationAllocator::default();
        let component = FrameworkComponent::new(
            tree_span_provider,
            range_check_7_2_5::Eval { lookup_elements },
            (interaction_claim.claimed_sum, None),
        );

        let trace_polys = commitment_scheme
            .trees
            .as_ref()
            .map(|t| t.polynomials.iter().cloned().collect_vec());

        stwo_prover::constraint_framework::assert_constraints(
            &trace_polys,
            CanonicCoset::new(LOG_HEIGHT),
            |eval| {
                component.evaluate(eval);
            },
            (interaction_claim.claimed_sum, None),
        )
    }
}
