//! Growable owned sequence backed by Rust's Vec.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {
    use super::super::{Iter, Iterator, Option};

    /// A growable owned sequence.
    #[rils_struct]
    pub struct Vec<T>(std::vec::Vec<T>);

    impl<T> std::ops::Deref for Vec<T> {
        type Target = std::vec::Vec<T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Vec<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> Default for Vec<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> From<std::vec::Vec<T>> for Vec<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self(values)
        }
    }

    impl<T, const N: usize> From<[T; N]> for Vec<T> {
        fn from(values: [T; N]) -> Self {
            Self(std::vec::Vec::from(values))
        }
    }

    impl<T> IntoIterator for Vec<T> {
        type Item = T;
        type IntoIter = std::vec::IntoIter<T>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<T> Vec<T> {
        /// Creates an empty Vec.
        #[export_rils]
        #[rils_import(core::vec::new)]
        pub fn new() -> Self {
            Self(std::vec::Vec::new())
        }

        /// Creates a Vec from an owned array.
        #[export_rils]
        #[rils_import(core::vec::from)]
        #[rils_any(values)]
        #[allow(clippy::should_implement_trait)]
        pub fn from<const N: usize>(values: [T; N]) -> Self {
            Self(std::vec::Vec::from(values))
        }

        /// Returns the element count.
        #[export_rils]
        #[rils_legacy_id(core::sequence::len)]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns true when the Vec has no elements.
        #[export_rils]
        #[rils_legacy_id(core::sequence::is_empty)]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Appends one element.
        #[export_rils]
        pub fn push(&mut self, value: T) {
            self.0.push(value);
        }

        /// Removes and returns the last element.
        #[export_rils]
        pub fn pop(&mut self) -> Option<T> {
            self.0.pop().map_or(Option::None, Option::Some)
        }

        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }

        /// Shortens the Vec to at most the supplied length.
        #[export_rils]
        pub fn truncate(&mut self, length: usize) {
            self.0.truncate(length);
        }

        /// Inserts an element at the supplied index.
        #[export_rils]
        pub fn insert(&mut self, index: usize, value: T) {
            self.0.insert(index, value);
        }

        /// Removes and returns the element at the supplied index.
        #[export_rils]
        pub fn remove(&mut self, index: usize) -> T {
            self.0.remove(index)
        }

        /// Removes an element by replacing it with the final element.
        #[export_rils]
        pub fn swap_remove(&mut self, index: usize) -> T {
            self.0.swap_remove(index)
        }

        /// Moves every element from another Vec into this Vec.
        #[export_rils]
        pub fn extend(&mut self, other: Self) {
            self.0.extend(other.0);
        }

        /// Consumes the Vec and creates an iterator.
        #[export_rils]
        #[rils_legacy_id(core::sequence::into_iter)]
        #[allow(clippy::should_implement_trait)]
        pub fn into_iter(self) -> Iterator<T> {
            Iterator(self.0)
        }

        /// Borrows each element without consuming the Vec.
        #[export_rils]
        #[rils_legacy_id(core::sequence::iter)]
        pub fn iter(&self) -> Iter<&T> {
            Iter::from(self.0.iter().collect::<std::vec::Vec<_>>())
        }
    }

    impl<T: PartialEq> Vec<T> {
        /// Returns true when an equal element is present.
        #[export_rils]
        #[rils_legacy_id(core::sequence::contains)]
        pub fn contains(&self, value: &T) -> bool {
            self.0.contains(value)
        }
    }
}

pub use native::Vec;
