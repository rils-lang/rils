//! Double-ended sequence methods over a replaceable storage backend.
use rils_stdlib_macros::decl_rils;
#[decl_rils(core::collections)]
mod native {
    /// An owned double-ended queue.
    #[rils_struct]
    pub struct VecDeque<
        T,
        #[rils_storage(sequence, rils_stdlib::stdlib::collections::VecDeque)] S = std::collections::VecDeque<T>,
    >(S, std::marker::PhantomData<T>);
    impl<T, S> VecDeque<T, S> {
        pub fn into_storage(self) -> S {
            self.0
        }
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceRead> VecDeque<T, S> {
        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns whether the deque contains no elements.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceDeque<T>> VecDeque<T, S> {
        /// Adds an element at the front.
        #[export_rils]
        #[rils_return(())]
        pub fn push_front(&mut self, value: T) -> S::WriteOutput {
            self.0.push_front(value)
        }
        /// Adds an element at the back.
        #[export_rils]
        #[rils_return(())]
        pub fn push_back(&mut self, value: T) -> S::WriteOutput {
            self.0.push_back(value)
        }
        /// Removes and returns the first element, or None when empty.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn pop_front(&mut self) -> S::PopOutput {
            self.0.pop_front()
        }
        /// Removes and returns the last element, or None when empty.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn pop_back(&mut self) -> S::PopOutput {
            self.0.pop_back()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceTruncate> VecDeque<T, S> {
        /// Removes all elements.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::TruncateOutput {
            self.0.truncate(0)
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceClone> VecDeque<T, S> {
        /// Clones the first element, or returns None when empty.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn front_cloned(&self) -> S::CloneOutput {
            self.0.front_cloned()
        }
        /// Clones the last element, or returns None when empty.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn back_cloned(&self) -> S::CloneOutput {
            self.0.back_cloned()
        }
    }
    #[rils_impl]
    impl<T, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for VecDeque<T, S> {
        #[rils_associated(T)]
        type Item = S::Item;
        #[rils_associated(VecDequeIntoIter<T>)]
        type IntoIter = S::IntoIter;
        /// Consumes the deque and yields its elements from front to back.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> VecDeque<T, S> {
        /// Creates an empty deque.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> Default for VecDeque<T, S> {
        fn default() -> Self {
            Self::new()
        }
    }
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
            Self::with_storage(values)
        }
    }
    impl<T> From<VecDeque<T>> for std::collections::VecDeque<T> {
        fn from(values: VecDeque<T>) -> Self {
            values.0
        }
    }
}
pub use native::VecDeque;
