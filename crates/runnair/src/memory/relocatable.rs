use std::collections::HashMap;
use std::ops::{Add, Div, Mul, Sub};

use num_traits::Zero;
use stwo_prover::core::fields::m31::M31;
use stwo_prover::core::fields::qm31::QM31;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Relocatable {
    segment: isize,
    offset: M31,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MaybeRelocatable<T: From<M31>> {
    Relocatable(Relocatable),
    Absolute(T),
}

pub type RelocationTable = HashMap<isize, M31>;

impl Relocatable {
    pub fn relocate(&self, table: &RelocationTable) -> M31 {
        table[&self.segment] + self.offset
    }
}

impl<T: From<M31> + Copy> MaybeRelocatable<T> {
    pub fn relocate(&self, table: &RelocationTable) -> T {
        match self {
            MaybeRelocatable::Relocatable(x) => x.relocate(table).into(),
            MaybeRelocatable::Absolute(x) => *x,
        }
    }
}

impl From<(isize, M31)> for Relocatable {
    fn from((segment, offset): (isize, M31)) -> Self {
        Relocatable { segment, offset }
    }
}

impl From<(isize, u32)> for Relocatable {
    fn from((segment, offset): (isize, u32)) -> Self {
        Relocatable {
            segment,
            offset: M31(offset),
        }
    }
}

impl<T: From<M31>> From<Relocatable> for MaybeRelocatable<T> {
    fn from(relocatable: Relocatable) -> Self {
        MaybeRelocatable::Relocatable(relocatable)
    }
}

impl<T: From<M31>> From<T> for MaybeRelocatable<T> {
    fn from(value: T) -> Self {
        MaybeRelocatable::Absolute(value)
    }
}

impl From<M31> for MaybeRelocatable<QM31> {
    fn from(value: M31) -> Self {
        MaybeRelocatable::Absolute(value.into())
    }
}

impl From<MaybeRelocatable<M31>> for MaybeRelocatable<QM31> {
    fn from(value: MaybeRelocatable<M31>) -> Self {
        match value {
            MaybeRelocatable::Relocatable(x) => MaybeRelocatable::Relocatable(x),
            MaybeRelocatable::Absolute(x) => MaybeRelocatable::Absolute(x.into()),
        }
    }
}

impl Add<M31> for Relocatable {
    type Output = Self;
    fn add(self, rhs: M31) -> Self {
        Self {
            segment: self.segment,
            offset: self.offset + rhs,
        }
    }
}

// TODO(alont): Can this be generalized?
impl Add<MaybeRelocatable<M31>> for MaybeRelocatable<M31> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs + rhs)
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                MaybeRelocatable::Relocatable(rhs + lhs)
            }
            (MaybeRelocatable::Relocatable(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot add two relocatables.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs + rhs)
            }
        }
    }
}

/// Assert that the input is in the base field and return the projection to the base field.
fn assert_and_project_on_felt(x: QM31) -> M31 {
    assert!(x.1.is_zero());
    assert!(x.0 .1.is_zero());
    x.0 .0
}

// TODO: change to `TryFrom` if we add error handling.
/// For an absolute value, assert that the input is in the base field and return the projection to
/// the base field.
/// For a relocatable value, simply returns as-is.
pub(crate) fn assert_and_project(x: MaybeRelocatable<QM31>) -> MaybeRelocatable<M31> {
    match x {
        MaybeRelocatable::Relocatable(x) => MaybeRelocatable::<M31>::Relocatable(x),
        MaybeRelocatable::Absolute(x) => MaybeRelocatable::Absolute(assert_and_project_on_felt(x)),
    }
}

impl Add<MaybeRelocatable<QM31>> for MaybeRelocatable<M31> {
    type Output = MaybeRelocatable<QM31>;
    fn add(self, rhs: MaybeRelocatable<QM31>) -> MaybeRelocatable<QM31> {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs + assert_and_project_on_felt(rhs))
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                MaybeRelocatable::Relocatable(rhs + lhs)
            }
            (MaybeRelocatable::Relocatable(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot add two relocatables.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs + rhs)
            }
        }
    }
}

impl Add<MaybeRelocatable<M31>> for MaybeRelocatable<QM31> {
    type Output = Self;
    fn add(self, rhs: MaybeRelocatable<M31>) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs + rhs)
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                MaybeRelocatable::Relocatable(rhs + assert_and_project_on_felt(lhs))
            }
            (MaybeRelocatable::Relocatable(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot add two relocatables.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs + rhs)
            }
        }
    }
}

impl Add<MaybeRelocatable<QM31>> for MaybeRelocatable<QM31> {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs + assert_and_project_on_felt(rhs))
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                MaybeRelocatable::Relocatable(rhs + assert_and_project_on_felt(lhs))
            }
            (MaybeRelocatable::Relocatable(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot add two relocatables.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs + rhs)
            }
        }
    }
}

impl<T: Add<M31, Output = T> + From<M31>> Add<M31> for MaybeRelocatable<T> {
    type Output = Self;
    fn add(self, rhs: M31) -> Self {
        match self {
            MaybeRelocatable::Relocatable(lhs) => MaybeRelocatable::Relocatable(lhs + rhs),
            MaybeRelocatable::Absolute(lhs) => MaybeRelocatable::Absolute(lhs + rhs),
        }
    }
}

