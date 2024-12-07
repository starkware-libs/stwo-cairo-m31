use std::collections::HashMap;
use std::ops::Index;

use relocatable::{Relocatable, RelocationTable};
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

use self::relocatable::MaybeRelocatable;

pub mod relocatable;

pub type MaybeRelocatableAddr = MaybeRelocatable<M31>;
pub type MaybeRelocatableValue = MaybeRelocatable<QM31>;

#[derive(Debug, Clone, Default)]
pub struct Memory {
    // TODO(alont) Consdier changing the implementation to segment -> (offset -> value) for memory
    // locality.
    relocatable_data: HashMap<Relocatable, MaybeRelocatableValue>,
    absolute_data: HashMap<M31, MaybeRelocatableValue>,
}

impl<T: Into<MaybeRelocatableAddr>> Index<T> for Memory {
    type Output = MaybeRelocatableValue;
    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            MaybeRelocatableAddr::Absolute(addr) => &self.absolute_data[&addr],
            MaybeRelocatableAddr::Relocatable(addr) => &self.relocatable_data[&addr],
        }
    }
}

impl<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>> FromIterator<(T, S)>
    for Memory
{
    fn from_iter<I: IntoIterator<Item = (T, S)>>(iter: I) -> Self {
        let mut relocatable_data = HashMap::new();
        let mut absolute_data = HashMap::new();

        for (key, value) in iter {
            let value = value.into();

            match key.into() {
                MaybeRelocatableAddr::Relocatable(addr) => {
                    relocatable_data.insert(addr, value);
                }
                MaybeRelocatableAddr::Absolute(addr) => {
                    absolute_data.insert(addr, value);
                }
            }
        }

        Self {
            relocatable_data,
            absolute_data,
        }
    }
}

impl Memory {
    pub fn relocate(&mut self, table: &RelocationTable) {
        let relocated_data = self.relocatable_data.iter().map(|(key, value)| {
            (
                key.relocate(table),
                MaybeRelocatableValue::from(value.relocate(table)),
            )
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
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.insert(addr, value),
            MaybeRelocatableAddr::Relocatable(addr) => self.relocatable_data.insert(addr, value),
        }
    }

    pub fn get<T: Into<MaybeRelocatableAddr>>(&self, key: T) -> Option<MaybeRelocatableValue> {
        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.get(&addr).copied(),
            MaybeRelocatableAddr::Relocatable(addr) => self.relocatable_data.get(&addr).copied(),
        }
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
