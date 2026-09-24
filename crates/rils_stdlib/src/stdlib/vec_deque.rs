//! Native double-ended queue operations.

use rils_builtins_macros::decl_rils;

use super::prelude::Option;

#[decl_rils(core::vec_deque)]
mod native {
    use super::Option;

    /// A growable double-ended queue.
    #[rils_opaque]
    pub struct VecDeque<T>(std::collections::VecDeque<T>);

    impl<T> VecDeque<T> {
        pub fn from_std(values: std::collections::VecDeque<T>) -> Self {
            Self(values)
        }

        pub fn into_std(self) -> std::collections::VecDeque<T> {
            self.0
        }

        pub fn clone_front_with<E>(
            values: &std::collections::VecDeque<T>,
            clone: impl FnOnce(&T) -> std::result::Result<T, E>,
        ) -> std::result::Result<Option<T>, E> {
            match values.front() {
                Some(value) => clone(value).map(Option::Some),
                None => Ok(Option::None),
            }
        }

        pub fn clone_back_with<E>(
            values: &std::collections::VecDeque<T>,
            clone: impl FnOnce(&T) -> std::result::Result<T, E>,
        ) -> std::result::Result<Option<T>, E> {
            match values.back() {
                Some(value) => clone(value).map(Option::Some),
                None => Ok(Option::None),
            }
        }

        /// Creates an empty queue.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::VecDeque::new())
        }

        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns whether the queue is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Adds an element at the front.
        #[export_rils]
        pub fn push_front(&mut self, value: T) {
            self.0.push_front(value);
        }

        /// Adds an element at the back.
        #[export_rils]
        pub fn push_back(&mut self, value: T) {
            self.0.push_back(value);
        }

        /// Removes the front element.
        #[export_rils]
        pub fn pop_front(&mut self) -> Option<T> {
            match self.0.pop_front() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Removes the back element.
        #[export_rils]
        pub fn pop_back(&mut self) -> Option<T> {
            match self.0.pop_back() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Clones the front element.
        #[export_rils]
        pub fn front_cloned(&self) -> Option<T>
        where
            T: Clone,
        {
            Self::clone_front_with(&self.0, |value| {
                Ok::<T, std::convert::Infallible>(value.clone())
            })
            .unwrap_or_else(|never| match never {})
        }

        /// Clones the back element.
        #[export_rils]
        pub fn back_cloned(&self) -> Option<T>
        where
            T: Clone,
        {
            Self::clone_back_with(&self.0, |value| {
                Ok::<T, std::convert::Infallible>(value.clone())
            })
            .unwrap_or_else(|never| match never {})
        }

        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl<T> std::default::Default for VecDeque<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use native::VecDeque;
