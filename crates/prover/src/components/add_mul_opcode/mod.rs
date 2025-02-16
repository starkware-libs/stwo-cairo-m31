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
    use stwo_prover::core::backend::simd::qm31::PackedSecureField;
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
    use crate::input::instructions::VmState;
    use crate::relations;

    #[test]
    fn test_add_mul_opcode() {
        const LOG_HEIGHT: u32 = 8;
        const LOG_BLOWUP_FACTOR: u32 = 1;

        let mut rng = SmallRng::seed_from_u64(0);

        #[allow(clippy::erasing_op, clippy::identity_op)]
        let add_ap_ap_fp = INSTRUCTION_BASE + M31(0 + 0 * 2 + 1 * 4 + 1 * 8 + 0 * 16);
        #[allow(clippy::erasing_op, clippy::identity_op)]
        let mul_fp_fp_ap_appp = INSTRUCTION_BASE + M31(1 + 1 * 2 + 1 * 4 + 0 * 8 + 1 * 16);
        let x: QM31 = rng.gen();
        let y: QM31 = rng.gen();
        let z: QM31 = rng.gen();

        // Initialize at pc=0, ap=2, fp=4 with:
        //  pc -> 0: [ap] = [fp] + [fp]
        //        1: [fp + 1] = [fp + 2] * [ap + 1]; ap++
        //  ap -> 2: 2X
        //        3: Y
        //  fp -> 4: X
        //        5: Z * Y
        //        6: Z
        let mut memory_claim_generator = memory::ClaimGenerator {
            values: vec![PackedSecureField::from_array([
                QM31::from_m31_array([add_ap_ap_fp, M31(0), M31(0), M31(0)]),
                QM31::from_m31_array([mul_fp_fp_ap_appp, M31(1), M31(2), M31(1)]),
                x + x,
                y,
                x,
                z * y,
                z,
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
                QM31::zero(),
            ])],
            // Dummy multiplicities
            multiplicities: vec![1; 16],
        };

        let claim_generator = ClaimGenerator::new(
            chain!(
                vec![
                    VmState {
                        pc: 0,
                        ap: 2,
                        fp: 4,
                    };
                    128
                ],
                vec![
                    VmState {
                        pc: 1,
                        ap: 2,
                        fp: 4,
                    };
                    128
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
