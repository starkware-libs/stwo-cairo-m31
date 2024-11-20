use itertools::Itertools;
use stwo_prover::constraint_framework::logup::LogupTraceGenerator;
use stwo_prover::core::backend::simd::column::BaseColumn;
use stwo_prover::core::backend::simd::m31::{PackedBaseField, PackedM31, LOG_N_LANES, N_LANES};
use stwo_prover::core::backend::simd::qm31::PackedQM31;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::backend::{Col, Column};
use stwo_prover::core::fields::m31::{BaseField, M31};
use stwo_prover::core::pcs::TreeBuilder;
use stwo_prover::core::poly::circle::{CanonicCoset, CircleEvaluation};
use stwo_prover::core::poly::BitReversedOrder;
use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

use super::component::{
    Claim, InteractionClaim, MULTIPLICITY_COLUMN_OFFSET, N_ADDR_AND_VALUE_COLUMNS, N_COLUMNS,
};
use super::RelationElements;
use crate::components::memory::MEMORY_ADDRESS_BOUND;
use crate::input::mem::{Memory, MemoryValue};

pub struct ClaimGenerator {
    pub values: Vec<PackedM31>,
    pub multiplicities: Vec<u32>,
}
impl ClaimGenerator {
    pub fn new(mem: &Memory) -> Self {
        // TODO(spapini): Split to multiple components.
        // TODO(spapini): More repetitions, for efficiency.
        let mut values = (0..mem.address_to_value_index.len())
            .map(|addr| mem.get(addr as u32))
            .collect_vec();

        let size = values.len().next_power_of_two();
        assert!(size <= MEMORY_ADDRESS_BOUND);
        values.resize(size, MemoryValue(Default::default()));

        let values = values
            .into_iter()
            .array_chunks::<N_LANES>()
            .flat_map(|chunk| -> [PackedM31; 4] {
                std::array::from_fn(|i| {
                    PackedM31::from_array(std::array::from_fn(|j| chunk[j].0.to_m31_array()[i]))
                })
            })
            .collect_vec();
        let multiplicities = vec![0; size];
        Self {
            values,
            multiplicities,
        }
    }

    pub fn add_inputs(&mut self, memory_index: usize) {
        self.multiplicities[memory_index] += 1;
    }

    pub fn write_trace(
        &mut self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
    ) -> (Claim, InteractionClaimGenerator) {
        let size = self.values.len() * N_LANES;
        let mut trace = (0..N_COLUMNS)
            .map(|_| Col::<SimdBackend, BaseField>::zeros(size))
            .collect_vec();

        let inc = PackedBaseField::from_array(std::array::from_fn(|i| {
            M31::from_u32_unchecked((i) as u32)
        }));
        for (i, value) in self.values.iter().enumerate() {
            // TODO(AlonH): Either create a constant column for the addresses and remove it from
            // here or add constraints to the column here.
            trace[0].data[i] =
                PackedM31::broadcast(M31::from_u32_unchecked((i * N_LANES) as u32)) + inc;
            trace[1].data[i] = *value;
        }
        trace[MULTIPLICITY_COLUMN_OFFSET] = BaseColumn::from_iter(
            self.multiplicities
                .clone()
                .into_iter()
                .map(BaseField::from_u32_unchecked),
        );
        // Lookup data.
        let ids_and_values: [Vec<PackedM31>; N_ADDR_AND_VALUE_COLUMNS] = trace
            [0..N_ADDR_AND_VALUE_COLUMNS]
            .iter()
            .map(|col| col.data.clone())
            .collect_vec()
            .try_into()
            .unwrap();
        let multiplicities = trace[MULTIPLICITY_COLUMN_OFFSET].data.clone();

        // Extend trace.
        let log_address_bound = size.checked_ilog2().unwrap();
        let domain = CanonicCoset::new(log_address_bound).circle_domain();
        let trace = trace
            .into_iter()
            .map(|eval| CircleEvaluation::<SimdBackend, _, BitReversedOrder>::new(domain, eval))
            .collect_vec();
        tree_builder.extend_evals(trace);

        (
            Claim {
                log_size: log_address_bound,
            },
            InteractionClaimGenerator {
                ids_and_values,
                multiplicities,
            },
        )
    }
}

#[derive(Debug)]
pub struct InteractionClaimGenerator {
    pub ids_and_values: [Vec<PackedM31>; N_ADDR_AND_VALUE_COLUMNS],
    pub multiplicities: Vec<PackedM31>,
}
impl InteractionClaimGenerator {
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            ids_and_values: std::array::from_fn(|_| Vec::with_capacity(capacity)),
            multiplicities: Vec::with_capacity(capacity),
        }
    }

    pub fn write_interaction_trace(
        &self,
        tree_builder: &mut TreeBuilder<'_, '_, SimdBackend, Blake2sMerkleChannel>,
        lookup_elements: &RelationElements,
    ) -> InteractionClaim {
        let log_size = self.ids_and_values[0].len().ilog2() + LOG_N_LANES;
        let mut logup_gen = LogupTraceGenerator::new(log_size);
        let mut col_gen = logup_gen.new_col();

        // Lookup values columns.
        for vec_row in 0..1 << (log_size - LOG_N_LANES) {
            let values: [PackedM31; N_ADDR_AND_VALUE_COLUMNS] =
                std::array::from_fn(|i| self.ids_and_values[i][vec_row]);
            let denom: PackedQM31 = lookup_elements.combine(&values);
            col_gen.write_frac(vec_row, (-self.multiplicities[vec_row]).into(), denom);
        }
        col_gen.finalize_col();

        let (trace, claimed_sum) = logup_gen.finalize_last();
        tree_builder.extend_evals(trace);

        InteractionClaim { claimed_sum }
    }
}

// del rangecheck, split, redudnet stuff, small
// u32 -> m31
