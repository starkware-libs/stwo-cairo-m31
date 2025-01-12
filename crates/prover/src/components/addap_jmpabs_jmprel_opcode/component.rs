use num_traits::One;
use serde::{Deserialize, Serialize};
use stwo_prover::constraint_framework::logup::LogupSums;
use stwo_prover::constraint_framework::{
    EvalAtRow, FrameworkComponent, FrameworkEval, RelationEntry,
};
use stwo_prover::core::backend::simd::m31::LOG_N_LANES;
use stwo_prover::core::channel::Channel;
use stwo_prover::core::fields::m31::{BaseField, M31};
use stwo_prover::core::fields::secure_column::SECURE_EXTENSION_DEGREE;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::{MemoryRelation, StateRelation};
use crate::utils::component::decode_opcode;
use crate::utils::{Selector, SelectorTrait};

pub const ADD_AP_N_TRACE_CELLS: usize = 7;
// TODO: organize opcodes so that deduce_opcode will actually work with this.
// Opcode base should be: `addap_imm = K`
// such that:
// ```
// jmp_abs_imm = K + 1
// jmp_rel_imm = K + 2
// ```
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

pub type Component = FrameworkComponent<Eval>;

pub struct Eval {
    pub claim: Claim,
    pub memory_lookup: MemoryRelation,
    pub state_lookup: StateRelation,
}

impl FrameworkEval for Eval {
    fn log_size(&self) -> u32 {
        std::cmp::max(self.claim.n_rows.next_power_of_two().ilog2(), LOG_N_LANES)
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let state = std::array::from_fn(|_| eval.next_trace_mask());
        // Use initial state.
        eval.add_to_relation(RelationEntry::new(&self.state_lookup, E::EF::one(), &state));
        let [pc, ap, fp] = state;

        // Assert flag is in range.
        let opcode_type = eval.next_trace_mask();
        const ONE: BaseField = BaseField::from_u32_unchecked(1);
        const TWO: BaseField = BaseField::from_u32_unchecked(2);
        const THREE: BaseField = BaseField::from_u32_unchecked(3);
        // assert is_trit.
        eval.add_constraint(
            opcode_type.clone() * opcode_type.clone() * opcode_type.clone()
                - E::F::from(THREE) * opcode_type.clone() * opcode_type.clone()
                + E::F::from(TWO) * opcode_type.clone(),
        );

        // Check instruction.
        let opcode = decode_opcode(
            INSTRUCTION_BASE.into(),
            &[
                (opcode_type.clone(), 3), // [addap, jmp_abs, jmp_rel]
            ],
        );

        let imm = eval.next_trace_mask();

        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[pc.clone(), opcode.clone()],
        ));

        // Compute new pc and new ap.
        // NEW_PC = { addap: pc + 1, jmp_imm: imm, jmp_rel: pc + imm }[opcode_type]
        let new_pc = Selector::select(
            &opcode_type,
            [
                &(pc.clone() + E::F::one()),
                &imm,
                &(pc.clone() + imm.clone()),
            ],
        );
        let new_pc_value = eval.next_trace_mask();
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[new_pc.clone(), new_pc_value.clone()],
        ));

        // NEW_AP = { addap: ap+imm, jmp_*: ap}[opcode_type]
        let new_ap = Selector::select(&opcode_type, [&(ap.clone() + imm.clone()), &ap, &ap]);
        let new_ap_value = eval.next_trace_mask();
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[new_ap.clone(), new_ap_value.clone()],
        ));

        // Yield final state.
        let new_state = [new_pc, new_ap, fp];
        eval.add_to_relation(RelationEntry::new(
            &self.state_lookup,
            -E::EF::one(),
            &new_state,
        ));

        eval.finalize_logup_in_pairs();
        eval
    }
}

#[derive(Copy, Clone, Serialize, Deserialize)]
pub struct Claim {
    pub n_rows: usize,
}

impl Claim {
    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let log_size = std::cmp::max(self.n_rows.next_power_of_two().ilog2(), LOG_N_LANES);
        let preprocessed_log_sizes = vec![log_size];
        let trace_log_sizes = vec![log_size; ADD_AP_N_TRACE_CELLS];
        let interaction_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        TreeVec::new(vec![
            preprocessed_log_sizes,
            trace_log_sizes,
            interaction_log_sizes,
        ])
    }

    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.n_rows as u64);
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct InteractionClaim {
    pub log_size: u32,
    pub logup_sums: LogupSums,
}
impl InteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        let (total_sum, claimed_sum) = self.logup_sums;
        channel.mix_felts(&[total_sum]);
        if let Some(claimed_sum) = claimed_sum {
            channel.mix_felts(&[claimed_sum.0]);
            channel.mix_u64(claimed_sum.1 as u64);
        }
    }
}
