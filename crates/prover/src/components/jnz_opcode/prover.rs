use std::simd::cmp::SimdPartialEq;
use std::simd::Simd;

use itertools::{zip_eq, Itertools};
use num_traits::{One, Zero};
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use stwo_air_utils::trace::component_trace::ComponentTrace;
use stwo_air_utils_derive::{IterMut, ParIterMut, Uninitialized};
use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::constraint_framework::Relation;
use stwo_prover::core::backend::simd::m31::{PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::qm31::PackedQM31;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::FieldExpOps;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::utils::bit_reverse_coset_to_circle_domain_order;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim};
use crate::components::addap_jmpabs_jmprel_opcode::component::INSTRUCTION_BASE;
use crate::components::memory;
use crate::input::instructions::VmState;
use crate::relations::{MemoryRelation, StateRelation, N_MEMORY_ELEMS, STATE_SIZE};
use crate::utils::prover::decode_opcode;
use crate::utils::types::{CasmState, PackedCasmState};
use crate::utils::{Selector, SelectorTrait};

const N_TRACE_COLUMNS: usize = 18;
const N_MEMORY_LOOKUPS: usize = 2;
const N_STATE_LOOKUPS: usize = 2;

#[derive(Debug)]
pub struct ClaimGenerator {
    pub inputs: Vec<PackedCasmState>,
}
impl ClaimGenerator {
    pub fn new(mut inputs: Vec<VmState>) -> Self {
        assert!(!inputs.is_empty());

        // TODO(spapini): Split to multiple components.
        let size = inputs.len().next_power_of_two();
        inputs.resize(size, inputs[0]);

        let inputs = inputs
            .into_iter()
            .array_chunks::<N_LANES>()
            .map(|chunk| PackedCasmState {
                pc: PackedM31::from_array(std::array::from_fn(|i| {
                    M31::from_u32_unchecked(chunk[i].pc)
                })),
                ap: PackedM31::from_array(std::array::from_fn(|i| {
                    M31::from_u32_unchecked(chunk[i].ap)
                })),
                fp: PackedM31::from_array(std::array::from_fn(|i| {
                    M31::from_u32_unchecked(chunk[i].fp)
                })),
            })
            .collect_vec();
        Self { inputs }
    }

    pub fn write_trace(
        mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let (trace, lookup_data) = write_trace_simd(&self.inputs, memory_trace_generator);

        let n_rows = self.inputs.len() * N_LANES;
        println!("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa n_rows: {}", n_rows);
        assert_ne!(n_rows, 0);
        let size = std::cmp::max(n_rows.next_power_of_two(), N_LANES);
        let need_padding = n_rows != size;

        if need_padding {
            self.inputs
                .resize(size, self.inputs.first().unwrap().clone());
            bit_reverse_coset_to_circle_domain_order(&mut self.inputs);
        }
        lookup_data.memory.iter().for_each(|c| {
            c.iter()
                .for_each(|v| memory_trace_generator.add_inputs_simd(&v[0]))
        });
        tree_builder.extend_evals(trace.to_evals());
        (
            Claim { n_rows },
            InteractionClaimGenerator {
                n_rows,
                lookup_data,
            },
        )
    }
}
#[derive(Debug, Uninitialized, IterMut, ParIterMut)]
pub struct LookupData {
    pub memory: [Vec<[PackedM31; N_MEMORY_ELEMS]>; N_MEMORY_LOOKUPS],
    pub state: [Vec<[PackedM31; STATE_SIZE]>; N_STATE_LOOKUPS],
}

#[derive(Debug)]
pub struct InteractionClaimGenerator {
    pub n_rows: usize,
    pub lookup_data: LookupData,
}