impl Sub<M31> for Relocatable {
    type Output = Self;
    fn sub(self, rhs: M31) -> Self {
        Self {
            segment: self.segment,
            offset: self.offset - rhs,
        }
    }
}

impl Sub<MaybeRelocatable<M31>> for MaybeRelocatable<M31> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs - rhs)
            }
            (MaybeRelocatable::Absolute(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot subtract a relocatable from an absolute.")
            }
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                if lhs.segment != rhs.segment {
                    panic!("Cannot subtract relocatables from different segments.");
                }
                MaybeRelocatable::Absolute(lhs.offset - rhs.offset)
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs - rhs)
            }
        }
    }
}

impl Sub<MaybeRelocatable<QM31>> for MaybeRelocatable<M31> {
    type Output = MaybeRelocatable<QM31>;
    fn sub(self, rhs: MaybeRelocatable<QM31>) -> MaybeRelocatable<QM31> {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs - assert_and_project_on_felt(rhs))
            }
            (MaybeRelocatable::Absolute(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot subtract a relocatable from an absolute.")
            }
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                if lhs.segment != rhs.segment {
                    panic!("Cannot subtract relocatables from different segments.");
                }
                MaybeRelocatable::Absolute((lhs.offset - rhs.offset).into())
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs - rhs)
            }
        }
    }
}

impl Sub<MaybeRelocatable<M31>> for MaybeRelocatable<QM31> {
    type Output = Self;
    fn sub(self, rhs: MaybeRelocatable<M31>) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs - rhs)
            }
            (MaybeRelocatable::Absolute(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot subtract a relocatable from an absolute.")
            }
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                if lhs.segment != rhs.segment {
                    panic!("Cannot subtract relocatables from different segments.");
                }
                MaybeRelocatable::Absolute(QM31::from(lhs.offset - rhs.offset))
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs - rhs)
            }
        }
    }
}

impl Sub<MaybeRelocatable<QM31>> for MaybeRelocatable<QM31> {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Relocatable(lhs - assert_and_project_on_felt(rhs))
            }
            (MaybeRelocatable::Absolute(_), MaybeRelocatable::Relocatable(_)) => {
                panic!("Cannot subtract a relocatable from an absolute.")
            }
            (MaybeRelocatable::Relocatable(lhs), MaybeRelocatable::Relocatable(rhs)) => {
                if lhs.segment != rhs.segment {
                    panic!("Cannot subtract relocatables from different segments.");
                }
                MaybeRelocatable::Absolute(QM31::from(lhs.offset - rhs.offset))
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs - rhs)
            }
        }
    }
}

impl<T: Sub<M31, Output = T> + From<M31>> Sub<M31> for MaybeRelocatable<T> {
    type Output = Self;
    fn sub(self, rhs: M31) -> Self {
        match self {
            MaybeRelocatable::Relocatable(lhs) => MaybeRelocatable::Relocatable(lhs - rhs),
            MaybeRelocatable::Absolute(lhs) => MaybeRelocatable::Absolute(lhs - rhs),
        }
    }
}

impl<T: From<M31> + Mul<S, Output = S>, S: From<M31>> Mul<MaybeRelocatable<S>>
    for MaybeRelocatable<T>
{
    type Output = MaybeRelocatable<S>;
    fn mul(self, rhs: MaybeRelocatable<S>) -> Self::Output {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(_), _) | (_, MaybeRelocatable::Relocatable(_)) => {
                panic!("Multiplication involving relocatable values is not possible.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs * rhs)
            }
        }
    }
}

impl<T: From<M31> + Mul<M31, Output = T>> Mul<M31> for MaybeRelocatable<T> {
    type Output = Self;
    fn mul(self, rhs: M31) -> Self {
        match self {
            MaybeRelocatable::Relocatable(_) => panic!("Cannot multiply a relocatable."),
            MaybeRelocatable::Absolute(lhs) => MaybeRelocatable::Absolute(lhs * rhs),
        }
    }
}

impl<T: From<M31> + Div<S, Output = S>, S: From<M31>> Div<MaybeRelocatable<S>>
    for MaybeRelocatable<T>
{
    type Output = MaybeRelocatable<S>;
    fn div(self, rhs: MaybeRelocatable<S>) -> Self::Output {
        match (self, rhs) {
            (MaybeRelocatable::Relocatable(_), _) | (_, MaybeRelocatable::Relocatable(_)) => {
                panic!("Division involving relocatable values is not possible.")
            }
            (MaybeRelocatable::Absolute(lhs), MaybeRelocatable::Absolute(rhs)) => {
                MaybeRelocatable::Absolute(lhs / rhs)
            }
        }
    }
}

impl<T: From<M31> + Div<M31, Output = T>> Div<M31> for MaybeRelocatable<T> {
    type Output = Self;
    fn div(self, rhs: M31) -> Self {
        match self {
            MaybeRelocatable::Relocatable(_) => panic!("Cannot divide a relocatable."),
            MaybeRelocatable::Absolute(lhs) => MaybeRelocatable::Absolute(lhs / rhs),
        }
    }
}
