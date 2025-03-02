use std::simd::Simd;

use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::constraint_framework::Relation;
use stwo_prover::core::backend::simd::column::BaseColumn;
use stwo_prover::core::backend::simd::m31::{PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::backend::BackendForChannel;
use stwo_prover::core::channel::MerkleChannel;
use stwo_prover::core::fields::m31::{BaseField, M31};
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::poly::circle::{CanonicCoset, CircleEvaluation};
use stwo_prover::core::poly::BitReversedOrder;

use super::component::LOG_RANGE;
use super::{Claim, InteractionClaim};
use crate::components::range_check::{partition_into_bit_segments, SIMD_ENUMERATION_0};
use crate::components::utils::AtomicMultiplicityColumn;
use crate::relations;
pub type PackedInputType = PackedM31;
pub type InputType = M31;

pub struct ClaimGenerator {
    multiplicities: AtomicMultiplicityColumn,
}
impl ClaimGenerator {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let length = 1 << LOG_RANGE as usize;
        let multiplicities = AtomicMultiplicityColumn::new(length);

        Self { multiplicities }
    }

    fn log_size(&self) -> u32 {
        LOG_RANGE
    }

    pub fn add_inputs(&self, inputs: &[InputType]) {
        for input in inputs {
            self.add_input(input);
        }
    }

    pub fn add_input(&self, input: &InputType) {
        self.multiplicities.increase_at(input.0);
    }

    pub fn write_trace<MC: MerkleChannel>(
        self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, MC>,
    ) -> (Claim, InteractionClaimGenerator)
    where
        SimdBackend: BackendForChannel<MC>,
    {
        let log_size = self.log_size();

        let multiplicity_data = self.multiplicities.into_simd_vec();
        let multiplicity_column = BaseColumn::from_simd(multiplicity_data.clone());

        let domain = CanonicCoset::new(log_size).circle_domain();
        let trace = [multiplicity_column].map(|col| {
            CircleEvaluation::<SimdBackend, BaseField, BitReversedOrder>::new(domain, col)
        });

        tree_builder.extend_evals(trace);

        let claim = Claim { log_size };

        let interaction_claim_prover = InteractionClaimGenerator {
            multiplicities: multiplicity_data,
        };

        (claim, interaction_claim_prover)
    }
}

#[derive(Debug)]
pub struct InteractionClaimGenerator {
    pub multiplicities: Vec<PackedM31>,
}
impl InteractionClaimGenerator {
    pub fn write_interaction_trace<MC: MerkleChannel>(
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, MC>,
        lookup_elements: &relations::RangeCheck_16,
    ) -> InteractionClaim
    where
        SimdBackend: BackendForChannel<MC>,
    {
        let log_size = LOG_RANGE;
        let mut logup_gen = LogupTraceGenerator::new(log_size);
        let mut col_gen = logup_gen.new_col();

        // Lookup values columns.
        for vec_row in 0..(1 << (log_size - LOG_N_LANES)) {
            let numerator = (-self.multiplicities[vec_row]).into();
            let partition = partition_into_bit_segments(
                SIMD_ENUMERATION_0 + Simd::splat((vec_row * N_LANES) as u32),
                [LOG_RANGE],
            );
            let partition = [unsafe { PackedM31::from_simd_unchecked(partition[0]) }];
            let denom = lookup_elements.combine(&partition);
            col_gen.write_frac(vec_row, numerator, denom);
        }
        col_gen.finalize_col();

        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim { claimed_sum }
    }
}
