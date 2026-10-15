//! Growable owned sequence backed by Rust's Vec.

use rils_stdlib_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {

    /// A growable owned sequence.
    #[rils_struct]
    #[repr(transparent)]
    pub struct Vec<
        T,
        #[rils_storage(sequence, rils_stdlib::stdlib::collections::vector::Vec)] S = std::vec::Vec<T>,
    >(S, std::marker::PhantomData<T>);

    impl<T, S> Vec<T, S> {
        /// Wrap a storage backend without converting or copying its elements.
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceRead> Vec<T, S> {
        /// Returns the element count.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns true when the Vec has no elements.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceTruncate> Vec<T, S> {
        /// Removes all elements.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::TruncateOutput {
            self.truncate(0)
        }

        /// Shortens the Vec to at most the supplied length.
        #[export_rils]
        #[rils_return(())]
        pub fn truncate(&mut self, length: usize) -> S::TruncateOutput {
            self.0.truncate(length)
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceTransfer<T>> Vec<T, S> {
        /// Appends one element.
        #[export_rils]
        #[rils_return(())]
        pub fn push(&mut self, value: T) -> S::WriteOutput {
            self.0.push(value)
        }

        /// Removes and returns the last element.
        #[export_rils]
        #[rils_return(Option<T>)]
        pub fn pop(&mut self) -> S::PopOutput {
            self.0.pop()
        }

        /// Inserts an element at the supplied index.
        #[export_rils]
        #[rils_return(())]
        pub fn insert(&mut self, index: usize, value: T) -> S::WriteOutput {
            self.0.insert(index, value)
        }

        /// Removes and returns the element at the supplied index.
        #[export_rils]
        #[rils_return(T)]
        pub fn remove(&mut self, index: usize) -> S::TakeOutput {
            self.0.remove(index)
        }

        /// Removes an element by replacing it with the final element.
        #[export_rils]
        #[rils_return(T)]
        pub fn swap_remove(&mut self, index: usize) -> S::TakeOutput {
            self.0.swap_remove(index)
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceExtend> Vec<T, S> {
        /// Moves every element from another Vec into this Vec.
        #[export_rils]
        #[rils_return(())]
        pub fn extend(&mut self, other: Self) -> S::ExtendOutput {
            self.0.extend(other.0)
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceIter> Vec<T, S> {
        /// Borrows each element without consuming the Vec.
        #[export_rils]
        #[rils_return(Iter<&T>)]
        pub fn iter(&self) -> S::IterOutput<'_> {
            self.0.iter()
        }
    }

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

    impl<T, S: rils_stdlib::storage::SequenceNew> Default for Vec<T, S> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> From<std::vec::Vec<T>> for Vec<T> {
        fn from(values: std::vec::Vec<T>) -> Self {
            Self::with_storage(values)
        }
    }

    impl<T, const N: usize> From<[T; N]> for Vec<T> {
        fn from(values: [T; N]) -> Self {
            Self::with_storage(std::vec::Vec::from(values))
        }
    }

    #[rils_impl]
    impl<T, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for Vec<T, S> {
        #[rils_associated(T)]
        type Item = S::Item;
        #[rils_associated(Iterator<T>)]
        type IntoIter = S::IntoIter;
        /// Consumes the Vec and creates an iterator.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceFromArray<T>> Vec<T, S> {
        /// Creates a Vec from an owned array.
        #[export_rils]
        #[rils_return(Self)]
        pub fn from<const N: usize>(
            #[rils_param([T; N])] values: S::Array<N>,
        ) -> <S::Array<N> as rils_stdlib::storage::ArrayInput>::Output<Self> {
            rils_stdlib::storage::ArrayInput::build(values, Self::with_storage)
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> Vec<T, S> {
        /// Creates an empty Vec.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<T, S> Vec<T, S> {
        /// Returns the owned storage to a native runtime adapter.
        pub fn into_storage(self) -> S {
            self.0
        }
    }
    impl<T> Vec<T> {
        /// Returns the default Rust storage.
        pub fn into_inner(self) -> std::vec::Vec<T> {
            self.0
        }
    }

    impl<T, S: rils_stdlib::storage::SequenceSearch<T>> Vec<T, S> {
        /// Returns true when an equal element is present.
        #[export_rils]
        #[rils_return(bool)]
        pub fn contains(&self, value: &T) -> S::SearchOutput {
            self.0.contains(value)
        }
    }
}

pub use native::Vec;

pub fn is_native_vec(ty: &rils_syntax::Type) -> bool {
    vec_layout::matches(ty)
}

#[path = "vector/format.rs"]
mod native_format;

pub const NATIVE_FORMAT_VEC: rils_native::FormatRegistration = rils_native::FormatRegistration {
    matches: |layout| is_native_vec(layout.rils_type()) && layout.sequence_item().is_some(),
    format: native_format::format,
};

#[path = "vector/equality.rs"]
mod native_equality;

pub const NATIVE_EQUALITY_VEC: rils_native::EqualityRegistration =
    rils_native::EqualityRegistration::view(
        |layout| is_native_vec(layout.rils_type()) && layout.sequence_item().is_some(),
        native_equality::equal,
    );
