use paste::paste;
use stwo_prover::core::fields::m31::M31;

use crate::memory::{MaybeRelocatableValue, Memory};
use crate::vm::{resolve_addresses, InstructionArgs, State};

enum Operation {
    Add,
    Mul,
}

impl Operation {
    fn apply(
        self,
        x: MaybeRelocatableValue,
        y: impl Into<MaybeRelocatableValue>,
    ) -> MaybeRelocatableValue {
        match self {
            Operation::Add => x + y.into(),
            Operation::Mul => x * y.into(),
        }
    }

    fn deduce(
        self,
        x: MaybeRelocatableValue,
        y: impl Into<MaybeRelocatableValue>,
    ) -> MaybeRelocatableValue {
        match self {
            Operation::Add => x - y.into(),
            Operation::Mul => x / y.into(),
        }
    }
}

fn assign_or_assert_operation(
    memory: &mut Memory,
    state: State,
    operation: Operation,
    bases: &[&str; 3],
    args: &[M31; 3],
) {
    let [dest, op1, op2] = bases;
    let [dest_addr, op1_addr, op2_addr] = resolve_addresses(state, &[dest, op1, op2], args);

    match (
        memory.get(dest_addr),
        memory.get(op1_addr),
        memory.get(op2_addr),
    ) {
        (Some(dest_val), Some(op1_val), Some(op2_val)) => {
            assert_eq!(
                dest_val,
                operation.apply(op1_val, op2_val),
                "Assertion failed."
            );
        }
        (None, Some(op1_val), Some(op2_val)) => {
            memory.insert(dest_addr, operation.apply(op1_val, op2_val));
        }
        (Some(dest_val), None, Some(op2_val)) => {
            memory.insert(op1_addr, operation.deduce(dest_val, op2_val));
        }
        (Some(dest_val), Some(op1_val), None) => {
            memory.insert(op2_addr, operation.deduce(dest_val, op1_val));
        }
        _ => panic!("Cannot deduce more than one operand"),
    };
}

// TODO: handle mul.
macro_rules! define_assert {
    ($dest:ident, $op1:ident, $op2:ident) => {
        paste! {
            /// Assert add without incrementing `ap`: `assert_[ap/fp]_add_[ap/fp/imm]_[ap/fp]`.
            pub(crate) fn [<assert_ $dest _add_ $op1 _ $op2>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));
                assign_or_assert_operation(memory, state, Operation::Add, &[dest, op1, op2], &args);
                state.advance()
            }

            /// Assert add with incrementing `ap`: `assert_[ap/fp]_add_[ap/fp/imm]_[ap/fp][_appp]`.
            pub(crate) fn [<assert_ $dest _add_ $op1 _ $op2 _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));
                assign_or_assert_operation(memory, state, Operation::Add, &[dest, op1, op2], &args);
                state.advance_and_increment_ap()
            }

            /// Assert mul without incrementing `ap`: `assert_[ap/fp]_mul_[ap/fp/imm]_[ap/fp]`.
            pub(crate) fn [<assert_ $dest _mul_ $op1 _ $op2>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));
                assign_or_assert_operation(memory, state, Operation::Mul, &[dest, op1, op2], &args);
                state.advance()
            }

            /// Assert mul with incrementing `ap`: `assert_[ap/fp]_mul_[ap/fp/imm]_[ap/fp][_appp]`.
            pub(crate) fn [<assert_ $dest _mul_ $op1 _ $op2 _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));
                assign_or_assert_operation(memory, state, Operation::Mul, &[dest, op1, op2], &args);
                state.advance_and_increment_ap()
            }
        }
    };
}

fn assign_or_assert_imm(memory: &mut Memory, state: State, base: &str, offsets: &[M31; 2]) {
    let [dest_addr] = resolve_addresses(state, &[base], &[offsets[0]]);
    let immediate = MaybeRelocatableValue::Absolute(offsets[1].into());

    if let Some(dest_val) = memory.get(dest_addr) {
        assert_eq!(dest_val, immediate, "Assertion failed.");
    } else {
        memory.insert(dest_addr, immediate);
    };
}

macro_rules! define_assert_imm {
    ($dest:ident) => {
        paste! {
            /// Assert immediate without incrementing `ap`: `assert_[ap/fp]_imm`.
            pub(crate) fn [<assert_ $dest _imm>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                assign_or_assert_imm(memory, state, stringify!($dest), &[args[0], args[1]]);
                state.advance()
            }

            /// Assert immediate with incrementing `ap`: `assert_[ap/fp]_imm_appp`.
            pub(crate) fn [<assert_ $dest _imm_appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                assign_or_assert_imm(memory, state, stringify!($dest), &[args[0], args[1]]);
                state.advance_and_increment_ap()
            }
        }
    };
}

define_assert!(ap, ap, ap);
define_assert!(ap, ap, fp);
define_assert!(ap, fp, ap);
define_assert!(ap, fp, fp);
define_assert!(ap, imm, ap);
define_assert!(ap, imm, fp);
define_assert!(fp, ap, ap);
define_assert!(fp, ap, fp);
define_assert!(fp, fp, ap);
define_assert!(fp, fp, fp);
define_assert!(fp, imm, ap);
define_assert!(fp, imm, fp);
define_assert_imm!(ap);
define_assert_imm!(fp);

// TODO: add tests.
