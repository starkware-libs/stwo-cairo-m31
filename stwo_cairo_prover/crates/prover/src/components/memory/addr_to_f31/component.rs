use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::logup::{LogupAtRow, LookupElements};
use stwo_prover::constraint_framework::{EvalAtRow, FrameworkComponent, FrameworkEval};
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::qm31::{SecureField, QM31};
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::lookups::utils::Fraction;
use stwo_prover::core::pcs::TreeVec;

pub const MEMORY_ID_SIZE: usize = 1;
pub const VALUE_SIZE: usize = 4;
pub const N_ADDR_AND_VALUE_COLUMNS: usize = MEMORY_ID_SIZE + VALUE_SIZE;
pub const MULTIPLICITY_COLUMN_OFFSET: usize = N_ADDR_AND_VALUE_COLUMNS;
pub const N_MULTIPLICITY_COLUMNS: usize = 1;
// TODO(AlonH): Make memory size configurable.
pub const N_COLUMNS: usize = N_ADDR_AND_VALUE_COLUMNS + N_MULTIPLICITY_COLUMNS;

pub type Component = FrameworkComponent<Eval>;

const N_LOGUP_POWERS: usize = MEMORY_ID_SIZE + VALUE_SIZE;
pub type RelationElements = LookupElements<N_LOGUP_POWERS>;

/// IDs are continuous and start from 0.
#[derive(Clone)]
pub struct Eval {
    pub log_n_rows: u32,
    pub lookup_elements: RelationElements,
    pub claimed_sum: QM31,
}
impl Eval {
    pub const fn n_columns(&self) -> usize {
        N_COLUMNS
    }
    pub fn new(
        claim: Claim,
        lookup_elements: RelationElements,
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
        let [is_first] = eval.next_interaction_mask(2, [0]);
        let mut logup = LogupAtRow::<E>::new(1, self.claimed_sum, None, is_first);

        let id_and_value: [E::F; N_ADDR_AND_VALUE_COLUMNS] =
            std::array::from_fn(|_| eval.next_trace_mask());
        let multiplicity = eval.next_trace_mask();
        let frac = Fraction::new(
            E::EF::from(-multiplicity),
            self.lookup_elements.combine(&id_and_value),
        );
        logup.write_frac(&mut eval, frac);

        logup.finalize(&mut eval);

        eval
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Claim {
    pub log_size: u32,
}
impl Claim {
    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let interaction_0_log_size = vec![self.log_size; N_COLUMNS];
        let interaction_1_log_size = vec![self.log_size; SECURE_EXTENSION_DEGREE]; // ???
        let fixed_column_log_sizes = vec![self.log_size];
        TreeVec::new(vec![
            interaction_0_log_size,
            interaction_1_log_size,
            fixed_column_log_sizes,
        ])
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
