use stwo_prover::core::fields::m31::M31;

use crate::memory::relocatable::assert_and_project;
use crate::memory::{MaybeRelocatableValue, Memory};
use crate::vm::{immediates_segment_base, State};

fn read_imm(memory: &Memory, offset: M31) -> MaybeRelocatableValue {
    memory[immediates_segment_base() + offset]
}

// Adds:
pub(crate) fn add_ap_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[0]] + memory[state.ap + args[1]]
}

pub(crate) fn add_ap_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[0]] + memory[state.fp + args[1]]
}

pub(crate) fn add_fp_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[0]] + memory[state.ap + args[1]]
}

pub(crate) fn add_fp_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[0]] + memory[state.fp + args[1]]
}

pub(crate) fn add_imm_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[1]] + read_imm(memory, args[0])
}

pub(crate) fn add_imm_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[1]] + read_imm(memory, args[0])
}

// Muls:
pub(crate) fn mul_ap_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[0]] * memory[state.ap + args[1]]
}

pub(crate) fn mul_ap_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[0]] * memory[state.fp + args[1]]
}

pub(crate) fn mul_fp_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[0]] * memory[state.ap + args[1]]
}

pub(crate) fn mul_fp_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[0]] * memory[state.fp + args[1]]
}

pub(crate) fn mul_imm_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[1]] * read_imm(memory, args[0])
}

pub(crate) fn mul_imm_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[1]] * read_imm(memory, args[0])
}

// Derefs:
pub(crate) fn imm(memory: &Memory, _state: State, args: &[M31]) -> MaybeRelocatableValue {
    read_imm(memory, args[0])
}

pub(crate) fn deref_ap(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.ap + args[0]]
}

pub(crate) fn deref_fp(memory: &Memory, state: State, args: &[M31]) -> MaybeRelocatableValue {
    memory[state.fp + args[0]]
}

pub(crate) fn double_deref_ap(
    memory: &Memory,
    state: State,
    args: &[M31],
) -> MaybeRelocatableValue {
    let address = assert_and_project(memory[state.ap + args[0]] + args[1]);
    memory[address]
}

pub(crate) fn double_deref_fp(
    memory: &Memory,
    state: State,
    args: &[M31],
) -> MaybeRelocatableValue {
    let address = assert_and_project(memory[state.fp + args[0]] + args[1]);
    memory[address]
}
