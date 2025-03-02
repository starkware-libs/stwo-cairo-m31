use num_traits::One;
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

use crate::relations::{MemoryRelation, StateRelation};
use crate::utils::component::{decode_opcode, is_trit};
use crate::utils::{Selector, SelectorTrait};

pub const N_TRACE_CELLS: usize = 7;

// Assumes INSTRUCTION_BASE=K such that:
/// `
/// addap_imm = K
/// jmp_abs_imm = K + 1
/// jmp_rel_imm = K + 2
/// `
// TODO: organize opcodes so that K will work as detailed above, instead of just 0.
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

        // Assert opcode_type is in range: {0,1,2}.
        let opcode_type = eval.next_trace_mask();
        eval.add_constraint(is_trit::<E>(&opcode_type));

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
        // NEW_PC = { addap: pc + 1, jmp_imm: imm, jmp_rel: pc + imm }[opcode_name]
        let new_pc = Selector::select(
            &opcode_type,
            [
                &(pc.clone() + E::F::one()),
                &imm,
                &(pc.clone() + imm.clone()),
            ],
        );
        let new_pc_value = eval.next_trace_mask();
        eval.add_constraint(new_pc.clone() - new_pc_value.clone());

        // NEW_AP = { addap: ap+imm, jmp_*: ap}[opcode_name]
        let new_ap = Selector::select(&opcode_type, [&(ap.clone() + imm.clone()), &ap, &ap]);
        let new_ap_value = eval.next_trace_mask();
        eval.add_constraint(new_ap.clone() - new_ap_value.clone());

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
        let trace_log_sizes = vec![log_size; N_TRACE_CELLS];
        let interaction_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        TreeVec::new(vec![vec![], trace_log_sizes, interaction_log_sizes])
    }

    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.n_rows as u64);
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
