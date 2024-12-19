use std::path::PathBuf;

use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

// Converters.

pub(crate) fn m31_from_hex_str(x: &str) -> M31 {
    M31(u32::from_str_radix(x.trim_start_matches("0x"), 16).unwrap())
}

pub(crate) fn qm31_from_hex_str_array(x: [&str; 4]) -> QM31 {
    let m31_array = x.map(m31_from_hex_str);
    QM31::from_m31_array(m31_array)
}

pub(crate) fn u32_from_usize(value: usize) -> u32 {
    u32::try_from(value).unwrap()
}

pub(crate) fn usize_from_u32(value: u32) -> usize {
    usize::try_from(value).unwrap()
}

// General utils.

pub(crate) fn maybe_resize<T: Clone>(vector: &mut Vec<T>, index: usize, default_value: T) {
    let n_elements = vector.len();

    if index >= n_elements {
        let resize_by = std::cmp::max(index + 1, n_elements * 2);
        vector.resize(resize_by, default_value);
    }
}

pub(crate) fn get_crate_dir() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.to_path_buf()
}

pub(crate) fn get_tests_data_dir() -> PathBuf {
    get_crate_dir().join("tests").join("data")
}
