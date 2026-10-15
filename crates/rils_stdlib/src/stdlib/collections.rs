//! Native collection operations.

use rils_stdlib_macros::decl_rils;

pub mod btree_map;
pub mod btree_set;
pub mod deque;
pub mod hash;
pub mod hash_map;
pub mod hash_set;
pub mod heap;
pub mod vector;
pub use deque::{VecDeque, vec_deque_layout};
pub use heap::{BinaryHeap, HeapElement, binary_heap_layout, native_heap_key};

#[decl_rils(core::collections)]
mod native {
    /// An owning iterator over queue elements.
    #[rils_struct]
    pub struct VecDequeIntoIter<T>(std::collections::vec_deque::IntoIter<T>);

    #[rils_impl]
    impl<T> std::iter::Iterator for VecDequeIntoIter<T> {
        type Item = T;

        /// Returns the next owned element from the front.
        fn next(&mut self) -> std::option::Option<T> {
            self.0.next()
        }
    }

    impl<T> From<std::collections::vec_deque::IntoIter<T>> for VecDequeIntoIter<T> {
        fn from(cursor: std::collections::vec_deque::IntoIter<T>) -> Self {
            Self(cursor)
        }
    }

    /// An owning iterator over heap elements in storage order.
    #[rils_struct]
    pub struct BinaryHeapIntoIter<T>(std::collections::binary_heap::IntoIter<T>);

    #[rils_impl]
    impl<T> std::iter::Iterator for BinaryHeapIntoIter<T> {
        type Item = T;

        /// Returns the next owned element in storage order.
        fn next(&mut self) -> std::option::Option<T> {
            self.0.next()
        }
    }

    impl<T> From<std::collections::binary_heap::IntoIter<T>> for BinaryHeapIntoIter<T> {
        fn from(cursor: std::collections::binary_heap::IntoIter<T>) -> Self {
            Self(cursor)
        }
    }
}

pub use btree_map::{BTreeMap, BTreeMapIntoIter, btree_map_layout};
pub use btree_set::{BTreeSet, BTreeSetIntoIter, btree_set_layout};
pub use hash::{HashMap, HashMapIntoIter, HashSet, HashSetIntoIter};
pub use native::{BinaryHeapIntoIter, VecDequeIntoIter};
