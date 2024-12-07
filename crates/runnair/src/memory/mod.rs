use std::collections::HashMap;
use std::ops::Index;

use relocatable::{Relocatable, RelocationTable};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use self::relocatable::MaybeRelocatable;

pub mod relocatable;

pub type MaybeRelocatableAddr = MaybeRelocatable<M31>;
pub type MaybeRelocatableValue = MaybeRelocatable<QM31>;

// TODO: confirm this limit.
const MAX_MEMORY_SIZE_BITS: u8 = 30;

#[derive(Clone, Debug, Default)]
pub struct Memory {
    // TODO(alont) Consdier changing the implementation to segment -> (offset -> value) for memory
    // locality.
    relocatable_data: Vec<HashMap<M31, MaybeRelocatableValue>>,
    absolute_data: HashMap<M31, MaybeRelocatableValue>,
}

impl<T: Into<MaybeRelocatableAddr>> Index<T> for Memory {
    type Output = MaybeRelocatableValue;
    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            MaybeRelocatableAddr::Absolute(addr) => &self.absolute_data[&addr],
            MaybeRelocatable::Relocatable(Relocatable { segment, offset }) => {
                &self.relocatable_data[segment][&offset]
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
        let relocated_data =
            self.relocatable_data
                .iter()
                .enumerate()
                .flat_map(|(segment, segment_info)| {
                    segment_info.iter().map(move |(&offset, value)| {
                        let key = Relocatable { segment, offset };
                        (key.relocate(table), value.relocate(table).into())
                    })
                });

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
                let n_segments = self.relocatable_data.len();
                if segment >= n_segments {
                    let resize_by = if n_segments == 0 { 1 } else { n_segments * 2 };
                    self.relocatable_data.resize(resize_by, HashMap::new());
                }
                self.relocatable_data[segment].insert(offset, value)
            }
        }
    }

    pub fn get<T: Into<MaybeRelocatableAddr>>(&self, key: T) -> Option<MaybeRelocatableValue> {
        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.get(&addr).copied(),
            MaybeRelocatableAddr::Relocatable(Relocatable { segment, offset }) => self
                .relocatable_data
                .get(segment)
                .and_then(|segment| segment.get(&offset).copied()),
        }
    }
}

// Utils.

fn validate_address(address: M31) {
    if address.0 > (1 << MAX_MEMORY_SIZE_BITS) {
        panic!("Max memory size is 2 ** {MAX_MEMORY_SIZE_BITS}; got address: {address}.")
    }
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
