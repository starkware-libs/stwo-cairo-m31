use paste::paste;
use stwo_prover::core::fields::m31::M31;

use crate::memory::relocatable::assert_and_project;
use crate::memory::{MaybeRelocatableAddr, Memory};
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn resolve_destination_offset(
    memory: &Memory,
    state: State,
    base: &str,
    offset: M31,
) -> MaybeRelocatableAddr {
    let [offset_address] = resolve_addresses(state, &[base], &[offset]);
    let Some(destination_offset) = memory.get(offset_address) else {
        panic!("Destination offset cannot be deduced.")
    };

    assert_and_project(destination_offset)
}

fn push_return_fp_and_pc(memory: &mut Memory, state: State) {
    memory.insert(state.ap, state.fp);
    memory.insert(state.ap + M31(1), state.pc + M31(1));
}

fn call_rel(state: State, operand: impl Into<MaybeRelocatableAddr>) -> State {
    let next_ap = state.ap + M31(2);
    State {
        ap: next_ap,
        fp: next_ap,
        pc: state.pc + operand.into(),
    }
}

fn call_abs(state: State, operand: impl Into<MaybeRelocatableAddr>) -> State {
    let next_ap = state.ap + M31(2);
    State {
        ap: next_ap,
        fp: next_ap,
        pc: operand.into(),
    }
}

macro_rules! define_call {
    ($type:ident, $op:ident) => {
        paste! {
            pub(crate) fn [<call_ $type _ $op >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                push_return_fp_and_pc(memory, state);
                let destination_offset =
                    resolve_destination_offset(memory, state, stringify!($op), args[0]);
                [<call_ $type>](state, destination_offset)
            }
        }
    };
}

macro_rules! define_call_imm {
    ($type:ident) => {
        paste! {
            pub(crate) fn [<call_ $type _imm>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                push_return_fp_and_pc(memory, state);
                let immediate = args[0];
                [<call_ $type>](state, immediate)
            }
        }
    };
}

define_call!(abs, ap);
define_call!(abs, fp);
define_call!(rel, ap);
define_call!(rel, fp);
define_call_imm!(abs);
define_call_imm!(rel);

pub(crate) fn ret(memory: &mut Memory, state: State, _args: InstructionArgs) -> State {
    let Some(fp) = memory.get(state.fp - M31(2)) else {
        panic!("Previous `fp` cannot be deduced.")
    };

    let Some(pc) = memory.get(state.fp - M31(1)) else {
        panic!("Previous `pc` cannot be deduced.")
    };

    State {
        ap: state.ap,
        fp: assert_and_project(fp),
        pc: assert_and_project(pc),
    }
}
