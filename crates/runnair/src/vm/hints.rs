use cairo_lang_casm::operand::{CellRef, Register, ResOperand};
use num_traits::Zero;
use serde::{Deserialize, Serialize};
use stwo_prover::core::fields::m31::{self, M31};
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

    let value: i32 = cell_ref.offset.into();

    let m31_offset = if value < 0 {
        const P2: u64 = 2 * m31::P as u64;
        M31::reduce(P2 - value.unsigned_abs() as u64)
    } else {
        M31::reduce(value.unsigned_abs() as u64)
    };

    base_address + m31_offset
}

// TODO: add custom (de)serialization.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Hint {
    /// Writes a run argument of number `index` to `dst` and on.
    WriteRunParam {
        index: ResOperand,
        dst: CellRef,
    },
    TestLessThan {
        lhs: ResOperand,
        rhs: ResOperand,
        dst: CellRef,
    },
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
    AllocSegment {
        dst: CellRef,
    },
    AddMarker {
        start: ResOperand,
        end: ResOperand,
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
        ResOperand::Immediate(big_int) => {
            let val: u32 = big_int.value.clone().try_into().unwrap();
            let as_m31: M31 = val.into();
            Some(QM31::from_m31(as_m31, M31::zero(), M31::zero(), M31::zero()).into())
        }
        ResOperand::BinOp(_) => {
            todo!("not implemented")
        }
    }
}

impl Hint {
    fn execute(&self, memory: &mut Memory, state: &State, input: &Input, next_segment: &mut usize) {
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
            Self::TestLessThan { lhs, rhs, dst } => {
                let lhs_val = get_maybe(memory, state, lhs).unwrap();
                let rhs_val = get_maybe(memory, state, rhs).unwrap();

                let result: MaybeRelocatable<QM31> =
                    if lhs_val < rhs_val { M31(1) } else { M31(0) }.into();

                println!("dst: {:?}", cell_ref_address(dst, state));
                memory.insert(cell_ref_address(dst, state), result);
            }
            Self::TestLessThanOrEqual { lhs, rhs, dst }
            | Self::TestLessThanOrEqualAddress { lhs, rhs, dst } => {
                let lhs_val = get_maybe(memory, state, lhs).unwrap();
                let rhs_val = get_maybe(memory, state, rhs).unwrap();

                let result: MaybeRelocatable<QM31> =
                    if lhs_val <= rhs_val { M31(1) } else { M31(0) }.into();
                println!("dst: {:?}", cell_ref_address(dst, state));
                memory.insert(cell_ref_address(dst, state), result);
            }

            Self::AllocSegment { dst } => {
                let segment = *next_segment;
                *next_segment += 1;
                memory.insert(
                    cell_ref_address(dst, state),
                    MaybeRelocatable::<M31>::Relocatable(Relocatable {
                        segment,
                        offset: M31(0),
                    }),
                );
            }
            Self::AddMarker { .. } => {
                // TODO(ilya): Implement.
            }
        }
    }
}

pub(crate) type Hints = Vec<Option<Hint>>;

#[derive(Debug)]
pub(crate) struct HintRunner {
    pc_to_hint: Hints,
    input: Input,
    next_segment: usize,
}

impl HintRunner {
    pub(crate) fn new(pc_to_hint: Hints, input: Input) -> Self {
        for (pc, hint) in pc_to_hint.iter().enumerate() {
            if let Some(hint) = hint {
                println!("hint at pc {}: {:#?}", pc, hint);
            }
        }

        Self {
            pc_to_hint,
            input,
            next_segment: 5,
        }
    }

    pub(crate) fn maybe_execute_hint(&mut self, memory: &mut Memory, state: &State) {
        let MaybeRelocatable::Relocatable(Relocatable {
            segment: _,
            offset: pc,
        }) = state.pc
        else {
            panic!("`pc` must be a relocatable value.");
        };

        let pc = usize_from_u32(pc.0);
        println!("looking for hint at pc: {}", pc);
        if let Some(Some(hint)) = self.pc_to_hint.get(pc) {
            println!("executing");
            hint.execute(memory, state, &self.input, &mut self.next_segment);
        }
    }
}
