use itertools::{zip_eq, Itertools};
use num_traits::One;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use stwo_air_utils::trace::component_trace::ComponentTrace;
use stwo_air_utils_derive::{IterMut, ParIterMut, Uninitialized};
use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::constraint_framework::Relation;
use stwo_prover::core::backend::simd::m31::{PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::qm31::PackedQM31;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim, INSTRUCTION_BASE};
use crate::components::add_mul_opcode::component::N_TRACE_COLUMNS;
use crate::components::memory;
use crate::relations::{MemoryRelation, StateRelation, N_MEMORY_ELEMS, STATE_SIZE};
use crate::utils::prover::{decode_opcode, Enabler};
use crate::utils::types::{CasmState, PackedCasmState};
use crate::utils::{Selector, SelectorTrait};

const N_MEMORY_LOOKUPS: usize = 3;
const N_STATE_LOOKUPS: usize = 2;

pub struct ClaimGenerator {
    pub inputs: Vec<CasmState>,
}
impl ClaimGenerator {
    pub fn new(inputs: Vec<CasmState>) -> Self {
        assert!(!inputs.is_empty());
        Self { inputs }
    }
    pub fn write_trace(
        mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let n_rows = self.inputs.len();
        let size = std::cmp::max(n_rows.next_power_of_two(), N_LANES);
        let log_size = size.ilog2();
        let log_n_packed_rows = log_size - LOG_N_LANES;

        // Prepare inputs.
        self.inputs.resize(size, self.inputs[0]);
        let inputs = self
            .inputs
            .into_iter()
            .array_chunks::<N_LANES>()
            .map(|chunk| PackedCasmState {
                pc: PackedM31::from_array(std::array::from_fn(|i| chunk[i].pc)),
                ap: PackedM31::from_array(std::array::from_fn(|i| chunk[i].ap)),
                fp: PackedM31::from_array(std::array::from_fn(|i| chunk[i].fp)),
            })
            .collect_vec();
        let enabler = Enabler::new(n_rows);

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
            .enumerate()
            .for_each(|(i, ((row, input), lookup_data))| {
                *row[0] = enabler.packed_at(i);
                // Initial state
                *row[1] = input.pc;
                *row[2] = input.ap;
                *row[3] = input.fp;
                *lookup_data.state[0] = [input.pc, input.ap, input.fp];

                // Flags
                let [opcode, off0, off1, imm] = memory_trace_generator
                    .deduce_output(input.pc)
                    .into_packed_m31s();
                *lookup_data.memory[0] = [input.pc, opcode, off0, off1, imm];

                let [op_type, lhs_flag, rhs_flag, complex_flag, appp] =
                    decode_opcode(INSTRUCTION_BASE, opcode, [2, 2, 2, 3, 2]);

                *row[4] = op_type;
                *row[5] = lhs_flag;
                *row[6] = rhs_flag;
                *row[7] = complex_flag;
                *row[8] = appp;

                // Offsets
                *row[9] = off0;
                *row[10] = off1;
                *row[11] = imm;

                // Addresses
                let lhs_addr = Selector::select(&lhs_flag, [&input.ap, &input.fp]) + off0;
                let rhs_addr = Selector::select(&rhs_flag, [&input.ap, &input.fp]) + off1;

                *row[12] = lhs_addr;
                *row[13] = rhs_addr;

                // Values
                let [lhs0, lhs1, lhs2, lhs3] = memory_trace_generator
                    .deduce_output(lhs_addr)
                    .into_packed_m31s();
                let [rhs0, rhs1, rhs2, rhs3] = memory_trace_generator
                    .deduce_output(rhs_addr)
                    .into_packed_m31s();

                *row[14] = lhs0;
                *row[15] = lhs1;
                *row[16] = lhs2;
                *row[17] = lhs3;

                *row[18] = rhs0;
                *row[19] = rhs1;
                *row[20] = rhs2;
                *row[21] = rhs3;

                *lookup_data.memory[1] = [lhs_addr, lhs0, lhs1, lhs2, lhs3];
                *lookup_data.memory[2] = [rhs_addr, rhs0, rhs1, rhs2, rhs3];

                *lookup_data.state[1] = [input.pc + PackedM31::one(), input.ap + appp, input.fp];
            });

        lookup_data.memory.iter().for_each(|c| {
            c.iter()
                .for_each(|v| memory_trace_generator.add_inputs_simd(&v[0]))
        });
        tree_builder.extend_evals(trace.to_evals());
        (
            Claim { log_size },
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
        let enabler = Enabler::new(self.n_rows);

        let mut col0 = logup_gen.new_col();
        let state_use = &self.lookup_data.state[0];
        let read_pc = &self.lookup_data.memory[0];
        for (i, (x, y)) in zip_eq(state_use, read_pc).enumerate() {
            let denom_x: PackedQM31 = state_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

            col0.write_frac(
                i,
                denom_x + denom_y * enabler.packed_at(i),
                denom_x * denom_y,
            )
        }
        col0.finalize_col();

        let mut col1 = logup_gen.new_col();
        let read_lhs = &self.lookup_data.memory[1];
        let read_rhs = &self.lookup_data.memory[2];
        for (i, (x, y)) in zip_eq(read_lhs, read_rhs).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

            col1.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col1.finalize_col();

        let mut col2 = logup_gen.new_col();
        let state_yield = &self.lookup_data.state[1];
        for (i, x) in state_yield.iter().enumerate() {
            let denom_x: PackedQM31 = state_relation.combine(x);

            col2.write_frac(i, -PackedQM31::one() * enabler.packed_at(i), denom_x)
        }
        col2.finalize_col();

        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim {
            log_size,
            claimed_sum,
        }
    }
}
