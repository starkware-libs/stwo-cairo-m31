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
use crate::utils::prover::decode_opcode;
use crate::utils::types::{CasmState, PackedCasmState};
use crate::utils::{Selector, SelectorTrait};

const N_MEMORY_LOOKUPS: usize = 4;
const N_STATE_LOOKUPS: usize = 2;

pub struct ClaimGenerator {
    pub inputs: Vec<PackedCasmState>,
}
impl ClaimGenerator {
    pub fn new(mut inputs: Vec<CasmState>) -> Self {
        assert!(!inputs.is_empty());

        // TODO(spapini): Split to multiple components.
        let size = std::cmp::max(inputs.len().next_power_of_two(), N_LANES);
        inputs.resize(size, inputs[0].clone());

        let inputs = inputs
            .into_iter()
            .array_chunks::<N_LANES>()
            .map(|chunk| PackedCasmState {
                pc: PackedM31::from_array(std::array::from_fn(|i| chunk[i].pc)),
                ap: PackedM31::from_array(std::array::from_fn(|i| chunk[i].ap)),
                fp: PackedM31::from_array(std::array::from_fn(|i| chunk[i].fp)),
            })
            .collect_vec();
        Self { inputs }
    }
    pub fn write_trace(
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let (trace, lookup_data) = write_trace_simd(&self.inputs, memory_trace_generator);
        lookup_data.memory.iter().for_each(|c| {
            c.iter()
                .for_each(|v| memory_trace_generator.add_inputs_simd(&v[0]))
        });
        tree_builder.extend_evals(trace.to_evals());
        let n_rows = self.inputs.len() * N_LANES;
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
        let read_dst = &self.lookup_data.memory[1];
        let read_lhs = &self.lookup_data.memory[2];
        for (i, (x, y)) in zip_eq(read_dst, read_lhs).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

            col1.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col1.finalize_col();

        let mut col2 = logup_gen.new_col();
        let read_rhs = &self.lookup_data.memory[3];
        let state_yield = &self.lookup_data.state[1];
        for (i, (x, y)) in zip_eq(read_rhs, state_yield).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = state_relation.combine(y);

            col2.write_frac(i, denom_y - denom_x, denom_x * denom_y)
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
        .for_each(|((row, input), lookup_data)| {
            // Initial state
            *row[0] = input.pc;
            *row[1] = input.ap;
            *row[2] = input.fp;
            *lookup_data.state[0] = [input.pc, input.ap, input.fp];

            // Flags
            let [opcode, off0, off1, off2] = memory_trace_generator
                .deduce_output(input.pc)
                .into_packed_m31s();
            *lookup_data.memory[0] = [input.pc, opcode, off0, off1, off2];

            let [op_type, reg0, reg1, reg2, appp] =
                decode_opcode(INSTRUCTION_BASE, opcode, [2, 2, 2, 2, 2]);

            *row[3] = op_type;
            *row[4] = reg0;
            *row[5] = reg1;
            *row[6] = reg2;
            *row[7] = appp;

            // Offsets
            *row[8] = off0;
            *row[9] = off1;
            *row[10] = off2;

            // Addresses
            let dst_addr = Selector::select(&reg0, [&input.ap, &input.fp]) + off0;
            let lhs_addr = Selector::select(&reg1, [&input.ap, &input.fp]) + off1;
            let rhs_addr = Selector::select(&reg2, [&input.ap, &input.fp]) + off2;

            *row[11] = dst_addr;
            *row[12] = lhs_addr;
            *row[13] = rhs_addr;

            // Values
            let [dst0, dst1, dst2, dst3] = memory_trace_generator
                .deduce_output(dst_addr)
                .into_packed_m31s();
            let [lhs0, lhs1, lhs2, lhs3] = memory_trace_generator
                .deduce_output(lhs_addr)
                .into_packed_m31s();
            let [rhs0, rhs1, rhs2, rhs3] = memory_trace_generator
                .deduce_output(rhs_addr)
                .into_packed_m31s();

            *row[14] = dst0;
            *row[15] = dst1;
            *row[16] = dst2;
            *row[17] = dst3;

            *row[18] = lhs0;
            *row[19] = lhs1;
            *row[20] = lhs2;
            *row[21] = lhs3;

            *row[22] = rhs0;
            *row[23] = rhs1;
            *row[24] = rhs2;
            *row[25] = rhs3;

            *lookup_data.memory[1] = [dst_addr, dst0, dst1, dst2, dst3];
            *lookup_data.memory[2] = [lhs_addr, lhs0, lhs1, lhs2, lhs3];
            *lookup_data.memory[3] = [rhs_addr, rhs0, rhs1, rhs2, rhs3];

            *lookup_data.state[1] = [input.pc + PackedM31::one(), input.ap + appp, input.fp];
        });

    (trace, lookup_data)
}
