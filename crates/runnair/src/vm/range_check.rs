use num_traits::Zero;
use paste::paste;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use crate::memory::relocatable::MaybeRelocatable;
use crate::memory::Memory;
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn resolve_range_check_arg(memory: &Memory, state: State, base: &str, offset: M31) -> QM31 {
    let [checked_addr] = resolve_addresses(state, &[base], &[offset]);
    let opt_val =  memory.get(checked_addr);
  
    let Some(MaybeRelocatable::Absolute(checked_value)) = opt_val else {
          panic!(
            "Condition must be an absolute value. Got: {:?} at {:?}",
            opt_val, checked_addr
        )
    };
    checked_value
}

fn range_check(state: State, value: QM31, lower: M31, upper: M31) -> State {
    assert_eq!(value.1 .0, M31::zero());
    assert_eq!(value.1 .1, M31::zero());
    assert_eq!(value.0 .1, M31::zero());
    assert!(value.0 .0 >= lower);
    assert!(value.0 .0 <= upper);
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
