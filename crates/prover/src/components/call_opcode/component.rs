use num_traits::One;
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, RelationEntry,
};
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::SecureField;
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::MemoryRelation;

// | ap | fp | pc | is_rel | reg[ap/fp/imm] | arg_0 | dest_addr | dest | next_pc |
pub const CALL_N_TRACE_CELLS: usize = 9;
pub const CALL_INSTRUCTION: M31 = M31::from_u32_unchecked(171); // FIX.
pub type Component = FrameworkComponent<Eval>;

#[derive(Clone)]
pub struct Eval {
    pub log_n_rows: u32,
    pub memory_lookup: MemoryRelation,
    pub claimed_sum: SecureField,
}

impl Eval {
    pub fn new(
        ret_claim: Claim,
        memory_lookup: MemoryRelation,
        interaction_claim: InteractionClaim,
    ) -> Self {
        Self {
            log_n_rows: ret_claim.n_rets.next_power_of_two().ilog2(),
            memory_lookup,
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
        // PC column.
        let pc = eval.next_trace_mask();
        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[pc, CALL_INSTRUCTION.into()],
        )]);

        let _ap = eval.next_trace_mask();
        let fp = eval.next_trace_mask();

        // FP - 1
        let fp_minus_one = fp.clone() - E::F::one();
        let fp_minus_one_val = eval.next_trace_mask();
        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[fp_minus_one, fp_minus_one_val],
        )]);

        // FP - 2
        let fp_minus_two = fp - E::F::from(M31::from(2));
        let fp_minus_two_val = eval.next_trace_mask();
        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[fp_minus_two, fp_minus_two_val],
        )]);

        // TODO(giladchase): Add state lookups.

        eval.finalize_logup();
        eval
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Claim {
    pub n_rets: usize,
}
impl Claim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.n_rets as u64);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let log_size = self.n_rets.next_power_of_two().ilog2();
        let interaction_0_log_sizes = vec![log_size; CALL_N_TRACE_CELLS];
        let interaction_1_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        let fixed_log_sizes = vec![log_size];
        TreeVec::new(vec![
            interaction_0_log_sizes,
            interaction_1_log_sizes,
            fixed_log_sizes,
        ])
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InteractionClaim {
    pub log_size: u32,
    pub claimed_sum: SecureField,
}
impl InteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_felts(&[self.claimed_sum]);
    }
}
