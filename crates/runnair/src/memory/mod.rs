use std::collections::HashMap;
use std::ops::Index;

use relocatable::Relocatable;
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
    relocatable_data: Vec<Vec<MaybeRelocatableValue>>,
    absolute_data: HashMap<M31, MaybeRelocatableValue>,
}

impl<T: Into<MaybeRelocatableAddr>> Index<T> for Memory {
    type Output = MaybeRelocatableValue;
    #[inline(always)]
    fn index(&self, index: T) -> &Self::Output {
        match index.into() {
            MaybeRelocatableAddr::Absolute(addr) => &self.absolute_data[&addr],
            MaybeRelocatable::Relocatable(Relocatable { segment, offset }) => {
                &self.relocatable_data[segment][offset.0 as usize]
            }
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
    #[inline(always)]
    pub fn insert<T: Into<MaybeRelocatableAddr>, S: Into<MaybeRelocatableValue>>(
        &mut self,
        key: T,
        value: S,
    ) -> Option<MaybeRelocatableValue> {
        let value = value.into();
        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.insert(addr, value),
            MaybeRelocatableAddr::Relocatable(Relocatable { segment, offset }) => {
                let offset = offset.0 as usize;

                let n_segments = self.relocatable_data.len();
                if segment >= n_segments {
                    let resize_by = std::cmp::max(segment + 1, n_segments * 2);
                    self.relocatable_data.resize(resize_by, Vec::new());
                }

                let segment_size = self.relocatable_data[segment].len();
                if offset >= segment_size {
                    self.relocatable_data[segment].resize(offset + 1, value);
                    return None;
                }

                let old_value =
                    std::mem::replace(&mut self.relocatable_data[segment][offset], value);
                Some(old_value)
            }
        }
    }

    #[inline(always)]
    pub fn get<T: Into<MaybeRelocatableAddr>>(&self, key: T) -> Option<MaybeRelocatableValue> {
        match key.into() {
            MaybeRelocatableAddr::Absolute(addr) => self.absolute_data.get(&addr).copied(),
            MaybeRelocatableAddr::Relocatable(Relocatable { segment, offset }) => self
                .relocatable_data
                .get(segment)?
                .get(offset.0 as usize)
                .copied(),
        }
    }
}
