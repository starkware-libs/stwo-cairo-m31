pub mod add_mul_opcode;
pub mod memory;
pub mod ret_opcode;

// TODO(ShaharS): Move to a common file.
pub const LOOKUP_INTERACTION_PHASE: usize = 1;

#[cfg(test)]
mod tests {
    use itertools::Itertools;
    use num_traits::One;
    use stwo_prover::core::backend::simd::SimdBackend;
    use stwo_prover::core::channel::Blake2sChannel;
    use stwo_prover::core::fields::qm31::QM31;
    use stwo_prover::core::pcs::{CommitmentSchemeProver, PcsConfig};
    use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};

    use crate::cairo_air::CairoInteractionElements;
    use crate::components::{memory, ret_opcode};
    use crate::input::instructions::VmState;
    use crate::input::mem::{MemConfig, MemoryBuilder};
    use crate::input::vm_import::MemEntry;

    #[ignore]
    #[test]
    fn test_ret() {
        let memory = MemoryBuilder::from_iter(
            MemConfig::default(),
            (0..10).map(|i| MemEntry {
                addr: i,
                val: [i; 4],
            }),
        )
        .build();
        let mut memory_claim_generator = memory::ClaimGenerator::new(&memory);
        let ret_claim_generator = ret_opcode::ClaimGenerator::new(vec![VmState {
            pc: 1,
            ap: 2,
            fp: 3,
        }]);

        let config = PcsConfig::default();
        const LOG_MAX_ROWS: u32 = 20;
        let twiddles = SimdBackend::precompute_twiddles(
            CanonicCoset::new(LOG_MAX_ROWS + config.fri_config.log_blowup_factor + 2)
                .circle_domain()
                .half_coset,
        );
        let mut commitment_scheme = CommitmentSchemeProver::new(config, &twiddles);

        let mut tree_builder = commitment_scheme.tree_builder();
        // assert_eq!(

        let channel = &mut Blake2sChannel::default();
        let _interaction_elements = CairoInteractionElements::draw(channel);

        let (_claim, ret_interaction_prover) =
            ret_claim_generator.write_trace(&mut tree_builder, &mut memory_claim_generator);

        for a in ret_interaction_prover.memory_outputs {
            let b = a.into_iter().flat_map(|f| f.to_array()).collect_vec();
            assert_eq!(b, vec![QM31::one()]);
        }
    }
}
