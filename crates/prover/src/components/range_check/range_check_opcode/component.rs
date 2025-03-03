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
use crate::utils::component::{decode_opcode, is_bit};
use crate::utils::{Selector, SelectorTrait};

pub const N_TRACE_CELLS: usize = 12;

// TODO: organize opcodes so that K will work as detailed above, instead of just 0.
pub const INSTRUCTION_BASE: M31 = M31::from_u32_unchecked(0);

pub type Component = FrameworkComponent<Eval>;

pub struct Eval {
    pub claim: Claim,
    pub memory_relation_elements: MemoryRelation,
    pub range_check_14_lookup_elements: MemoryRelation,
    pub state_lookup: StateRelation,
}

impl FrameworkEval for Eval {
    fn log_size(&self) -> u32 {
        self.claim.log_size
    }

    fn max_constraint_log_degree_bound(&self) -> u32 {
        self.log_size() + 1
    }

    fn evaluate<E: EvalAtRow>(&self, mut eval: E) -> E {
        let state = std::array::from_fn(|_| eval.next_trace_mask());
        // Use initial state.
        eval.add_to_relation(RelationEntry::new(&self.state_lookup, E::EF::one(), &state));
        let [pc, ap, fp] = state;

        let reg = eval.next_trace_mask();
        eval.add_constraint(is_bit::<E>(&reg));

        // Check instruction.
        let opcode = decode_opcode(
            INSTRUCTION_BASE.into(),
            &[
                (reg.clone(), 2), // [ap, fp]
            ],
        );

        let [val, min, max, offset] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_to_relation(RelationEntry::new(
            &self.memory_relation_elements,
            E::EF::one(),
            &[
                pc.clone(),
                opcode.clone(),
                min.clone(),
                max.clone(),
                offset.clone(),
            ],
        ));

        eval.add_to_relation(RelationEntry::new(
            &self.memory_relation_elements,
            E::EF::one(),
            &[
                Selector::select(&reg, [&ap, &fp]) + offset.clone(),
                val.clone(),
            ],
        ));

        let [min_low, min_high, max_low, max_high] =
            std::array::from_fn(|_| eval.next_trace_mask());

        for rc14_part in [
            min_low.clone(),
            min_high.clone(),
            max_low.clone(),
            max_high.clone(),
        ] {
            eval.add_to_relation(RelationEntry::new(
                &self.range_check_14_lookup_elements,
                -E::EF::one(),
                &[rc14_part],
            ));
        }

        // 2 ** 14.
        let two_pow_14 = E::F::from(M31::from_u32_unchecked(2)).pow(14);

        // x is in RC28 iff it can be represented as `tmp_0 + 2**14 * tmp_1` where tmp_i \in RC14.
        let min_leq_val = min_low.clone() + two_pow_14.clone() * min_high.clone();
        eval.add_constraint(val.clone() - min.clone() - (min_leq_val));

        let val_leq_max = max_low.clone() + two_pow_14 * max_high.clone();
        eval.add_constraint(max.clone() - val.clone() - (val_leq_max));

        // Yield final state.
        let new_state = [pc + E::F::one(), ap, fp];
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
    pub log_size: u32,
}

impl Claim {
    pub fn log_sizes(&self) -> TreeVec<Vec<u32>> {
        let trace_log_sizes = vec![self.log_size; N_TRACE_CELLS];
        let interaction_log_sizes =
            vec![self.log_size; SECURE_EXTENSION_DEGREE * 3_usize.div_ceil(2)];
        TreeVec::new(vec![vec![], trace_log_sizes, interaction_log_sizes])
    }

    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_u64(self.log_size as u64);
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
