use num_traits::Zero;
use paste::paste;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use crate::memory::relocatable::MaybeRelocatable;
use crate::memory::Memory;
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn resolve_range_check_arg(memory: &Memory, state: State, base: &str, offset: M31) -> QM31 {
    let [checked_addr] = resolve_addresses(state, &[base], &[offset]);
    let Some(MaybeRelocatable::Absolute(checked_value)) = memory.get(checked_addr) else {
        panic!("Range checked value must be an absolute value.")
    };
    checked_value
}

fn range_check(state: State, value: QM31, lower: M31, upper: M31) -> State {
    assert_eq!(value.1 .0, M31::zero());
    assert_eq!(value.1 .1, M31::zero());
    assert_eq!(value.0 .1, M31::zero());
    assert!(value.0 .0 >= lower);
    assert!(value.0 .0 >= upper);
    state.advance()
}

macro_rules! define_range_check {
    ($value:ident) => {
        paste! {
            /// Range check opcode: `range_check_[ap/fp]`.
            pub(crate) fn [<range_check_ $value >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let value = resolve_range_check_arg(
                    memory,
                    state,
                    stringify!($value),
                    args[2],
                );
                range_check(state, value, args[0], args[1])
            }
        }
    };
}

define_range_check!(ap);
define_range_check!(fp);