impl InteractionClaimGenerator {
    pub fn write_interaction_trace(
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_relation: &MemoryRelation,
        state_relation: &StateRelation,
    ) -> InteractionClaim {
        let log_size = std::cmp::max(self.n_rows.next_power_of_two().ilog2(), LOG_N_LANES);
        let mut logup_gen = LogupTraceGenerator::new(log_size);
        let mut col0 = logup_gen.new_col();
        let state_use = &self.lookup_data.state[0];
        let read_pc = &self.lookup_data.memory[0];
        for (i, (x, y)) in zip_eq(state_use, read_pc).enumerate() {
            let denom_x: PackedQM31 = state_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);
            col0.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col0.finalize_col();

        let mut col1 = logup_gen.new_col();
        let read_val = &self.lookup_data.memory[1];
        let state_yield = &self.lookup_data.state[1];
        for (i, (x,y)) in zip_eq(read_val, state_yield).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = state_relation.combine(y);
            col1.write_frac(i, denom_y - denom_x, denom_x * denom_y);
        }
        col1.finalize_col();

        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim {
            log_size,
            claimed_sum,
        }
    }
}

// jnz trace row:
// pc | ap | fp | op | reg | off | imm | addr | addr_val (4) | inverse (4) | flag | new_pc
fn write_trace_simd(
    inputs: &[PackedCasmState],
    memory_trace_generator: &memory::ClaimGenerator,
) -> (ComponentTrace<N_TRACE_COLUMNS>, LookupData) {
    let log_n_packed_rows = inputs.len().ilog2();
    let log_size = log_n_packed_rows + LOG_N_LANES;
    let (mut trace, mut lookup_data) = unsafe {
        (
            ComponentTrace::<N_TRACE_COLUMNS>::uninitialized(log_size),
            LookupData::uninitialized(log_n_packed_rows),
        )
    };

    trace
        .par_iter_mut()
        .zip(inputs.par_iter())
        .zip(lookup_data.par_iter_mut())
        .for_each(|((row, opcode_input), lookup_data)| {
            // Initial state.
            let pc = opcode_input.pc;
            let ap = opcode_input.ap;
            let fp = opcode_input.fp;
            *row[0] = pc;
            *row[1] = ap;
            *row[2] = fp;
            *lookup_data.state[0] = [pc, ap, fp];

            // Decode insturction.
            let [opcode, off, imm, _] =
                memory_trace_generator.deduce_output(pc).into_packed_m31s();

            *lookup_data.memory[0] = [pc, opcode, off, imm, PackedM31::zero()];

            let [op_type, reg] = decode_opcode(INSTRUCTION_BASE, opcode, [2, 2]);

            *row[3] = op_type;
            *row[4] = reg;
            *row[5] = off;
            *row[6] = imm;

            let addr = Selector::select(&reg, [&(ap), &(fp)]) + off;
            *row[7] = addr;

            let val_arr = memory_trace_generator
                .deduce_output(addr)
                .into_packed_m31s();
            *row[8] = val_arr[0];
            *row[9] = val_arr[1];
            *row[10] = val_arr[2];
            *row[11] = val_arr[3];

            *lookup_data.memory[1] = [addr, val_arr[0], val_arr[1], val_arr[2], val_arr[3]];
            let val = PackedQM31::from_packed_m31s(val_arr).to_array();
            let maybe_inverse = PackedQM31::from_array(std::array::from_fn(|i| {
                if val[i].is_zero() {
                    val[i]
                } else {
                    val[i].inverse()
                }
            }))
            .into_packed_m31s();

            *row[12] = maybe_inverse[0];
            *row[13] = maybe_inverse[1];
            *row[14] = maybe_inverse[2];
            *row[15] = maybe_inverse[3];

            let is_non_zero = PackedM31::from_array(std::array::from_fn(|i| {
                if val[i].is_zero() {
                    M31(0)
                } else {
                    M31(1)
                }
            }));

            *row[16] = is_non_zero;

            // Calc new pc.
            // TODO(Gilad): look for stwo-cairo SELECT function that does this.
            let pc_plus_one = pc + PackedM31::one();
            let pc_plus_imm = pc + imm;
            let branch_target = Selector::select(&op_type, [&pc_plus_imm, &imm]);
            let mask = is_non_zero.into_simd().simd_ne(Simd::splat(0));
            let new_pc = unsafe {
                PackedM31::from_simd_unchecked(
                    mask.select(pc_plus_one.into_simd(), branch_target.into_simd()),
                )
            };
            *row[17] = new_pc;

            *lookup_data.state[1] = [new_pc, ap, fp];
        });

    (trace, lookup_data)
}
