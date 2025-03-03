use itertools::{zip_eq, Itertools};
use num_traits::One;
use rayon::iter::{IndexedParallelIterator, IntoParallelRefIterator, ParallelIterator};
use stwo_air_utils::trace::component_trace::ComponentTrace;
use stwo_air_utils_derive::{IterMut, ParIterMut, Uninitialized};
use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::constraint_framework::Relation;
use stwo_prover::core::backend::simd::conversion::Pack;
use stwo_prover::core::backend::simd::m31::{PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::qm31::PackedQM31;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim};
use crate::components::memory;
use crate::components::range_check::range_check_opcode::component::INSTRUCTION_BASE;
use crate::relations::{
    MemoryRelation, RangeCheck_15, StateRelation, N_MEMORY_ELEMS, RANGE_CHECK_SIZE, STATE_SIZE,
};
use crate::utils::prover::{decode_opcode, divmod};
use crate::utils::types::{CasmState, PackedCasmState};
use crate::utils::{Selector, SelectorTrait};

const N_TRACE_COLUMNS: usize = 12;
const N_MEMORY_LOOKUPS: usize = 2;
const N_RANGE_CHECK_14_LOOKUPS: usize = 4;
const N_STATE_LOOKUPS: usize = 2;

#[derive(Debug)]
pub struct ClaimGenerator {
    pub inputs: Vec<CasmState>,
}
impl ClaimGenerator {
    pub fn new(inputs: Vec<CasmState>) -> Option<Self> {
        if inputs.is_empty() {
            return None;
        };
        Some(Self { inputs })
    }

    pub fn write_trace(
        mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let n_rows = self.inputs.len();
        let size = std::cmp::max(n_rows.next_power_of_two(), N_LANES);
        let log_size = size.ilog2();

        // Prepare inputs.
        self.inputs.resize(size, self.inputs[0]);
        let inputs = self
            .inputs
            .into_iter()
            .array_chunks::<N_LANES>()
            .map(Pack::pack)
            .collect_vec();
        let (trace, lookup_data) = write_trace_simd(&inputs, memory_trace_generator);

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
    pub range_check_14: [Vec<[PackedM31; RANGE_CHECK_SIZE]>; N_RANGE_CHECK_14_LOOKUPS],
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
        range_check_14_relation: &RangeCheck_15,
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
        let range_check_14_min_low = &self.lookup_data.range_check_14[0];

        for (i, (x, y)) in zip_eq(read_val, range_check_14_min_low).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = range_check_14_relation.combine(y);

            col1.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col1.finalize_col();

        let mut col2 = logup_gen.new_col();
        let range_check_14_min_high = &self.lookup_data.range_check_14[1];
        let range_check_14_max_low = &self.lookup_data.range_check_14[2];

        for (i, (x, y)) in zip_eq(range_check_14_min_high, range_check_14_max_low).enumerate() {
            let denom_x: PackedQM31 = range_check_14_relation.combine(x);
            let denom_y: PackedQM31 = range_check_14_relation.combine(y);

            col2.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col2.finalize_col();

        let mut col3 = logup_gen.new_col();
        let range_check_14_max_high = &self.lookup_data.range_check_14[2];
        let state_yield = &self.lookup_data.state[1];
        for (i, (x, y)) in zip_eq(range_check_14_max_high, state_yield).enumerate() {
            let denom_x: PackedQM31 = range_check_14_relation.combine(x);
            let denom_y: PackedQM31 = state_relation.combine(y);

            col3.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col3.finalize_col();

        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim {
            log_size,
            claimed_sum,
        }
    }
}

// add_ap_ trace row:
// pc | ap | fp | reg | val | min | max | offset | min_low | min_high | max_low | max_high
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
            let [opcode, min, max, off] =
                memory_trace_generator.deduce_output(pc).into_packed_m31s();
            *lookup_data.memory[0] = [pc, opcode, min, max, off];

            let [reg] = decode_opcode(INSTRUCTION_BASE, opcode, [2]);
            *row[3] = reg;

            let addr = Selector::select(&reg, [&ap, &fp]) + off;
            let [val, zeros @ ..] = memory_trace_generator
                .deduce_output(addr)
                .into_packed_m31s();
            *row[4] = val;

            *lookup_data.memory[1] = [addr, val, zeros[0], zeros[1], zeros[2]];

            *row[5] = min;
            *row[6] = max;
            *row[7] = off;

            let (min_low, min_high) = divmod(min, 1 << 14);
            *row[8] = min_low;
            *row[9] = min_high;
            *lookup_data.range_check_14[0] = [min_low];
            *lookup_data.range_check_14[1] = [min_high];

            let (max_low, max_high) = divmod(max, 1 << 14);
            *row[10] = max_low;
            *row[11] = max_high;
            *lookup_data.range_check_14[2] = [max_low];
            *lookup_data.range_check_14[3] = [max_high];

            *lookup_data.state[1] = [pc + PackedM31::one(), ap, fp];
        });

    (trace, lookup_data)
}
