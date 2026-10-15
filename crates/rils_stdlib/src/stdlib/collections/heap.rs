//! Max-priority queue methods over a replaceable storage backend.
use rils_stdlib_macros::decl_rils;

/// Values accepted by the Rils max-priority queue.
pub trait HeapElement: Ord + Clone {}

macro_rules! heap_elements {
    ($($ty:ty),* $(,)?) => {
        $(impl HeapElement for $ty {})*

        /// Extract an ordered identity using the same element declarations as
        /// the Rust implementation; the execution layer needs no type list.
        pub fn native_heap_key(
            view: rils_value::DynamicValueRef<'_>,
        ) -> Result<rils_native::NativeKey, String> {
            let leaf = view.leaf()?;
            if !($(leaf.is_rust_type::<$ty>())||*) {
                return Err(format!("BinaryHeap does not support ordering {}", leaf.rils_type()));
            }
            crate::native::registry().ordered_key_leaf(&leaf)
        }
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
    crate::stdlib::string::String
);

#[decl_rils(core::collections)]
mod native {
    /// An owned max-priority queue. Elements must be orderable integers, char, or string.
    #[rils_struct]
    pub struct BinaryHeap<
        T,
        #[rils_storage(sequence, rils_stdlib::stdlib::collections::BinaryHeap)] S = std::collections::BinaryHeap<T>,
    >(S, std::marker::PhantomData<T>);

    impl<T, S> BinaryHeap<T, S> {
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
        pub fn into_storage(self) -> S {
            self.0
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceRead> BinaryHeap<T, S> {
        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns whether the queue is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceHeap<T>> BinaryHeap<T, S> {
        /// Inserts an element, rejecting unsupported ordering types.
        #[export_rils]
        #[rils_return(())]
        pub fn push(&mut self, value: T) -> S::WriteOutput {
            self.0.push(value)
        }
        /// Removes and returns the greatest element.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn pop(&mut self) -> S::PopOutput {
            self.0.pop()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceClone> BinaryHeap<T, S> {
        /// Explicitly clones the highest-priority element.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn peek_cloned(&self) -> S::CloneOutput {
            self.0.front_cloned()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceClear> BinaryHeap<T, S> {
        /// Removes all elements.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::ClearOutput {
            self.0.clear()
        }
    }
    #[rils_impl]
    impl<T, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for BinaryHeap<T, S> {
        #[rils_associated(T)]
        type Item = S::Item;
        #[rils_associated(BinaryHeapIntoIter<T>)]
        type IntoIter = S::IntoIter;
        /// Consumes the heap in its storage order.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> BinaryHeap<T, S> {
        /// Creates an empty max-priority queue.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> Default for BinaryHeap<T, S> {
        fn default() -> Self {
            Self::new()
        }
    }
}
pub use native::BinaryHeap;
