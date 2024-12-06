use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::ops::Index;

use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use self::relocatable::{MaybeRelocatable, RelocationTable};

pub mod relocatable;

pub type MaybeRelocatableAddr = MaybeRelocatable<M31>;
pub type MaybeRelocatableValue = MaybeRelocatable<QM31>;

#[derive(Debug, Clone, Default)]
pub struct Memory {
    // TODO(alont) Consdier changing the implementation to segment -> (offset -> value) for memory
    // locality.
    data: HashMap<MaybeRelocatableAddr, MaybeRelocatableValue>,
}

impl<T: Into<MaybeRelocatableAddr>> Index<T> for Memory {
    type Output = MaybeRelocatableValue;
    fn index(&self, index: T) -> &Self::Output {
        &self.data[&index.into()]
    }
}

impl From<HashMap<MaybeRelocatableAddr, MaybeRelocatableValue>> for Memory {
    fn from(data: HashMap<MaybeRelocatableAddr, MaybeRelocatableValue>) -> Self {
        Self { data }
    }
}

impl<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>> FromIterator<(T, S)>
    for Memory
{
    fn from_iter<I: IntoIterator<Item = (T, S)>>(iter: I) -> Self {
        Self {
            data: iter
                .into_iter()
                .map(|(key, value)| (key.into(), value.into()))
                .collect(),
        }
    }
}

impl Memory {
    pub fn relocate(&mut self, table: &RelocationTable) {
        *self = self
            .data
            .iter()
            .map(|(key, value)| (key.relocate(table), value.relocate(table)))
            .collect();
    }

    pub fn insert<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>>(
        &mut self,
        key: T,
        value: S,
    ) -> Option<MaybeRelocatableValue> {
        self.data.insert(key.into(), value.into())
    }

    pub fn entry<T: Into<MaybeRelocatableAddr>>(
        &mut self,
        key: T,
    ) -> Entry<'_, MaybeRelocatableAddr, MaybeRelocatableValue> {
        self.data.entry(key.into())
    }

    pub fn get<T: Into<MaybeRelocatableAddr>>(&self, key: T) -> Option<MaybeRelocatableValue> {
        self.data.get(&key.into()).copied()
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
