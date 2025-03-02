use cairo_lang_casm::operand::{CellRef, Register, ResOperand};
use serde::{Deserialize, Serialize};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use crate::memory::relocatable::{MaybeRelocatable, Relocatable};
use crate::memory::Memory;
use crate::utils::usize_from_u32;
use crate::vm::{Input, State};

fn cell_ref_address(cell_ref: &CellRef, state: &State) -> MaybeRelocatable<M31> {
    let base_address = match cell_ref.register {
        Register::AP => state.ap,
        Register::FP => state.fp,
    };

    let offset: i32 = cell_ref.offset.into();

    let offset_m31: M31 = offset.into();
    base_address + offset_m31
}

// TODO: add custom (de)serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Hint {
    /// Writes a run argument of number `index` to `dst` and on.
    WriteRunParam { index: ResOperand, dst: CellRef },

    TestLessThanOrEqual {
        lhs: ResOperand,
        rhs: ResOperand,
        dst: CellRef,
    },
    /// Variant of TestLessThanOrEqual that compares addresses.
    TestLessThanOrEqualAddress {
        lhs: ResOperand,
        rhs: ResOperand,
        dst: CellRef,
    },
}

fn get_maybe(
    memory: &mut Memory,
    state: &State,
    res_operand: &ResOperand,
) -> Option<MaybeRelocatable<QM31>> {
    match res_operand {
        ResOperand::Deref(cell) => Some(memory[cell_ref_address(cell, state)]),
        ResOperand::DoubleDeref(..) => {
            todo!("not implemented")
        }
        ResOperand::Immediate(_) => {
            todo!("not implemented")
        }
        ResOperand::BinOp(_) => {
            todo!("not implemented")
        }
    }
}

impl Hint {
    fn execute(&self, memory: &mut Memory, state: &State, input: &Input) {
        match self {
            Self::WriteRunParam { index, dst } => {
                let ResOperand::Immediate(big_int) = index else {
                    panic!("index should be Immediate");
                };
                let index: usize = big_int.value.clone().try_into().unwrap();

                let [a, b, c, d] = input[index];

                memory.insert(
                    cell_ref_address(dst, state),
                    QM31::from_u32_unchecked(a, b, c, d),
                );
            }
            Self::TestLessThanOrEqual { lhs, rhs, dst }
            | Self::TestLessThanOrEqualAddress { lhs, rhs, dst } => {
                let lhs_val = get_maybe(memory, state, lhs).unwrap();
                let rhs_val = get_maybe(memory, state, rhs).unwrap();

                let result: MaybeRelocatable<QM31> =
                    if lhs_val <= rhs_val { M31(1) } else { M31(0) }.into();
                memory.insert(cell_ref_address(dst, state), result);
            }
        }
    }
}

pub(crate) type Hints = Vec<Option<Hint>>;

#[derive(Debug)]
pub(crate) struct HintRunner {
    pc_to_hint: Hints,
    input: Input,
}

impl HintRunner {
    pub(crate) fn new(pc_to_hint: Hints, input: Input) -> Self {
        Self { pc_to_hint, input }
    }

    pub(crate) fn maybe_execute_hint(&self, memory: &mut Memory, state: &State) {
        let MaybeRelocatable::Relocatable(Relocatable {
            segment: _,
            offset: pc,
        }) = state.pc
        else {
            panic!("`pc` must be a relocatable value.");
        };

        let pc = usize_from_u32(pc.0);
        if let Some(Some(hint)) = self.pc_to_hint.get(pc) {
            hint.execute(memory, state, &self.input);
        }
    }
}
