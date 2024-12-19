use std::collections::HashMap;
use std::ops::Index;

use relocatable::{Relocatable, RelocationTable, Segment};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use self::relocatable::MaybeRelocatable;
use crate::utils::{maybe_resize, u32_from_usize, usize_from_u32};

pub mod relocatable;

pub type MaybeRelocatableAddr = MaybeRelocatable<M31>;
pub type MaybeRelocatableValue = MaybeRelocatable<QM31>;

// TODO: confirm this limit.
const MAX_MEMORY_SIZE_BITS: u8 = 30;

#[derive(Clone, Debug, Default)]
pub struct Memory {
    // TODO(alont) Consdier changing the implementation to segment -> (offset -> value) for memory
    // locality.
    relocatable_data: Vec<Vec<Option<MaybeRelocatable<QM31>>>>,
    // TODO: convert to a vector.
    absolute_data: HashMap<M31, MaybeRelocatableValue>,
}

impl<T: Into<MaybeRelocatableAddr>> Index<T> for Memory {
    type Output = MaybeRelocatableValue;
    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            MaybeRelocatableAddr::Absolute(addr) => &self.absolute_data[&addr],
            MaybeRelocatable::Relocatable(Relocatable { segment, offset }) => {
                let segment_info = &self.relocatable_data[segment];
                let offset = usize_from_u32(offset.0);

                segment_info[offset].as_ref().unwrap_or_else(|| {
                    panic!("Offset {offset} is out of bounds for segment {segment}.");
                })
            }
        }
    }
}

impl<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>> Extend<(T, S)> for Memory {
    fn extend<I: IntoIterator<Item = (T, S)>>(&mut self, iter: I) {
        for (key, value) in iter {
            self.insert(key, value);
        }
    }
}

impl<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>> FromIterator<(T, S)>
    for Memory
{
    fn from_iter<I: IntoIterator<Item = (T, S)>>(iter: I) -> Self {
        let mut memory = Self::default();

        for (key, value) in iter {
            memory.insert(key, value);
        }

        memory
    }
}

impl Memory {
    pub fn relocate(&mut self, table: &RelocationTable) {
        let relocated_data = self
            .relocatable_data
            .iter()
            .enumerate()
            .flat_map(|(segment, segment_info)| relocate_segment(segment, segment_info, table));

        self.absolute_data.extend(relocated_data);
        self.relocatable_data.clear();
    }

    pub fn insert<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>>(
        &mut self,
        key: T,
        value: S,
    ) -> Option<MaybeRelocatableValue> {
        let value = value.into();

        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => {
                validate_address(addr);
                self.absolute_data.insert(addr, value)
            }
            MaybeRelocatableAddr::Relocatable(Relocatable { segment, offset }) => {
                maybe_resize(&mut self.relocatable_data, segment, Vec::new());

                let segment_info = &mut self.relocatable_data[segment];
                let offset = usize_from_u32(offset.0);
                maybe_resize(segment_info, offset, None);

                std::mem::replace(&mut segment_info[offset], Some(value))
            }
        }
    }

    pub fn get<T: Into<MaybeRelocatableAddr>>(&self, key: T) -> Option<MaybeRelocatableValue> {
        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.get(&addr).copied(),
            MaybeRelocatableAddr::Relocatable(Relocatable { segment, offset }) => *self
                .relocatable_data
                .get(segment)?
                .get(usize_from_u32(offset.0))?,
        }
    }
}

// Utils.

fn validate_address(address: M31) {
    if address.0 > (1 << MAX_MEMORY_SIZE_BITS) {
        panic!("Max memory size is 2 ** {MAX_MEMORY_SIZE_BITS}; got address: {address}.")
    }
}

fn relocate_segment(
    segment: Segment,
    segment_info: &[Option<MaybeRelocatable<QM31>>],
    table: &RelocationTable,
) -> Vec<(M31, MaybeRelocatableValue)> {
    segment_info
        .iter()
        .enumerate()
        .filter_map(move |(offset, option_value)| {
            option_value.as_ref().map(|value| {
                let key = Relocatable::from((segment, u32_from_usize(offset)));
                (key.relocate(table), value.relocate(table).into())
            })
        })
        .collect()
}

#[cfg(test)]
mod test {
    use num_traits::Zero;
    use stwo_prover::core::fields::m31::M31;
    use stwo_prover::core::fields::qm31::QM31;

    use crate::memory::relocatable::Relocatable;
    use crate::memory::Memory;

    #[test]
    fn test_relocate_memory() {
        let mut memory = Memory::default();
        memory.insert(Relocatable::from((0, 0)), QM31::zero());
        memory.insert(Relocatable::from((1, 1)), Relocatable::from((1, 12)));

        let table = [(0, M31(1)), (1, M31(1234))].iter().cloned().collect();

        memory.relocate(&table);

        assert_eq!(memory[M31(1)], QM31::zero().into());
        assert_eq!(memory[M31(1235)], QM31::from(M31(1246)).into());
    }
}
