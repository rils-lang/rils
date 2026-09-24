pub mod basic;
pub mod collections;
pub mod float;
pub mod integer;
pub mod option;
pub mod prelude;
pub mod range;
pub mod result;
pub mod string;
pub mod traits;

pub mod binary_heap {
    pub use super::collections::{BinaryHeap, HeapElement};
}

pub mod vec_deque {
    pub use super::collections::VecDeque;
}
