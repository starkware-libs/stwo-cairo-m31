use num_traits::{One, Zero};
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, RelationEntry,
};
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::SecureField;
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::{MemoryRelation, StateRelation};

pub const N_TRACE_COLUMNS: usize = 4;
// TODO(alont): set instruction bases to not overlap
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

// TODO(alont) put these in a common file.
pub const IMM_BASE: M31 = M31::from_u32_unchecked(1 << 29);

pub type Component = FrameworkComponent<Eval>;

#[derive(Clone)]
pub struct Eval {
    pub log_n_rows: u32,
    pub memory_lookup: MemoryRelation,
    pub state_lookup: StateRelation,
    pub claimed_sum: SecureField,
}
impl Eval {
    pub fn new(
        ret_claim: Claim,
        memory_lookup: MemoryRelation,
        state_lookup: StateRelation,
        interaction_claim: InteractionClaim,
    ) -> Self {
        Self {
            log_n_rows: ret_claim.n_calls.next_power_of_two().ilog2(),
            memory_lookup,
            state_lookup,
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
        let state = std::array::from_fn(|_| eval.next_trace_mask());
        // Use initial state.
        eval.add_to_relation(RelationEntry::new(&self.state_lookup, E::EF::one(), &state));
        let [pc, ap, fp] = state;

        // Check instruction.
        let imm = eval.next_trace_mask();
        let opcode = E::F::from(INSTRUCTION_BASE);
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[
                pc.clone(),
                opcode.clone(),
                imm.clone(),
                E::F::zero(),
                E::F::zero(),
            ],
        ));

        // Return pointers.
        [
            [ap.clone(), fp.clone()],                             // Return `fp`.
            [ap.clone() + E::F::one(), pc.clone() + E::F::one()], // Return `pc`.
        ]
        .iter()
        .map(|memory_entry| {
            eval.add_to_relation(RelationEntry::new(
                &self.state_lookup,
                E::EF::one(),
                memory_entry,
            ));
        });

        let new_pc = pc.clone() + imm;
        let new_ap = ap.clone() + E::F::from(M31(2));
        let new_state = [new_pc, new_ap.clone(), new_ap];
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
    pub n_calls: usize,
}
impl Claim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.n_calls as u64);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let log_size = self.n_calls.next_power_of_two().ilog2();
        let preprocessed_log_sizes = vec![log_size];
        let interaction_1_log_sizes = vec![log_size; N_TRACE_COLUMNS];
        let interaction_2_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        TreeVec::new(vec![
            preprocessed_log_sizes,
            interaction_1_log_sizes,
            interaction_2_log_sizes,
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
