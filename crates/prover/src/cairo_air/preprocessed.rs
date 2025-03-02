use itertools::Itertools;
use stwo_prover::constraint_framework::preprocessed_columns::PreProcessedColumnId;
use stwo_prover::core::backend::simd::column::BaseColumn;
use stwo_prover::core::backend::simd::SimdBackend;
use stwo_prover::core::backend::BackendForChannel;
use stwo_prover::core::channel::MerkleChannel;
use stwo_prover::core::fields::m31::BaseField;
use stwo_prover::core::pcs::CommitmentTreeProver;
use stwo_prover::core::poly::circle::{CanonicCoset, CircleEvaluation, PolyOps};
use stwo_prover::core::poly::BitReversedOrder;
use stwo_prover::core::vcs::ops::MerkleHasher;

use crate::components::range_check::generate_partitioned_enumeration;

pub trait PreProcessedColumn {
    fn log_size(&self) -> u32;
    fn id(&self) -> PreProcessedColumnId;
    fn gen_column_simd(&self) -> CircleEvaluation<SimdBackend, BaseField, BitReversedOrder>;
}

pub struct PreProcessedTrace {
    range_check_columns: Vec<Box<dyn PreProcessedColumn>>,
}
impl PreProcessedTrace {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        let range_check_columns = gen_range_check_columns();
        Self {
            range_check_columns,
        }
    }

    fn columns(&self) -> Vec<&dyn PreProcessedColumn> {
        let mut columns: Vec<&dyn PreProcessedColumn> = vec![];
        columns.extend(self.range_check_columns.iter().map(|c| c.as_ref()));
        // Sort columns by descending log size.
        columns
            .into_iter()
            .sorted_by_key(|column| std::cmp::Reverse(column.log_size()))
            .collect_vec()
    }

    pub fn log_sizes(&self) -> Vec<u32> {
        self.columns().iter().map(|c| c.log_size()).collect()
    }

    pub fn gen_trace(&self) -> Vec<CircleEvaluation<SimdBackend, BaseField, BitReversedOrder>> {
        self.columns().iter().map(|c| c.gen_column_simd()).collect()
    }

    pub fn ids(&self) -> Vec<PreProcessedColumnId> {
        self.columns().iter().map(|c| c.id()).collect()
    }
}

fn gen_range_check_columns() -> Vec<Box<dyn PreProcessedColumn>> {
    // RangeCheck_16.
    let range_check_16 = RangeCheck::new([16], 0);

    vec![Box::new(range_check_16)]
}

pub struct RangeCheck<const N: usize> {
    ranges: [u32; N],
    column_idx: usize,
}
impl<const N: usize> RangeCheck<N> {
    pub fn new(ranges: [u32; N], column_idx: usize) -> Self {
        // TODO(Ohad): consider asserting height is lower than some bound.
        assert!(ranges.iter().all(|&r| r > 0));
        assert!(column_idx < N);
        Self { ranges, column_idx }
    }
}
impl<const N: usize> PreProcessedColumn for RangeCheck<N> {
    fn log_size(&self) -> u32 {
        self.ranges.iter().sum()
    }

    fn gen_column_simd(&self) -> CircleEvaluation<SimdBackend, BaseField, BitReversedOrder> {
        let partitions = generate_partitioned_enumeration(self.ranges);
        let column = partitions.into_iter().nth(self.column_idx).unwrap();
        CircleEvaluation::new(
            CanonicCoset::new(self.log_size()).circle_domain(),
            BaseColumn::from_simd(column),
        )
    }

    fn id(&self) -> PreProcessedColumnId {
        let ranges = self.ranges.iter().join("_");
        PreProcessedColumnId {
            id: format!("range_check_{}_column_{}", ranges, self.column_idx).to_string(),
        }
    }
}

/// Generates the root of the preprocessed trace commitment tree for a given `log_blowup_factor`.
// TODO(Shahars): remove allow.
#[allow(unused)]
pub fn generate_preprocessed_commitment_root<MC: MerkleChannel>(
    log_blowup_factor: u32,
) -> <<MC as MerkleChannel>::H as MerkleHasher>::Hash
where
    SimdBackend: BackendForChannel<MC>,
{
    let preprocessed_trace = PreProcessedTrace::new();

    // Precompute twiddles for the commitment scheme.
    let max_log_size = preprocessed_trace.log_sizes().into_iter().max().unwrap();
    let twiddles = SimdBackend::precompute_twiddles(
        CanonicCoset::new(max_log_size + log_blowup_factor)
            .circle_domain()
            .half_coset,
    );

    // Generate the commitment tree.
    let polys = SimdBackend::interpolate_columns(preprocessed_trace.gen_trace(), &twiddles);
    let commitment_scheme = CommitmentTreeProver::<SimdBackend, MC>::new(
        polys,
        log_blowup_factor,
        &mut MC::C::default(),
        &twiddles,
    );

    commitment_scheme.commitment.root()
}

#[cfg(test)]
mod tests {
    use stwo_prover::core::fields::m31::M31;
    use stwo_prover::core::vcs::blake2_hash::Blake2sHash;
    use stwo_prover::core::vcs::blake2_merkle::Blake2sMerkleChannel;

    use super::*;

    #[test]
    fn test_columns_are_in_decending_order() {
        let preprocessed_trace = PreProcessedTrace::new();

        let columns = preprocessed_trace.columns();

        assert!(columns
            .windows(2)
            .all(|w| w[0].log_size() >= w[1].log_size()));
    }

    #[test]
    fn test_range_check_gen_column_simd() {
        let ranges = [3, 1];
        let expected_0 = [0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7].map(M31);
        let expected_1 = [0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1, 0, 1].map(M31);

        let col_0 = RangeCheck::new(ranges, 0);
        let col_1 = RangeCheck::new(ranges, 1);
        let col_0_simd = col_0.gen_column_simd().to_cpu().to_vec();
        let col_1_simd = col_1.gen_column_simd().to_cpu().to_vec();

        assert_eq!(col_0_simd, expected_0);
        assert_eq!(col_1_simd, expected_1);
    }

    #[test]
    fn test_range_check_id() {
        let ranges = [1, 2, 3, 4];
        let range_check = RangeCheck::new(ranges, 2);

        let id = range_check.id();

        assert_eq!(id.id, "range_check_1_2_3_4_column_2");
    }

    #[test]
    fn test_preprocessed_root_regression() {
        let log_blowup_factor = 1;
        let expected = Blake2sHash::from(
            hex::decode("b35da289f836aedf18fd979c8884d0d6ae17371b15138b59606cb1947206ca96")
                .expect("Invalid hex string"),
        );

        let root = generate_preprocessed_commitment_root::<Blake2sMerkleChannel>(log_blowup_factor);

        assert_eq!(root, expected);
    }
}
