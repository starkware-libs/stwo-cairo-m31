use paste::paste;
use stwo_prover::core::fields::m31::M31;

use crate::memory::relocatable::assert_and_project;
use crate::memory::{MaybeRelocatableValue, Memory};
use crate::vm::{InstructionArgs, State};

fn addap(state: State, summand: M31) -> State {
    State {
        ap: state.ap + summand,
        fp: state.fp,
        pc: state.pc + M31(1),
    }
}

macro_rules! addap_with_operand {
    ($operand:ident) => {
        paste! {
            pub(crate) fn [<addap_ $operand>](
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                if let MaybeRelocatableValue::Absolute(summand) =
                    crate::vm::operand::$operand(memory, state, &args)
                {
                    addap(state, assert_and_project(summand))
                } else {
                    panic!("Can't addap by a relocatable.")
                }
            }
        }
    };
}

addap_with_operand!(add_ap_ap);
addap_with_operand!(add_ap_fp);
addap_with_operand!(add_fp_ap);
addap_with_operand!(add_fp_fp);
addap_with_operand!(add_imm_ap);
addap_with_operand!(add_imm_fp);
addap_with_operand!(deref_ap);
addap_with_operand!(deref_fp);
addap_with_operand!(double_deref_ap);
addap_with_operand!(double_deref_fp);
addap_with_operand!(imm);
addap_with_operand!(mul_ap_ap);
addap_with_operand!(mul_ap_fp);
addap_with_operand!(mul_fp_ap);
addap_with_operand!(mul_fp_fp);
addap_with_operand!(mul_imm_ap);
addap_with_operand!(mul_imm_fp);
