use itertools::{chain, Itertools};
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

use crate::relations::{MemoryRelation, StateRelation};
use crate::utils::component::{decode_opcode, is_bit};
use crate::utils::{Selector, SelectorTrait};

pub const N_TRACE_COLUMNS: usize = 27;
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
        self.claim.log_size
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let mult = eval.next_trace_mask();
        eval.add_constraint(is_bit::<E>(&mult));

        let state = std::array::from_fn(|_| eval.next_trace_mask());
        // Use initial state.
        eval.add_to_relation(RelationEntry::new(
            &self.state_lookup,
            mult.clone().into(),
            &state,
        ));
        let [pc, ap, fp] = state;

        // Assert flags are in range.
        let [op_type, reg0, reg1, reg2, appp] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_constraint(op_type.clone() * op_type.clone() - op_type.clone());
        eval.add_constraint(reg0.clone() * reg0.clone() - reg0.clone());
        eval.add_constraint(reg1.clone() * reg1.clone() - reg1.clone());
        eval.add_constraint(reg2.clone() * reg2.clone() - reg2.clone());
        eval.add_constraint(appp.clone() * appp.clone() - appp.clone());

        // Check instruction.
        let [off0, off1, off2] = std::array::from_fn(|_| eval.next_trace_mask());
        let opcode = decode_opcode(
            INSTRUCTION_BASE.into(),
            &[
                (op_type.clone(), 2), // [add, mul]
                (reg0.clone(), 2),    // [ap, fp]
                (reg1.clone(), 2),    // [ap, fp]
                (reg2.clone(), 2),    // [ap, fp]
                (appp.clone(), 2),    // [false, true]
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
                off2.clone(),
            ],
        ));

        // Compute addresses.
        let [dst_address, lhs_address, rhs_address] =
            std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_constraint(
            dst_address.clone() - (Selector::select(&reg0, [&ap, &fp]) + off0.clone()),
        );
        eval.add_constraint(
            lhs_address.clone() - (Selector::select(&reg1, [&ap, &fp]) + off1.clone()),
        );
        eval.add_constraint(
            rhs_address.clone() - (Selector::select(&reg2, [&ap, &fp]) + off2.clone()),
        );

        // Read memory.
        let dst_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        let lhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        let rhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([dst_address], dst_val_arr.clone()).collect_vec(),
        ));

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

        let dst_val = E::combine_ef(dst_val_arr);
        let lhs_val = E::combine_ef(lhs_val_arr);
        let rhs_val = E::combine_ef(rhs_val_arr);

        // Apply operation.
        eval.add_constraint(
            dst_val
                - (Selector::select(
                    &E::EF::from(op_type),
                    [
                        &(lhs_val.clone() + rhs_val.clone()),
                        &(lhs_val.clone() * rhs_val.clone()),
                    ],
                )),
        );

        // Yield final state.
        let new_state = [pc + E::F::one(), ap + appp, fp];
        eval.add_to_relation(RelationEntry::new(
            &self.state_lookup,
            -E::EF::one() * mult,
            &new_state,
        ));

        eval.finalize_logup_in_pairs();
        eval
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Claim {
    pub log_size: u32,
}
impl Claim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.log_size as u64);
    }

    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let interaction_1_log_sizes = vec![self.log_size; N_TRACE_COLUMNS];
        let interaction_2_log_sizes = vec![self.log_size; SECURE_EXTENSION_DEGREE * 3];
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
