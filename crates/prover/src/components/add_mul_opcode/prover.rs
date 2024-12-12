use std::simd::Simd;

use itertools::{zip_eq, Itertools};
use num_traits::{One, Zero};
use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::constraint_framework::{Relation, SimdDomainEvaluator};
use stwo_prover::core::backend::simd::m31::{PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::qm31::PackedQM31;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::backend::{Col, Column};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::FieldExpOps;
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::poly::circle::{CanonicCoset, CircleEvaluation};
use stwo_prover::core::poly::BitReversedOrder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{Claim, InteractionClaim, INSTRUCTION_BASE};
use crate::components::add_mul_opcode::component::{IMM_BASE, N_TRACE_COLUMNS};
use crate::components::memory::addr_to_f31;
use crate::input::instructions::VmState;
use crate::relations::{MemoryRelation, StateRelation, N_MEMORY_ELEMS, STATE_SIZE};

const N_MEMORY_LOOKUPS: usize = 4;
const N_STATE_LOOKUPS: usize = 2;

// TODO(Ohad): take from prover_types and remove.
pub struct PackedVmState {
    pub pc: PackedM31,
    pub ap: PackedM31,
    pub fp: PackedM31,
}

pub struct ClaimGenerator {
    pub inputs: Vec<PackedVmState>,
}
impl ClaimGenerator {
    pub fn new(mut inputs: Vec<VmState>) -> Self {
        assert!(!inputs.is_empty());

        // TODO(spapini): Split to multiple components.
        let size = inputs.len().next_power_of_two();
        inputs.resize(size, inputs[0].clone());

        let inputs = inputs
            .into_iter()
            .array_chunks::<N_LANES>()
            .map(|chunk| PackedVmState {
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
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_trace_generator: &mut addr_to_f31::ClaimGenerator,
    ) -> (Claim, InteractionClaimGenerator) {
        let (trace, interaction_claim_generator) =
            write_trace_simd(&self.inputs, memory_trace_generator);
        interaction_claim_generator.memory.iter().for_each(|c| {
            c.iter()
                .for_each(|v| memory_trace_generator.add_inputs_simd(&v[0]))
        });
        tree_builder.extend_evals(trace);
        let claim = Claim {
            n_calls: self.inputs.len() * N_LANES,
        };
        (claim, interaction_claim_generator)
    }
}

pub struct InteractionClaimGenerator {
    pub memory: [Vec<[PackedM31; N_MEMORY_ELEMS]>; N_MEMORY_LOOKUPS],
    pub state: [Vec<[PackedM31; STATE_SIZE]>; N_STATE_LOOKUPS],
}
impl InteractionClaimGenerator {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            memory: [
                Vec::with_capacity(capacity),
                Vec::with_capacity(capacity),
                Vec::with_capacity(capacity),
                Vec::with_capacity(capacity),
            ],
            state: [Vec::with_capacity(capacity), Vec::with_capacity(capacity)],
        }
    }

    pub fn write_interaction_trace(
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        memory_relation: &MemoryRelation,
        state_relation: &StateRelation,
    ) -> InteractionClaim {
        let log_size = self.memory[0].len().ilog2() + LOG_N_LANES;
        let mut logup_gen = LogupTraceGenerator::new(log_size);

        let mut col0 = logup_gen.new_col();
        let state_use = &self.state[0];
        let read_pc = &self.memory[0];
        for (i, (x, y)) in zip_eq(state_use, read_pc).enumerate() {
            let denom_x: PackedQM31 = state_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

            col0.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col0.finalize_col();

        let mut col1 = logup_gen.new_col();
        let read_dst = &self.memory[1];
        let read_lhs = &self.memory[2];
        for (i, (x, y)) in zip_eq(read_dst, read_lhs).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

            col1.write_frac(i, denom_x + denom_y, denom_x * denom_y)
        }
        col1.finalize_col();

        let mut col2 = logup_gen.new_col();
        let read_rhs = &self.memory[3];
        let state_yield = &self.state[1];
        for (i, (x, y)) in zip_eq(read_rhs, state_yield).enumerate() {
            let denom_x: PackedQM31 = memory_relation.combine(x);
            let denom_y: PackedQM31 = memory_relation.combine(y);

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
    inputs: &[PackedVmState],
    memory_trace_generator: &addr_to_f31::ClaimGenerator,
) -> (
    Vec<CircleEvaluation<SimdBackend, M31, BitReversedOrder>>,
    InteractionClaimGenerator,
) {
    let n_trace_columns = N_TRACE_COLUMNS;
    let mut trace_values = (0..n_trace_columns)
        .map(|_| Col::<SimdBackend, M31>::zeros(inputs.len() * N_LANES))
        .collect_vec();
    let mut sub_components_inputs = InteractionClaimGenerator::with_capacity(inputs.len());
    inputs.iter().enumerate().for_each(|(i, input)| {
        write_trace_row(
            &mut trace_values,
            input,
            i,
            &mut sub_components_inputs,
            memory_trace_generator,
        );
    });

    let trace = trace_values
        .into_iter()
        .map(|eval| {
            // TODO(Ohad): Support non-power of 2 inputs.
            let domain = CanonicCoset::new(
                eval.len()
                    .checked_ilog2()
                    .expect("Input is not a power of 2!"),
            )
            .circle_domain();
            CircleEvaluation::<SimdBackend, M31, BitReversedOrder>::new(domain, eval)
        })
        .collect_vec();

    (trace, sub_components_inputs)
}

// TODO(alont) put these in a common place.
pub fn select_trit(trit: PackedM31, a: PackedM31, b: PackedM31, c: PackedM31) -> PackedM31 {
    let trit_minus_one = trit - PackedM31::one();
    let trit_minus_two = trit - PackedM31::broadcast(M31(2));
    let two_inv = PackedM31::broadcast(M31(2).inverse());

    (two_inv * trit_minus_one * trit_minus_two * a) + (two_inv * trit * trit_minus_one * b)
        - (trit * trit_minus_two * c)
}

pub fn divmod(x: PackedM31, divisor: u32) -> (PackedM31, PackedM31) {
    unsafe {
        let simd_x = x.into_simd();
        (
            PackedM31::from_simd_unchecked(simd_x / Simd::splat(divisor)),
            PackedM31::from_simd_unchecked(simd_x % Simd::splat(divisor)),
        )
    }
}

// Add / Mul trace row:
// | State (3) | flags (5) | offsets (3) | addrs (3) | values (3 * 4) |
// TODO(Ohad): redo when air team decides how it should look.
fn write_trace_row(
    trace: &mut [Col<SimdBackend, M31>],
    input: &PackedVmState,
    row_index: usize,
    interaction_claim_generator: &mut InteractionClaimGenerator,
    memory_trace_generator: &addr_to_f31::ClaimGenerator,
) {
    // Initial state
    trace[0].data[row_index] = input.pc;
    trace[1].data[row_index] = input.ap;
    trace[2].data[row_index] = input.fp;
    interaction_claim_generator.state[0].push([input.pc, input.ap, input.fp]);

    // Flags
    // TODO(alont) change to actual values once memory values are QM31.
    let [opcode, off0, off1, off2] = [
        memory_trace_generator.deduce_output(input.pc),
        PackedM31::zero(),
        PackedM31::zero(),
        PackedM31::zero(),
    ];
    interaction_claim_generator.memory[0].push([input.pc, opcode, off0, off1, off2]);

    let flags = opcode - PackedM31::broadcast(INSTRUCTION_BASE);

    let (flags, is_mul) = divmod(flags, 2);
    let (flags, reg0) = divmod(flags, 2);
    let (flags, reg1) = divmod(flags, 3);
    let (flags, reg2) = divmod(flags, 2);
    let (flags, appp) = divmod(flags, 2);
    assert!(flags.is_zero(), "Too many flags.");

    trace[3].data[row_index] = is_mul;
    trace[4].data[row_index] = reg0;
    trace[5].data[row_index] = reg1;
    trace[6].data[row_index] = reg2;
    trace[7].data[row_index] = appp;

    // Offsets
    trace[8].data[row_index] = off0;
    trace[9].data[row_index] = off1;
    trace[10].data[row_index] = off2;

    // Addresses
    let dst_addr = (reg0 * input.fp) + (PackedM31::one() - reg0) * input.ap + off0;
    let lhs_addr = select_trit(reg1, input.ap, input.fp, PackedM31::broadcast(IMM_BASE)) + off1;
    let rhs_addr = (reg2 * input.fp) + (PackedM31::one() - reg2) * input.ap + off2;

    trace[11].data[row_index] = dst_addr;
    trace[12].data[row_index] = lhs_addr;
    trace[13].data[row_index] = rhs_addr;

    // Values
    let [dst0, dst1, dst2, dst3] = [
        memory_trace_generator.deduce_output(dst_addr),
        PackedM31::zero(),
        PackedM31::zero(),
        PackedM31::zero(),
    ];
    let [lhs0, lhs1, lhs2, lhs3] = [
        memory_trace_generator.deduce_output(lhs_addr),
        PackedM31::zero(),
        PackedM31::zero(),
        PackedM31::zero(),
    ];
    let [rhs0, rhs1, rhs2, rhs3] = [
        memory_trace_generator.deduce_output(rhs_addr),
        PackedM31::zero(),
        PackedM31::zero(),
        PackedM31::zero(),
    ];
    interaction_claim_generator.memory[1].push([dst_addr, dst0, dst1, dst2, dst3]);
    interaction_claim_generator.memory[2].push([lhs_addr, lhs0, lhs1, lhs2, lhs3]);
    interaction_claim_generator.memory[3].push([rhs_addr, rhs0, rhs1, rhs2, rhs3]);

    interaction_claim_generator.state[1].push([
        input.pc + PackedM31::one(),
        input.ap + appp,
        input.fp,
    ]);
}
