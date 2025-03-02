use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, RelationEntry,
};
use stwo_prover::core::backend::simd::m31::LOG_N_LANES;
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::SecureField;
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::cairo_air::preprocessed::{PreProcessedColumn, RangeCheck};
use crate::relations::RangeCheck_16;

pub const N_TRACE_COLUMNS: usize = 1;
pub const LOG_RANGE: u32 = 16;
// TODO(alont): set instruction bases to not overlap
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

pub type Component = FrameworkComponent<Eval>;

#[derive(Clone)]
pub struct Eval {
    pub claim: Claim,
    pub range_check_16_lookup_elements: RangeCheck_16,
}
impl Eval {
    pub fn new(claim: Claim, range_check_16_lookup_elements: RangeCheck_16) -> Self {
        Self {
            claim,
            range_check_16_lookup_elements,
        }
    }
}

impl FrameworkEval for Eval {
    fn log_size(&self) -> u32 {
        self.claim.log_size
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let value = eval.get_preprocessed_column(RangeCheck::new([LOG_RANGE], 0).id());
        let multiplicity = eval.next_trace_mask();

        eval.add_to_relation(RelationEntry::new(
            &self.range_check_16_lookup_elements,
            E::EF::from(-multiplicity),
            &[value],
        ));

        eval.finalize_logup();
        eval
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Claim {
    pub log_size: u32,
}
impl Claim {
    pub fn new(log_size: u32) -> Self {
        assert!(
            log_size >= LOG_N_LANES,
            "log_size must be at least LOG_N_LANES"
        );
        Self { log_size }
    }
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.log_size as u64);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let preprocessed_log_sizes = vec![self.log_size];
        let trace_log_sizes = vec![self.log_size; N_TRACE_COLUMNS];
        let interaction_log_sizes = vec![self.log_size; SECURE_EXTENSION_DEGREE];
        TreeVec::new(vec![
            preprocessed_log_sizes,
            trace_log_sizes,
            interaction_log_sizes,
        ])
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InteractionClaim {
    pub claimed_sum: SecureField,
}
impl InteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_felts(&[self.claimed_sum]);
    }
}
