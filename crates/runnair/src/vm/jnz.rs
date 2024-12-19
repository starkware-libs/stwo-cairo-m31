use num_traits::Zero;
use paste::paste;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use crate::memory::relocatable::{assert_and_project, MaybeRelocatable};
use crate::memory::{MaybeRelocatableAddr, Memory};
use crate::vm::jmp::{jmp_rel, jmp_rel_appp};
use crate::vm::{resolve_addresses, InstructionArgs, State};

fn resolve_jnz_args(
    memory: &Memory,
    state: State,
    bases: &[&str; 2],
    offsets: &[M31; 2],
) -> (MaybeRelocatableAddr, QM31) {
    let [dest_addr, cond_addr] = resolve_addresses(state, bases, offsets);
    let Some(destination) = memory.get(dest_addr) else {
        panic!("Destination cannot be deduced.")
    };
    let Some(MaybeRelocatable::Absolute(condition)) = memory.get(cond_addr) else {
        panic!("Condition must be an absolute value.")
    };

    (assert_and_project(destination), condition)
}

fn resolve_jnz_imm_args(
    memory: &Memory,
    state: State,
    base: &str,
    offsets: &[M31; 2],
) -> (M31, QM31) {
    let [cond_addr] = resolve_addresses(state, &[base], &[offsets[1]]);
    let destination = offsets[0];
    let Some(MaybeRelocatable::Absolute(condition)) = memory.get(cond_addr) else {
        panic!("Condition must be an absolute value.")
    };

    (destination, condition)
}

fn jnz(state: State, destination: impl Into<MaybeRelocatableAddr>, condition: impl Zero) -> State {
    if condition.is_zero() {
        state.advance()
    } else {
        jmp_rel(state, destination.into())
    }
}

fn jnz_appp(
    state: State,
    destination: impl Into<MaybeRelocatableAddr>,
    condition: impl Zero,
) -> State {
    if condition.is_zero() {
        state.advance_and_increment_ap()
    } else {
        jmp_rel_appp(state, destination.into())
    }
}

macro_rules! define_jnz {
    ($cond:ident, $dest:ident) => {
        paste! {
            /// Jump-not-zero without incrementing `ap`: `jnz_[ap/fp]_[ap/fp]`.
            pub(crate) fn [<jnz_ $cond _ $dest >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (destination, condition) = resolve_jnz_args(
                    memory,
                    state,
                    &[stringify!($cond), stringify!($dest)],
                    &[args[0], args[1]],
                );
                jnz(state, destination, condition)
            }

            /// Jump-not-zero with incrementing `ap`: `jnz_[ap/fp]_[ap/fp][_appp]`.
            pub(crate) fn [<jnz_ $cond _ $dest _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (destination, condition) = resolve_jnz_args(
                    memory,
                    state,
                    &[stringify!($cond), stringify!($dest)],
                    &[args[0], args[1]],
                );
                jnz_appp(state, destination, condition)
            }
        }
    };
}

macro_rules! define_jnz_imm {
    ($dest:ident) => {
        paste! {
            /// Jump-not-zero without incrementing `ap`: `jnz_imm_[ap/fp]`.
            pub(crate) fn [<jnz_imm_ $dest >] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (destination, condition) = resolve_jnz_imm_args(
                    memory,
                    state,
                    stringify!($dest),
                    &[args[0], args[1]],
                );
                jnz(state, destination, condition)
            }

            /// Jump-not-zero with incrementing `ap`: `jnz_imm_[ap/fp]_appp`.
            pub(crate) fn [<jnz_imm_ $dest _appp>] (
                memory: &mut Memory,
                state: State,
                args: InstructionArgs,
            ) -> State {
                let (destination, condition) = resolve_jnz_imm_args(
                    memory,
                    state,
                    stringify!($dest),
                    &[args[0], args[1]],
                );
                jnz_appp(state, destination, condition)
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
