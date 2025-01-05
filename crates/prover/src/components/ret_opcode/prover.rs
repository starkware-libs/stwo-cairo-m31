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
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::utils::bit_reverse_coset_to_circle_domain_order;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim, RET_INSTRUCTION};
use crate::components::memory;
use crate::input::instructions::VmState;
use crate::relations::MemoryRelation;

const N_MEMORY_CALLS: usize = 3;
const N_TRACE_COLUMNS: usize = 5;

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
        mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut memory::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let (trace, lookup_data) = write_trace_simd(&self.inputs, memory_trace_generator);

        let n_rows = self.inputs.len();
        assert_ne!(n_rows, 0);
        let size = std::cmp::max(n_rows.next_power_of_two(), N_LANES);
        let need_padding = n_rows != size;

        if need_padding {
            self.inputs
                .resize(size, self.inputs.first().unwrap().clone());
            bit_reverse_coset_to_circle_domain_order(&mut self.inputs);
        }
        lookup_data.memory_inputs.iter().for_each(|c| {
            c.iter()
                .for_each(|v| memory_trace_generator.add_inputs_simd(v))
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
    // TODO(gilad): replace with `SubComponentInputs` like here:
    // https://github.com/starkware-libs/stwo-cairo-m31/commit/91898288f77e4bf6de206992801a6aa4cf004953
    pub memory_inputs: Vec<[PackedM31; N_MEMORY_CALLS]>,
    pub memory_outputs: Vec<[PackedQM31; N_MEMORY_CALLS]>,
    // Callee data.
    // pc: Vec<PackedM31>,
    // fp: Vec<PackedM31>,
    // instr: Vec<PackedM31>,
    // new_pc: Vec<PackedM31>,
    // new_fp: Vec<PackedM31>,
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
        lookup_elements: &MemoryRelation,
    ) -> InteractionClaim {
        let log_size = std::cmp::max(self.n_rows.next_power_of_two().ilog2(), LOG_N_LANES);
        let mut logup_gen = LogupTraceGenerator::new(log_size);

        for col_index in 0..N_MEMORY_CALLS {
            let mut col_gen = logup_gen.new_col();
            for (i, (&addr, &output)) in zip_eq(
                &self.lookup_data.memory_inputs[col_index],
                &self.lookup_data.memory_outputs[col_index],
            )
            .enumerate()
            {
                let address_and_value = vec![addr, output.into_packed_m31s()[0]];
                let denom = lookup_elements.combine(&address_and_value);
                col_gen.write_frac(i, PackedQM31::one(), denom);
            }
            col_gen.finalize_col();
        }
        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim {
            log_size,
            claimed_sum,
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
            let input_tmp_e23a5_0 = ret_opcode_input;
            let col0_pc = input_tmp_e23a5_0.pc;
            *row[0] = col0_pc;
            // Not added to memory inputs: `ap` not part of constraint yet.
            let col1_ap = input_tmp_e23a5_0.ap;
            *row[1] = col1_ap;
            let col2_fp = input_tmp_e23a5_0.fp;
            *row[2] = col2_fp;
            let mem_fp_minus_one = memory_trace_generator
                .deduce_output((col2_fp) - (PackedM31::broadcast(M31::one())));

            let col3 = mem_fp_minus_one;
            *row[3] = col3.into_packed_m31s()[0];
            let mem_fp_minus_two = memory_trace_generator
                .deduce_output((col2_fp) - (PackedM31::broadcast(M31::from(2))));

            *lookup_data.memory_inputs = [
                col0_pc,
                col2_fp - PackedM31::broadcast(M31::one()),
                col2_fp - PackedM31::broadcast(M31::from(2)),
            ];
            *lookup_data.memory_outputs = [
                PackedM31::broadcast(RET_INSTRUCTION).into(),
                mem_fp_minus_one,
                mem_fp_minus_two,
            ];

            let col4 = mem_fp_minus_two;
            *row[4] = col4.into_packed_m31s()[0];
        });

    (trace, lookup_data)
}
