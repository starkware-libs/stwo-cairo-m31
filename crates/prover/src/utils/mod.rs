pub mod component;
pub mod prover;

use std::ops::{Add, Mul, Sub};

use num_traits::One;
use stwo_prover::core::fields::FieldExpOps;

pub struct Selector();

pub trait SelectorTrait<T, const N: usize> {
    /// Selects between the elements of `elements` using `selector`.
    /// Note that `selector` has to be externally verified to be in the range [0, N-1]
    fn select(selector: &T, elements: [&T; N]) -> T;
}

impl<T> SelectorTrait<T, 2> for Selector
where
    T: Clone + One + Mul<Output = T> + Add<Output = T> + Sub<Output = T>,
{
    fn select(selector: &T, [a, b]: [&T; 2]) -> T {
        selector.clone() * b.clone() + (T::one() - selector.clone()) * a.clone()
    }
}

impl<T> SelectorTrait<T, 3> for Selector
where
    T: Clone + One + Mul<Output = T> + Add<Output = T> + Sub<Output = T> + FieldExpOps,
{
    fn select(selector: &T, [a, b, c]: [&T; 3]) -> T {
        let selector_minus_1 = selector.clone() - T::one();
        let two = T::one() + T::one();
        let selector_minus_2 = selector.clone() - two.clone();
        let field_half = two.inverse();

        (selector_minus_2.clone() * selector_minus_1.clone() * field_half.clone() * a.clone())
            - (selector_minus_2 * selector.clone() * b.clone())
            + (selector_minus_1.clone() * selector.clone() * field_half * c.clone())
    }
}

#[cfg(test)]
mod tests {
    use stwo_prover::core::fields::m31::M31;

    use crate::utils::{Selector, SelectorTrait};

    #[test]
    fn test_selector() {
        assert_eq!(Selector::select(&M31(0), [&M31(1), &M31(2)]), M31(1));
        assert_eq!(Selector::select(&M31(1), [&M31(5), &M31(6)]), M31(6));

        assert_eq!(
            Selector::select(&M31(0), [&M31(1), &M31(2), &M31(3)]),
            M31(1)
        );
        assert_eq!(
            Selector::select(&M31(1), [&M31(5), &M31(6), &M31(7)]),
            M31(6)
        );
        assert_eq!(
            Selector::select(&M31(2), [&M31(9), &M31(10), &M31(11)]),
            M31(11)
        );
    }
}
