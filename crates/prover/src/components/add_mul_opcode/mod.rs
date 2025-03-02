pub mod component;
pub mod prover;

pub use component::{Claim, Component, Eval, InteractionClaim};
pub use prover::ClaimGenerator;

#[cfg(test)]
mod tests {

    use itertools::{chain, Itertools};
    use num_traits::Zero;
    use rand::rngs::SmallRng;
    use rand::{Rng, SeedableRng};
    use stwo_prover::constraint_framework::{
        FrameworkComponent, FrameworkEval, TraceLocationAllocator,
    };
    use stwo_prover::core::backend::simd::SimdBackend;
    use stwo_prover::core::channel::Blake2sChannel;
    use stwo_prover::core::fields::m31::M31;
    use stwo_prover::core::fields::qm31::QM31;
    use stwo_prover::core::pcs::{CommitmentSchemeProver, PcsConfig};
    use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};
    use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

    use super::*;
    use crate::components::add_mul_opcode::component::INSTRUCTION_BASE;
    use crate::components::memory;
    use crate::input::mem::MemoryValue;
    use crate::relations;
    use crate::utils::types::CasmState;

    #[test]
    fn test_add_mul_opcode() {
        const LOG_HEIGHT: u32 = 8;
        const LOG_BLOWUP_FACTOR: u32 = 1;

        let mut rng = SmallRng::seed_from_u64(0);

        #[allow(clippy::erasing_op, clippy::identity_op)]
        let add_ap_ap_fp = INSTRUCTION_BASE + M31(0 + 0 * 2 + 1 * 4 + 1 * 8 + 0 * 24);
        #[allow(clippy::erasing_op, clippy::identity_op)]
        let mul_fp_fp_ap_appp = INSTRUCTION_BASE + M31(1 + 1 * 2 + 1 * 4 + 0 * 8 + 1 * 24);
        #[allow(clippy::erasing_op, clippy::identity_op)]
        let add_ap_fp_ddrf = INSTRUCTION_BASE + M31(0 + 0 * 2 + 1 * 4 + 2 * 8 + 0 * 24);
        let x: QM31 = rng.gen();
        let y: QM31 = rng.gen();
        let z: QM31 = rng.gen();

        // Initialize at pc=0, ap=3, fp=6 with:
        //  pc -> 0: [ap] = [fp + 2] + [fp + 2]
        //        1: [fp + 3] = [fp + 4] * [ap + 2]; ap++
        //        2: [ap] = [[fp]]
        //  ap -> 3: 2X
        //        4: X
        //        5: Y
        //  fp -> 6: 7
        //        7: 8
        //        8: X
        //        9: Z * Y
        //        10: Z
        let mut memory_claim_generator = memory::ClaimGenerator {
            values: [
                QM31::from_m31_array([add_ap_ap_fp, M31(0), M31(2), M31(2)]),
                QM31::from_m31_array([mul_fp_fp_ap_appp, M31(3), M31(4), M31(2)]),
                QM31::from_m31_array([add_ap_fp_ddrf, M31(0), M31(0), M31(0)]),
                x + x,
                x,
                y,
                QM31::from_m31_array([M31(7), M31(0), M31(0), M31(0)]),
                QM31::from_m31_array([M31(8), M31(0), M31(0), M31(0)]),
                x,
                z * y,
                z,
                QM31::zero(),
            ]
            .into_iter()
            .map(MemoryValue)
            .collect(),
            // Dummy multiplicities
            multiplicities: vec![1; 16],
        };

        let claim_generator = ClaimGenerator::new(
            chain!(
                vec![
                    CasmState {
                        pc: M31::from(0),
                        ap: M31::from(3),
                        fp: M31::from(6),
                    };
                    128
                ],
                vec![
                    CasmState {
                        pc: M31::from(1),
                        ap: M31::from(3),
                        fp: M31::from(6),
                    };
                    64
                ],
                vec![
                    CasmState {
                        pc: M31::from(2),
                        ap: M31::from(4),
                        fp: M31::from(6),
                    };
                    64
                ]
            )
            .collect_vec(),
        );

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

        // Preprocessed.
        let tree_builder = commitment_scheme.tree_builder();
        tree_builder.commit(channel);

        let mut tree_builder = commitment_scheme.tree_builder();
        let (claim, interaction_claim_generator) =
            claim_generator.write_trace(&mut tree_builder, &mut memory_claim_generator);

        tree_builder.commit(channel);
        let mut tree_builder = commitment_scheme.tree_builder();

        let memory_relation = relations::MemoryRelation::draw(channel);
        let state_relation = relations::StateRelation::draw(channel);
        let interaction_claim = interaction_claim_generator.write_interaction_trace(
            &mut tree_builder,
            &memory_relation,
            &state_relation,
        );
        tree_builder.commit(channel);

        let trace_location_allocator = &mut TraceLocationAllocator::default();
        let component = FrameworkComponent::new(
            trace_location_allocator,
            Eval::new(claim, memory_relation, state_relation),
            interaction_claim.claimed_sum,
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
            interaction_claim.claimed_sum,
        )
    }
}
