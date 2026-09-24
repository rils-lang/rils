//! Native collection operations.

use rils_builtins_macros::decl_rils;

use super::prelude::Option;

/// Values accepted by the Rils max-priority queue.
pub trait HeapElement: Ord + Clone {}

macro_rules! heap_elements {
    ($($ty:ty),* $(,)?) => {
        $(impl HeapElement for $ty {})*
    };
}

heap_elements!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    char,
    super::string::String
);

#[decl_rils(core::collections)]
mod native {
    use super::{HeapElement, Option};

    /// A growable double-ended queue.
    #[rils_struct]
    pub struct VecDeque<T>(std::collections::VecDeque<T>);

    impl<T> std::ops::Deref for VecDeque<T> {
        type Target = std::collections::VecDeque<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for VecDeque<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::collections::VecDeque<T>> for VecDeque<T> {
        fn from(values: std::collections::VecDeque<T>) -> Self {
            Self(values)
        }
    }

    impl<T> From<VecDeque<T>> for std::collections::VecDeque<T> {
        fn from(values: VecDeque<T>) -> Self {
            values.0
        }
    }

    impl<T> VecDeque<T> {
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

    /// An owned max-priority queue. Elements must be orderable integers, char, or string.
    #[rils_struct]
    pub struct BinaryHeap<T>(std::collections::BinaryHeap<T>);

    impl<T: HeapElement> BinaryHeap<T> {
        /// Creates an empty max-priority queue.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::BinaryHeap::new())
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

        /// Inserts an element, rejecting unsupported ordering types.
        #[export_rils]
        pub fn push(&mut self, value: T) {
            self.0.push(value);
        }

        /// Removes and returns the greatest element.
        #[export_rils]
        pub fn pop(&mut self) -> Option<T> {
            match self.0.pop() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Explicitly clones the highest-priority element.
        #[export_rils]
        pub fn peek_cloned(&self) -> Option<T> {
            match self.0.peek() {
                Some(value) => Option::Some(value.clone()),
                None => Option::None,
            }
        }

        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl<T: HeapElement> std::default::Default for BinaryHeap<T> {
        fn default() -> Self {
            Self::new()
        }
    }
}

pub use native::{BinaryHeap, VecDeque};
