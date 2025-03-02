use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, RelationEntry,
};
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::qm31::{SecureField, QM31};
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::{MemoryRelation, ADDR_SIZE, VALUE_SIZE};

// TODO(Gilad): remove the addr column and use preprocessed column.
pub const N_ADDR_AND_VALUE_COLUMNS: usize = ADDR_SIZE + VALUE_SIZE;

pub const MULTIPLICITY_COLUMN_OFFSET: usize = N_ADDR_AND_VALUE_COLUMNS;
pub const N_MULTIPLICITY_COLUMNS: usize = 1;
// TODO(AlonH): Make memory size configurable.
pub const N_COLUMNS: usize = N_ADDR_AND_VALUE_COLUMNS + N_MULTIPLICITY_COLUMNS;

pub type Component = FrameworkComponent<Eval>;

/// IDs are continuous and start from 0.
#[derive(Clone)]
pub struct Eval {
    pub log_n_rows: u32,
    pub lookup_elements: MemoryRelation,
    pub claimed_sum: QM31,
}
impl Eval {
    pub const fn n_columns(&self) -> usize {
        N_COLUMNS
    }
    pub fn new(
        claim: Claim,
        lookup_elements: MemoryRelation,
        interaction_claim: InteractionClaim,
    ) -> Self {
        Self {
            log_n_rows: claim.log_size,
            lookup_elements,
            claimed_sum: interaction_claim.claimed_sum,
        }
    }
}

impl FrameworkEval for Eval {
    fn log_size(&self) -> u32 {
        self.log_n_rows
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let addr_and_value: [E::F; N_ADDR_AND_VALUE_COLUMNS] =
            std::array::from_fn(|_| eval.next_trace_mask());
        let multiplicity = eval.next_trace_mask();

        eval.add_to_relation(RelationEntry::new(
            &self.lookup_elements,
            -E::EF::from(multiplicity),
            &addr_and_value,
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
    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let trace = vec![self.log_size; N_COLUMNS];
        let interaction_trace = vec![self.log_size; SECURE_EXTENSION_DEGREE];
        TreeVec::new(vec![vec![], trace, interaction_trace])
    }

    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.log_size as u64);
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
