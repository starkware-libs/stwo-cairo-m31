use stwo_prover::core::fields::m31::M31;

use crate::vm::State;

#[derive(Debug, Default)]
pub(crate) struct AirStates {
    add_ap_or_jmp: Vec<State>,
    add_or_mul: Vec<State>,
    add_imm_or_mul_imm: Vec<State>,
    call: Vec<State>,
    deref_or_double_deref: Vec<State>,
    jnz: Vec<State>,
    ret: Vec<State>,
}

impl AirStates {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn add_state(&mut self, opcode: M31, state: &State) {
        let state = *state;
        match opcode.0 {
            0..=16 | 91..=158 => self.add_ap_or_jmp.push(state),
            17..=24 | 39..=46 | 51..=58 | 73..=80 => self.add_or_mul.push(state),
            25..=28 | 37..=38 | 47..=50 | 59..=62 | 71..=72 | 81..=84 => {
                self.add_imm_or_mul_imm.push(state)
            }
            85..=90 => self.call.push(state),
            29..=36 | 63..=70 => self.deref_or_double_deref.push(state),
            159..=170 => self.jnz.push(state),
            171 => self.ret.push(state),
            _ => panic!("Unknown opcode: {}.", opcode),
        }
    }
}
