use paste::paste;
use stwo_prover::core::fields::m31::M31;

use crate::memory::relocatable::assert_and_project;
use crate::memory::{MaybeRelocatableAddr, MaybeRelocatableValue, Memory};
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn assign_or_assert_deref_on_memory(
    memory: &mut Memory,
    dest_addr: MaybeRelocatableAddr,
    op1_addr: impl Into<MaybeRelocatableAddr>,
    op1_val: Option<MaybeRelocatableValue>,
) {
    match (memory.get(dest_addr), op1_val) {
        (Some(dest_val), Some(op1_val)) => {
            assert_eq!(dest_val, op1_val, "Assertion failed.");
        }
        (Some(dest_val), None) => {
            memory.insert(op1_addr, dest_val);
        }
        (None, Some(op1_val)) => {
            memory.insert(dest_addr, op1_val);
        }
        _ => panic!("Cannot deduce more than one operand"),
    };
}

fn assign_or_assert_deref(memory: &mut Memory, state: State, bases: &[&str; 2], args: &[M31; 2]) {
    let [dest, op1] = bases;
    let [dest_addr, op1_addr] = resolve_addresses(state, &[dest, op1], args);
    let op1_val = memory.get(op1_addr);

    assign_or_assert_deref_on_memory(memory, dest_addr, op1_addr, op1_val)
}

macro_rules! define_assert_deref {
    ($dest:ident, $op1:ident) => {
        paste! {
            /// Assert deref without incrementing `ap`: `assert_[ap/fp]_deref_[ap/fp]`.
            pub fn [<assert_ $dest _deref_ $op1>](
                memory: &mut Memory,
                state: State,
                args: InstructionArgs
            ) -> State {
                assign_or_assert_deref(
                    memory,
                    state,
                    &[stringify!($dest), stringify!($op1)],
                    &[args[0], args[1]],
                );
                state.advance()
            }

            /// Assert deref with incrementing `ap`: `assert_[ap/fp]_deref_[ap/fp]_appp`.
            pub fn [<assert_ $dest _deref_ $op1 _appp>](
                memory: &mut Memory,
                state: State,
                args: InstructionArgs
            ) -> State {
                assign_or_assert_deref(
                    memory,
                    state,
                    &[stringify!($dest), stringify!($op1)],
                    &[args[0], args[1]],
                );
                state.advance_and_increment_ap()
            }
        }
    };
}

fn assign_or_assert_double_deref(
    memory: &mut Memory,
    state: State,
    bases: &[&str; 2],
    args: &[M31; 3],
) {
    let [dest, inner_offset] = bases;
    let [dest_addr, inner_addr] =
        resolve_addresses(state, &[dest, inner_offset], &[args[0], args[1]]);
    let Some(outer_addr_base) = memory.get(inner_addr) else {
        panic!("Cannot deduce inner address of a double dereference");
    };
    let outer_addr = assert_and_project(outer_addr_base + args[2]);
    let outer_val = memory.get(outer_addr);

    assign_or_assert_deref_on_memory(memory, dest_addr, outer_addr, outer_val)
}

macro_rules! define_assert_double_deref {
    ($dest:ident, $op1:ident) => {
        paste! {
            /// Assert double deref without incrementing `ap`:
            /// `assert_[ap/fp]_double_deref_[ap/fp]`.
            pub fn [<assert_ $dest _double_deref_ $op1>](
                memory: &mut Memory,
                state: State,
                args: InstructionArgs
            ) -> State {
                assign_or_assert_double_deref(
                    memory,
                    state,
                    &[stringify!($dest), stringify!($op1)],
                    &args,
                );
                state.advance()
            }

            /// Assert double deref with incrementing `ap`:
            /// `assert_[ap/fp]_double_deref_[ap/fp]_appp`.
            pub fn [<assert_ $dest _double_deref_ $op1 _appp>](
                memory: &mut Memory,
                state: State,
                args: InstructionArgs
            ) -> State {
                assign_or_assert_double_deref(
                    memory,
                    state,
                    &[stringify!($dest), stringify!($op1)],
                    &args,
                );
                state.advance_and_increment_ap()
            }
        }
    };
}

define_assert_deref!(ap, ap);
define_assert_deref!(ap, fp);
define_assert_deref!(fp, ap);
define_assert_deref!(fp, fp);
define_assert_double_deref!(ap, ap);
define_assert_double_deref!(ap, fp);
define_assert_double_deref!(fp, ap);
define_assert_double_deref!(fp, fp);
