pub mod component;
pub mod prover;

use std::ops::{Add, Mul, Sub};

use num_traits::One;
/// Selects between `a` and `b` using `bit`.
/// Note that `bit` has to be externally verified to actually be a bit.
pub fn select_by_bit<T>(bit: T, a: T, b: T) -> T
where
    T: Clone + One + Mul<Output = T> + Add<Output = T> + Sub<Output = T>,
{
    bit.clone() * b.clone() + (T::one() - bit.clone()) * a.clone()
}
