use num_traits::One;
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{EvalAtRow, FrameworkComponent, RelationEntry};
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::SecureField;
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::{MemoryRelation, StateRelation};
use crate::utils::component::log_size;

pub const RET_N_TRACE_CELLS: usize = 5;
// TODO(alont): set instruction bases to not overlap
pub const RET_INSTRUCTION: M31 = M31::from_u32_unchecked(0);
pub type Component = FrameworkComponent<Eval>;

#[derive(Clone)]
pub struct Eval {
    pub claim: Claim,
    pub memory_lookup: MemoryRelation,
    pub state_lookup: StateRelation,
}

impl Eval {
    pub fn log_size(&self) -> u32 {
        log_size(self.claim.n_rows)
    }

    pub fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    pub fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        // Initial state.
        let state = std::array::from_fn(|_| eval.next_trace_mask());
        // Use initial state.
        eval.add_to_relation(RelationEntry::new(&self.state_lookup, E::EF::one(), &state));
        let [pc, ap, fp] = state;

        // Lookup pc.
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[pc, RET_INSTRUCTION.into()],
        ));

        // FP - 1
        let fp_minus_one = fp.clone() - E::F::one();
        let fp_minus_one_val = eval.next_trace_mask();
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[fp_minus_one, fp_minus_one_val.clone()],
        ));

        // FP - 2
        let fp_minus_two = fp - E::F::from(M31::from(2));
        let fp_minus_two_val = eval.next_trace_mask();
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[fp_minus_two, fp_minus_two_val.clone()],
        ));
        let new_pc = fp_minus_one_val;
        let new_fp = fp_minus_two_val;

        let new_state = [new_pc, ap, new_fp];
        eval.add_to_relation(RelationEntry::new(
            &self.state_lookup,
            -E::EF::one(),
            &new_state,
        ));

        eval.finalize_logup_in_pairs();
        eval
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Claim {
    pub n_rows: usize,
}
impl Claim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.n_rows as u64);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let log_size = self.n_rows.next_power_of_two().ilog2();
        let interaction_0_log_sizes = vec![log_size; RET_N_TRACE_CELLS];
        let interaction_1_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        TreeVec::new(vec![
            vec![],
            interaction_0_log_sizes,
            interaction_1_log_sizes,
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
