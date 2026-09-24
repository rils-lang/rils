//! Native max-priority queue operations.

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

#[decl_rils(core::binary_heap)]
mod native {
    use super::{HeapElement, Option};

    /// An owned max-priority queue. Elements must be orderable integers, char, or string.
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

pub use native::BinaryHeap;
