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
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::utils::bit_reverse_coset_to_circle_domain_order;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim, RET_INSTRUCTION};
use crate::components::memory;
use crate::input::instructions::VmState;
use crate::relations::{MemoryRelation, StateRelation, N_MEMORY_ELEMS, STATE_SIZE};

const N_TRACE_COLUMNS: usize = 5;

const N_MEMORY_LOOKUPS: usize = 3;
const N_STATE_LOOKUPS: usize = 2;

// TODO(Ohad): take from prover_types and remove.
#[derive(Debug, Clone)]
pub struct PackedCasmState {
    pub pc: PackedM31,
    pub ap: PackedM31,
    pub fp: PackedM31,
}

#[derive(Debug)]
pub struct ClaimGenerator {
    pub inputs: Vec<PackedCasmState>,
}
impl ClaimGenerator {
    pub fn new(mut inputs: Vec<VmState>) -> Self {
        assert!(!inputs.is_empty());

        // TODO(spapini): Split to multiple components.
        let n_rows = inputs.len();
        assert_ne!(n_rows, 0);
        let size = std::cmp::max(n_rows.next_power_of_two(), N_LANES);
        inputs.resize(size, inputs[0]);
        let need_padding = n_rows != size;

        if need_padding {
            inputs.resize(size, *inputs.first().unwrap());
            bit_reverse_coset_to_circle_domain_order(&mut inputs);
        }

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
        self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let (trace, lookup_data) = write_trace_simd(&self.inputs, memory_trace_generator);

        let n_rows = self.inputs.len();
        assert_ne!(n_rows, 0);

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

        let mut col_gen = logup_gen.new_col();
        let state_yield = &self.lookup_data.state[1];
        for (i, values) in state_yield.iter().enumerate() {
            let denom: PackedQM31 = state_relation.combine(values);
            col_gen.write_frac(i, -PackedQM31::one(), denom);
        }
        col_gen.finalize_col();

        let (trace, total_sum, claimed_sum) = if self.n_rows == 1 << log_size {
            let (trace, claimed_sum) = logup_gen.finalize_last();
            (trace, claimed_sum, None)
        } else {
            let (trace, [total_sum, claimed_sum]) =
                logup_gen.finalize_at([(1 << log_size) - 1, self.n_rows - 1]);
            (trace, total_sum, Some((claimed_sum, self.n_rows - 1)))
        };
        tree_builder.extend_evals(trace);

        InteractionClaim {
            log_size,
            logup_sums: (total_sum, claimed_sum),
        }
    }
}

// Ret trace row:
// | pc | ap | fp | [fp-1] | [fp-2] |
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
        .for_each(|((row, ret_opcode_input), lookup_data)| {
            let col0_pc = ret_opcode_input.pc;
            *row[0] = col0_pc;
            // Not added to memory inputs: `ap` not part of constraint yet.
            let col1_ap = ret_opcode_input.ap;
            *row[1] = col1_ap;
            let col2_fp = ret_opcode_input.fp;
            *row[2] = col2_fp;
            let mem_fp_minus_one = memory_trace_generator
                .deduce_output((col2_fp) - (PackedM31::broadcast(M31::one())));

            *lookup_data.state[0] = [col0_pc, col1_ap, col2_fp];

            let col3 = mem_fp_minus_one;
            *row[3] = col3.into_packed_m31s()[0];
            let mem_fp_minus_two = memory_trace_generator
                .deduce_output((col2_fp) - (PackedM31::broadcast(M31::from(2))));

            *lookup_data.memory[0] = std::array::from_fn(|i| match i {
                0 => col0_pc,
                1 => PackedM31::broadcast(RET_INSTRUCTION),
                _ => PackedM31::zero(),
            });

            let [v0, v1, v2, v3] = mem_fp_minus_one.into_packed_m31s();
            *lookup_data.memory[1] = [col2_fp - PackedM31::broadcast(M31::one()), v0, v1, v2, v3];

            let [v0, v1, v2, v3] = mem_fp_minus_two.into_packed_m31s();
            *lookup_data.memory[2] = [col2_fp - PackedM31::broadcast(M31::from(2)), v0, v1, v2, v3];

            let col4 = mem_fp_minus_two;
            *row[4] = col4.into_packed_m31s()[0];
        });

    (trace, lookup_data)
}
