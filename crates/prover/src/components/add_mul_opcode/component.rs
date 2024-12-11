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
use stwo_prover::core::fields::FieldExpOps;
use stwo_prover::core::pcs::TreeVec;

use crate::relations::{MemoryRelation, StateRelation};

pub const N_TRACE_CELLS: usize = 26;
// TODO(alont): set instruction bases to not overlap
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

// TODO(alont) put these in a common file.
pub const IMM_BASE: M31 = M31::from_u32_unchecked(1 << 29);
pub const TWO_INV: M31 = M31(2).inverse();
// TODO(alont) document this!!
pub fn select_trit<E: EvalAtRow>(trit: E::F, a: E::F, b: E::F, c: E::F) -> E::F {
    let trit_minus_one = trit - E::F::one();
    let trit_minus_two = trit - E::F::from(M31(2));
    let two_inv = E::F::from(TWO_INV);

    (two_inv * trit_minus_one * trit_minus_two * a) + (two_inv * trit * trit_minus_one * b)
        - (trit * trit_minus_two * c)
}

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
        eval.add_to_relation(&[RelationEntry::new(&self.state_lookup, E::EF::one(), &state)]);
        let [ap, fp, pc] = state;

        // Assert flags are in range.
        let [is_mul, reg0, reg1, reg2, appp] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_constraint(is_mul * is_mul - is_mul);
        eval.add_constraint(reg0 * reg0 - reg0);
        eval.add_constraint(reg2 * reg2 - reg2);
        eval.add_constraint(appp * appp - appp);
        eval.add_constraint((reg1 - E::F::from(M31(2))) * (reg1 - E::F::one()) * reg1);

        // Check instruction.
        let [off0, off1, off2] = std::array::from_fn(|_| eval.next_trace_mask());
        let opcode = E::F::from(INSTRUCTION_BASE)
            + is_mul
            + E::F::from(M31(2)) * reg0
            + E::F::from(M31(4)) * reg1
            + E::F::from(M31(12)) * reg2
            + E::F::from(M31(24)) * appp;
        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[pc, opcode, off0, off1, off2],
        )]);

        // Compute addresses.
        let [dst_address, lhs_address, rhs_address] =
            std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_constraint(dst_address - (reg0 * fp + (E::F::one() - reg0) * ap + off0));
        eval.add_constraint(rhs_address - (reg2 * fp + (E::F::one() - reg2) * ap + off2));
        eval.add_constraint(
            lhs_address - (select_trit::<E>(reg1, ap, fp, E::F::from(IMM_BASE)) + off1),
        );

        // Read memory.
        let dst_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        let lhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        let rhs_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());

        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([dst_address], dst_val_arr).collect_vec(),
        )]);

        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([lhs_address], lhs_val_arr).collect_vec(),
        )]);

        eval.add_to_relation(&[RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([rhs_address], rhs_val_arr).collect_vec(),
        )]);

        let dst_val = E::combine_ef(dst_val_arr);
        let lhs_val = E::combine_ef(lhs_val_arr);
        let rhs_val = E::combine_ef(rhs_val_arr);

        // Apply operation.
        eval.add_constraint(
            dst_val
                - ((E::EF::from(is_mul) * lhs_val * rhs_val)
                    + (E::EF::one() - E::EF::from(is_mul)) * (lhs_val + rhs_val)),
        );

        // Yield final state.
        let new_state = [ap + appp, fp, pc + E::F::one()];
        eval.add_to_relation(&[RelationEntry::new(
            &self.state_lookup,
            -E::EF::one(),
            &new_state,
        )]);

        eval.finalize_logup();
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
        let interaction_0_log_sizes = vec![log_size; N_TRACE_CELLS];
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
