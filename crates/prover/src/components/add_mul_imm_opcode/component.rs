use itertools::{chain, Itertools};
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
use crate::utils::component::{decode_opcode, is_bit};
use crate::utils::{Selector, SelectorTrait};

pub const N_TRACE_COLUMNS: usize = 20;
// TODO(alont): set instruction bases to not overlap
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

pub type Component = FrameworkComponent<Eval>;

#[derive(Clone)]
pub struct Eval {
    pub claim: Claim,
    pub memory_lookup: MemoryRelation,
    pub state_lookup: StateRelation,
}

impl Eval {
    pub fn new(claim: Claim, memory_lookup: MemoryRelation, state_lookup: StateRelation) -> Self {
        Self {
            claim: claim.clone(),
            memory_lookup,
            state_lookup,
        }
    }
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

        // Assert flags are in range.
        let [op_type, lhs_flag, rhs_flag, appp] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_constraint(is_bit::<E>(&op_type));
        eval.add_constraint(is_bit::<E>(&lhs_flag));
        eval.add_constraint(is_bit::<E>(&rhs_flag));
        eval.add_constraint(is_bit::<E>(&appp));

        // Check instruction.
        let [off0, off1, imm] = std::array::from_fn(|_| eval.next_trace_mask());
        let opcode = decode_opcode(
            INSTRUCTION_BASE.into(),
            &[
                (op_type.clone(), 2),  // [add, mul]
                (lhs_flag.clone(), 2), // [ap, fp]
                (rhs_flag.clone(), 2), // [ap, fp]
                (appp.clone(), 2),     // [false, true]
            ],
        );

        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[
                pc.clone(),
                opcode.clone(),
                off0.clone(),
                off1.clone(),
                imm.clone(),
            ],
        ));

        // Compute addresses.
        let [lhs_address, rhs_address] = std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_constraint(
            lhs_address.clone() - (Selector::select(&lhs_flag, [&ap, &fp]) + off0.clone()),
        );
        eval.add_constraint(
            rhs_address.clone() - (Selector::select(&rhs_flag, [&ap, &fp]) + off1.clone()),
        );

        // Read memory.
        let lhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        let rhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([lhs_address], lhs_val_arr.clone()).collect_vec(),
        ));

        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([rhs_address], rhs_val_arr.clone()).collect_vec(),
        ));

        let lhs_val = E::combine_ef(lhs_val_arr);
        let rhs_val = E::combine_ef(rhs_val_arr);

        // Apply operation.
        eval.add_constraint(
            lhs_val
                - (Selector::select(
                    &E::EF::from(op_type),
                    [
                        &(rhs_val.clone() + imm.clone()),
                        &(rhs_val.clone() * imm.clone()),
                    ],
                )),
        );

        // Yield final state.
        let new_state = [pc + E::F::one(), ap + appp, fp];
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
        let log_size = std::cmp::max(self.n_rows.next_power_of_two().ilog2(), LOG_N_LANES);
        let interaction_1_log_sizes = vec![log_size; N_TRACE_COLUMNS];
        let interaction_2_log_sizes = vec![log_size; SECURE_EXTENSION_DEGREE * 3];
        TreeVec::new(vec![
            vec![],
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
