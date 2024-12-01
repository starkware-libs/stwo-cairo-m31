use paste::paste;
use stwo_prover::core::fields::m31::M31;

use crate::memory::{MaybeRelocatableAddr, Memory};
use crate::vm::{InstructionArgs, State};

fn resolve_addresses<const N: usize>(
    state: State,
    bases: &[&str; N],
    offsets: &[M31; N],
) -> [MaybeRelocatableAddr; N] {
    assert!(
        bases.len() <= 3,
        "The number of bases and offsets should not exceed 3"
    );

    std::array::from_fn(|i| {
        let base = bases[i];
        let base_address = match base {
            "ap" => state.ap,
            "fp" => state.fp,
            _ => panic!("Invalid base: {}", base),
        };
        MaybeRelocatableAddr::Absolute(base_address + offsets[i])
    })
}

fn assign_or_assert_add_on_memory(
    memory: &mut Memory,
    dest: MaybeRelocatableAddr,
    op1: MaybeRelocatableAddr,
    op2: MaybeRelocatableAddr,
) {
    match (memory.get(dest), memory.get(op1), memory.get(op2)) {
        (Some(dest_val), Some(op0_val), Some(op1_val)) => {
            assert_eq!(dest_val, op0_val + op1_val, "Assertion failed.");
        }
        (None, Some(op0_val), Some(op1_val)) => {
            let deduced_value = op0_val + op1_val;
            memory.insert(dest, deduced_value);
        }
        (Some(dest_val), None, Some(op1_val)) => {
            let deduced_value = dest_val - op1_val;
            memory.insert(op1, deduced_value);
        }
        (Some(dest_val), Some(op0_val), None) => {
            let deduced_value = dest_val - op0_val;
            memory.insert(op2, deduced_value);
        }
        _ => panic!("Cannot deduce more than one operand"),
    };
}

// TODO: handle imm.
// TODO: handle mul.
macro_rules! define_assert {
    ($dest:ident, $op1:ident, $op2:ident) => {
        paste! {
            /// Function without incrementing `ap`: `assert_[ap/fp]_add_[ap/fp]_[ap/fp]`.
            pub fn [<assert_ $dest _add_ $op1 _ $op2>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));

                let addresses = resolve_addresses(state, &[dest, op1, op2], &args);
                assign_or_assert_add_on_memory(memory, addresses[0], addresses[1], addresses[2]);

                state.advance()
            }

            /// Function with incrementing `ap`: `assert_[ap/fp]_add_[ap/fp]_[ap/fp][_appp]`.
            pub fn [<assert_ $dest _add_ $op1 _ $op2 _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (dest, op1, op2) = (stringify!($dest), stringify!($op1), stringify!($op2));

                let addresses = resolve_addresses(state, &[dest, op1, op2], &args);
                assign_or_assert_add_on_memory(memory, addresses[0], addresses[1], addresses[2]);

                state.advance_and_increment_ap()
            }
        }
    };
}

define_assert!(ap, ap, ap);
define_assert!(ap, ap, fp);
define_assert!(ap, fp, ap);
define_assert!(ap, fp, fp);
define_assert!(fp, ap, ap);
define_assert!(fp, ap, fp);
define_assert!(fp, fp, ap);
define_assert!(fp, fp, fp);

// TODO: add tests.
