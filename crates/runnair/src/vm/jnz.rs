use num_traits::Zero;
use paste::paste;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use crate::memory::relocatable::{assert_and_project, MaybeRelocatable};
use crate::memory::Memory;
use crate::vm::jmp::{jmp_abs, jmp_abs_appp};
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn resolve_jnz_args(
    memory: &Memory,
    state: State,
    bases: &[&str; 2],
    offsets: &[M31; 2],
) -> [QM31; 2] {
    let [cond_addr, dest_addr] = resolve_addresses(state, bases, offsets);
    let Some(MaybeRelocatable::Absolute(condition)) = memory.get(cond_addr) else {
        panic!("Condition must be an absolute value.")
    };
    let Some(MaybeRelocatable::Absolute(destination)) = memory.get(dest_addr) else {
        panic!("Destination must be an absolute value.")
    };

    [condition, destination]
}

fn resolve_jnz_imm_args(
    memory: &Memory,
    state: State,
    base: &str,
    offsets: &[M31; 2],
) -> (M31, QM31) {
    let [dest_addr] = resolve_addresses(state, &[base], &[offsets[1]]);
    let Some(MaybeRelocatable::Absolute(destination)) = memory.get(dest_addr) else {
        panic!("Destination must be an absolute value.")
    };
    let condition = offsets[0];

    (condition, destination)
}

fn jnz<T: Zero>(state: State, condition: T, destination: QM31) -> State {
    if condition.is_zero() {
        state.advance()
    } else {
        jmp_abs(state, assert_and_project(destination))
    }
}

fn jnz_appp<T: Zero>(state: State, condition: T, destination: QM31) -> State {
    if condition.is_zero() {
        state.advance_and_increment_ap()
    } else {
        jmp_abs_appp(state, assert_and_project(destination))
    }
}

macro_rules! define_jnz {
    ($cond:ident, $dest:ident) => {
        paste! {
            /// Function without incrementing `ap`: `jnz_[ap/fp]_[ap/fp]`.
            pub fn [<jnz_ $cond _ $dest >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let [condition, destination] = resolve_jnz_args(
                    memory,
                    state,
                    &[stringify!($cond), stringify!($dest)],
                    &[args[0], args[1]],
                );
                jnz(state, condition, destination)
            }

            /// Function with incrementing `ap`: `jnz_[ap/fp]_[ap/fp][_appp]`.
            pub fn [<jnz_ $cond _ $dest _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let [condition, destination] = resolve_jnz_args(
                    memory,
                    state,
                    &[stringify!($cond), stringify!($dest)],
                    &[args[0], args[1]],
                );
                jnz_appp(state, condition, destination)
            }
        }
    };
}

macro_rules! define_jnz_imm {
    ($dest:ident) => {
        paste! {
            /// Function without incrementing `ap`: `jnz_imm_[ap/fp]`.
            pub fn [<jnz_imm_ $dest >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (condition, destination) = resolve_jnz_imm_args(
                    memory,
                    state,
                    stringify!($dest),
                    &[args[0], args[1]],
                );
                jnz(state, condition, destination)
            }

            /// Function with incrementing `ap`: `jnz_imm_[ap/fp]_appp`.
            pub fn [<jnz_imm_ $dest _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (condition, destination) = resolve_jnz_imm_args(
                    memory,
                    state,
                    stringify!($dest),
                    &[args[0], args[1]],
                );
                jnz_appp(state, condition, destination)
            }
        }
    };
}

define_jnz!(ap, ap);
define_jnz!(ap, fp);
define_jnz!(fp, ap);
define_jnz!(fp, fp);
define_jnz_imm!(ap);
define_jnz_imm!(fp);