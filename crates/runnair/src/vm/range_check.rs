use crate::memory::Memory;
use crate::vm::{InstructionArgs, State};

pub(crate) fn range_check_ap(memory: &mut Memory, state: State, args: InstructionArgs) -> State {
    let (min, max) = (args[0].into(), args[1].into());
    let value = crate::vm::operand::range_check_ap(memory, state, &args);
    assert!(value >= min && value <= max);
    state.advance()
}

pub(crate) fn range_check_fp(memory: &mut Memory, state: State, args: InstructionArgs) -> State {
    let (min, max) = (args[0].into(), args[1].into());
    let value = crate::vm::operand::range_check_fp(memory, state, &args);
    assert!(value >= min && value <= max);
    state.advance()
}
