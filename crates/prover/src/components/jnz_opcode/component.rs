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

pub const N_TRACE_CELLS: usize = 19;

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
        let [op_type, reg] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_constraint(is_bit::<E>(&op_type));
        eval.add_constraint(is_bit::<E>(&reg));

        // Check instruction.
        let opcode = decode_opcode(
            INSTRUCTION_BASE.into(),
            &[
                (op_type.clone(), 2), // [jmp abs, jmp rel]
                (reg.clone(), 2),     // [ap, fp]
            ],
        );

        let [off, imm] = std::array::from_fn(|_| eval.next_trace_mask());
        println!("1");
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &[
                pc.clone(),
                opcode.clone(),
                off.clone(),
                imm.clone(),
            ],
        ));
        println!("2");
        // Compute address.
        let addr = eval.next_trace_mask();
        eval.add_constraint(
            addr.clone() - (Selector::select(&reg, [&(ap.clone()), &(fp.clone())]) + off),
        );

        println!("3");
        let addr_val_arr: [E::F; 4] = std::array::from_fn(|_| eval.next_trace_mask());
        eval.add_to_relation(RelationEntry::new(
            &self.memory_lookup,
            E::EF::one(),
            &chain!([addr], addr_val_arr.clone()).collect_vec(),
        ));
        let val = E::combine_ef(addr_val_arr); 


        // Check jnz condition.
        let maybe_inverse_val = E::combine_ef(std::array::from_fn(|_| eval.next_trace_mask()));
        println!("4");
        let flag = eval.next_trace_mask();
        eval.add_constraint(is_bit::<E>(&flag));

        // flag == 0 iff val == 0 iff val is not invertible <=>
        // ==> 0 = val * (1 - flag) + (1 - val * val^{-1}) * flag
        println!("5");
        eval.add_constraint(
            val.clone() * (E::F::one() - flag.clone())
                + (E::EF::one() - val.clone() * maybe_inverse_val) * flag.clone(),
        );
        
        // Assert new pc.
        // The relative branch when taken is obvious.
        println!("5.1");
        let jmp_target_if_taken =
            &Selector::select(&op_type, [&imm, &(pc.clone() + imm.clone())]);

        println!("5.2");
        let new_pc = eval.next_trace_mask();

        println!("6");
        eval.add_constraint(
            new_pc.clone() - Selector::select(&(E::F::one()-flag), [&(pc + E::F::one()), jmp_target_if_taken]),
        );

        // Yield final state.
        let new_state = [new_pc, ap, fp];
        println!("7");
        eval.add_to_relation(RelationEntry::new(
            &self.state_lookup,
            -E::EF::one(),
            &new_state,
        ));

        println!("8");
        eval.finalize_logup_in_pairs();
        println!("9");
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
        let trace_log_sizes = vec![log_size; N_TRACE_CELLS];
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
    pub claimed_sum: SecureField,
}
impl InteractionClaim {
    pub fn mix_into(&self, channel: &mut impl Channel) {
        channel.mix_felts(&[self.claimed_sum]);
    }
}
