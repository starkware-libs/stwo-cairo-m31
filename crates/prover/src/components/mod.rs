pub mod add_mul_opcode;
pub mod memory;
pub mod ret_opcode;

// TODO(ShaharS): Move to a common file.
pub const LOOKUP_INTERACTION_PHASE: usize = 1;

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use stwo_prover::core::backend::simd::SimdBackend;
    use stwo_prover::core::fields::m31::BaseField;
    use stwo_prover::core::pcs::{CommitmentSchemeProver, PcsConfig};
    use stwo_prover::core::poly::circle::{CanonicCoset, PolyOps};

    use crate::components::ret_opcode::component::RET_INSTRUCTION;
    use crate::components::{memory, ret_opcode};
    use crate::input::instructions::VmState;
    use crate::input::mem::{MemConfig, MemoryBuilder};
    use crate::input::vm_import::MemEntry;

    #[test]
    fn test_ret_generator() {
        // Setup.

        let memory = MemoryBuilder::from_iter(
            MemConfig::default(),
            (0..10).map(|i| MemEntry {
                addr: i,
                val: [i * 100; 4],
            }),
        )
        .build();
        let mut memory_claim_generator = memory::ClaimGenerator::new(&memory);

        let input = VmState {
            pc: 10, // arbitrary
            ap: 15, // arbitrary
            fp: 3,
        };
        // Memory multiplies addr `i` by 100.
        let expected_output = vec![
            RET_INSTRUCTION,
            BaseField::from_u32_unchecked((input.fp - 1) * 100),
            BaseField::from_u32_unchecked((input.fp - 2) * 100),
        ];

        // Boilerplate.
        let ret_claim_generator = ret_opcode::ClaimGenerator::new(vec![input]);
        let config = PcsConfig::default();
        const LOG_MAX_ROWS: u32 = 20;
        let twiddles = SimdBackend::precompute_twiddles(
            CanonicCoset::new(LOG_MAX_ROWS + config.fri_config.log_blowup_factor + 2)
                .circle_domain()
                .half_coset,
        );
        let mut commitment_scheme = CommitmentSchemeProver::new(config, &twiddles);
        let mut tree_builder = commitment_scheme.tree_builder();

        // Test.
        let (_claim, ret_interaction_prover) =
            ret_claim_generator.write_trace(&mut tree_builder, &mut memory_claim_generator);

        let output = ret_interaction_prover
            .memory_outputs
            .map(|output| output[0].to_array()[0].to_m31_array()[0]);
        assert_eq!(output, *expected_output);
    }
}
