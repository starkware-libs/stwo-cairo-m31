use std::collections::HashMap;
use std::ops::{Deref, DerefMut};

use itertools::Itertools;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use super::vm_import::MemEntry;

// Note: this should be smaller than 2^29.
const SMALL_VALUE_SHIFT: u32 = 1 << 26;

#[derive(Debug)]
pub struct MemConfig {
    /// The absolute value of the smallest negative value that can be stored as a small value.
    pub small_min_neg: u32,
    /// The largest value that can be stored as a small value.
    pub small_max: u32,
}
impl MemConfig {
    pub fn new(small_min_neg: u32, small_max: u32) -> MemConfig {
        assert!(small_min_neg <= SMALL_VALUE_SHIFT);
        assert!(small_max <= SMALL_VALUE_SHIFT);
        MemConfig {
            small_min_neg,
            small_max,
        }
    }
}
impl Default for MemConfig {
    fn default() -> Self {
        MemConfig {
            small_min_neg: (1 << 10) - 1,
            small_max: (1 << 10) - 1,
        }
    }
}

// TODO(spapini): Add U26 for addresses and U128 for range checks.
// TODO(spapini): Use some struct for Felt252 (that is still memory efficient).
#[derive(Debug)]
pub struct Memory {
    pub config: MemConfig,
    pub address_to_value_index: Vec<usize>,
    pub inst_cache: HashMap<u32, u64>,
    pub f31_values: Vec<QM31>,
}
impl Memory {
    pub fn get(&self, addr: u32) -> MemoryValue {
        let f31_values_index = self.address_to_value_index[addr as usize];
        MemoryValue(self.f31_values[f31_values_index])
    }

    pub fn get_raw_id(&self, addr: u32) -> usize {
        self.address_to_value_index[addr as usize]
    }

    pub fn get_inst(&self, addr: u32) -> Option<u64> {
        self.inst_cache.get(&addr).copied()
    }

    pub fn value_from_f31(&self, value: u32) -> MemoryValue {
        MemoryValue(value.into())
    }

    pub fn iter_values(&self) -> impl Iterator<Item = MemoryValue> + '_ {
        let mut values = (0..self.address_to_value_index.len())
            .map(|addr| self.get(addr as u32))
            .collect_vec();

        let size = values.len().next_power_of_two();
        values.resize(size, MemoryValue(0.into()));
        values.into_iter()
    }
}

pub struct MemoryBuilder {
    mem: Memory,
    felt31_id_cache: HashMap<[u32; 4], usize>,
}
impl MemoryBuilder {
    pub fn new(config: MemConfig) -> Self {
        Self {
            mem: Memory {
                config,
                address_to_value_index: Vec::new(),
                inst_cache: HashMap::new(),
                f31_values: Vec::new(),
            },
            felt31_id_cache: HashMap::new(),
        }
    }

    pub fn from_iter<I: IntoIterator<Item = MemEntry>>(
        config: MemConfig,
        iter: I,
    ) -> MemoryBuilder {
        let mem_entries = iter.into_iter();
        let mut builder = Self::new(config);
        for entry in mem_entries {
            // Convert entry.val from [u64; 8] to M31.
            // Assuming the relevant value is in entry.val[0].
            let index = *builder.felt31_id_cache.entry(entry.val).or_insert_with(|| {
                builder
                    .mem
                    .f31_values
                    .push(QM31::from_m31_array(std::array::from_fn(|i| {
                        M31::from(entry.val[i])
                    })));
                builder.mem.f31_values.len() - 1
            });
            builder.set(entry.addr, index); // Corrected positional arguments
        }

        builder
    }

    pub fn get_inst(&mut self, addr: u32) -> u64 {
        let mut inst_cache = std::mem::take(&mut self.inst_cache);
        let res = *inst_cache.entry(addr).or_insert_with(|| {
            let inst: M31 = self.mem.get(addr).0.try_into().unwrap();
            inst.0 as u64
        });
        self.inst_cache = inst_cache;
        res
    }

    pub fn set(&mut self, addr: u32, index: usize) {
        if addr as usize >= self.mem.address_to_value_index.len() {
            self.mem
                .address_to_value_index
                .resize(addr as usize + 1, Default::default());
        }
        self.mem.address_to_value_index[addr as usize] = index;
    }

    pub fn build(self) -> Memory {
        self.mem
    }
}
impl Deref for MemoryBuilder {
    type Target = Memory;
    fn deref(&self) -> &Self::Target {
        &self.mem
    }
}
impl DerefMut for MemoryBuilder {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.mem
    }
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct MemoryValue(pub QM31);

#[cfg(test)]
mod tests {

    use super::*;

    #[test]
    fn test_memory() {
        let entries = [
            MemEntry {
                addr: 0,
                val: [1; 4],
            },
            MemEntry {
                addr: 1,
                val: [6, 0, 0, 0],
            },
            MemEntry {
                addr: 2,
                val: [1, 2, 0, 0],
            },
            MemEntry {
                addr: 5,
                val: [1 << 30, 0, 0, 0],
            },
            MemEntry {
                addr: 8,
                val: [0x7FFF_FFFF, 0, 0, 0],
            },
            // Duplicates.
            MemEntry {
                addr: 100,
                val: [1; 4],
            },
            MemEntry {
                addr: 105,
                val: [1 << 30, 0, 0, 0],
            },
        ];

        let memory =
            MemoryBuilder::from_iter(MemConfig::default(), entries.iter().cloned()).build();

        // Test non-duplicate entries
        assert_eq!(QM31::from_m31_array([M31(1); 4]), memory.get(0).0);
        assert_eq!(M31::from(6), memory.get(1).0.try_into().unwrap());
        assert_eq!(
            QM31::from_m31_array([M31(1), M31(2), M31(0), M31(0)]),
            memory.get(2).0
        );
        assert_eq!(M31::from(1 << 30), memory.get(5).0.try_into().unwrap());
        assert_eq!(M31::from(0x7FFF_FFFF), memory.get(8).0.try_into().unwrap());

        // Test duplicates
        assert_eq!(QM31::from_m31_array([M31(1); 4]), memory.get(100).0);
        assert_eq!(
            memory.address_to_value_index[0],
            memory.address_to_value_index[100]
        );

        assert_eq!(M31::from(1 << 30), memory.get(105).0.try_into().unwrap());
        assert_eq!(
            memory.address_to_value_index[5],
            memory.address_to_value_index[105]
        );
    }
}
