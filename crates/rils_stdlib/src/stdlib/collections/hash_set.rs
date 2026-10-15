//! HashSet methods over a replaceable storage backend.
use rils_stdlib_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {
    /// An owned hash set.
    #[rils_struct]
    pub struct HashSet<
        T,
        #[rils_storage(sequence, rils_stdlib::stdlib::collections::HashSet)] S = std::collections::HashSet<T>,
    >(S, std::marker::PhantomData<T>);

    impl<T, S> HashSet<T, S> {
        /// Wraps storage without copying its entries.
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
        /// Returns the owned storage to its native caller.
        pub fn into_storage(self) -> S {
            self.0
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> HashSet<T, S> {
        /// Creates an empty collection.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceNew> Default for HashSet<T, S> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceRead> HashSet<T, S> {
        /// Returns the number of entries.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns whether the collection is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.len() == 0
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceClear> HashSet<T, S> {
        /// Removes all entries.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::ClearOutput {
            self.0.clear()
        }
    }
    impl<T, S: rils_stdlib::storage::SequenceIter> HashSet<T, S> {
        /// Borrows each entry without consuming the collection.
        #[export_rils]
        #[rils_return(Iter<&T>)]
        pub fn iter(&self) -> S::IterOutput<'_> {
            self.0.iter()
        }
    }
    #[rils_impl]
    impl<T, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for HashSet<T, S> {
        #[rils_associated(T)]
        type Item = S::Item;
        #[rils_associated(HashSetIntoIter<T>)]
        type IntoIter = S::IntoIter;
        /// Consumes the collection and yields owned entries.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
    impl<T, S: rils_stdlib::storage::SetInsert<T>> HashSet<T, S> {
        /// Inserts a value and reports whether it was new.
        #[export_rils]
        #[rils_return(bool)]
        pub fn insert(&mut self, value: T) -> S::InsertOutput {
            self.0.insert(value, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetRemove<T>> HashSet<T, S> {
        /// Removes a value and reports whether it was present.
        #[export_rils]
        #[rils_return(bool)]
        pub fn remove(&mut self, value: &T) -> S::RemoveOutput {
            self.0.remove(value, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetSearch<T>> HashSet<T, S> {
        /// Returns whether a value is present.
        #[export_rils]
        #[rils_return(bool)]
        pub fn contains(&self, value: &T) -> S::SearchOutput {
            self.0
                .contains(value, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetRelations> HashSet<T, S> {
        /// Returns whether every element is present in the other set.
        #[export_rils]
        #[rils_return(bool)]
        pub fn is_subset(&self, other: &Self) -> S::RelationOutput {
            self.0
                .is_subset(&other.0, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetRelations> HashSet<T, S> {
        /// Returns whether this set contains every element of the other set.
        #[export_rils]
        #[rils_return(bool)]
        pub fn is_superset(&self, other: &Self) -> S::RelationOutput {
            self.0
                .is_superset(&other.0, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetRelations> HashSet<T, S> {
        /// Returns whether the sets share no elements.
        #[export_rils]
        #[rils_return(bool)]
        pub fn is_disjoint(&self, other: &Self) -> S::RelationOutput {
            self.0
                .is_disjoint(&other.0, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<T, S: rils_stdlib::storage::SetAlgebra> HashSet<T, S> {
        /// Clones the union into a new set.
        #[export_rils]
        #[rils_return(Self)]
        pub fn union(&self, other: &Self) -> S::AlgebraOutput<HashSet<T, S::Storage>> {
            self.0.union(
                &other.0,
                rils_stdlib::storage::KeyPolicy::Hash,
                HashSet::with_storage,
            )
        }
    }
    impl<T, S: rils_stdlib::storage::SetAlgebra> HashSet<T, S> {
        /// Clones the intersection into a new set.
        #[export_rils]
        #[rils_return(Self)]
        pub fn intersection(&self, other: &Self) -> S::AlgebraOutput<HashSet<T, S::Storage>> {
            self.0.intersection(
                &other.0,
                rils_stdlib::storage::KeyPolicy::Hash,
                HashSet::with_storage,
            )
        }
    }
    impl<T, S: rils_stdlib::storage::SetAlgebra> HashSet<T, S> {
        /// Clones elements absent from the other set.
        #[export_rils]
        #[rils_return(Self)]
        pub fn difference(&self, other: &Self) -> S::AlgebraOutput<HashSet<T, S::Storage>> {
            self.0.difference(
                &other.0,
                rils_stdlib::storage::KeyPolicy::Hash,
                HashSet::with_storage,
            )
        }
    }
    impl<T, S: rils_stdlib::storage::SetAlgebra> HashSet<T, S> {
        /// Clones elements present in exactly one set.
        #[export_rils]
        #[rils_return(Self)]
        pub fn symmetric_difference(
            &self,
            other: &Self,
        ) -> S::AlgebraOutput<HashSet<T, S::Storage>> {
            self.0.symmetric_difference(
                &other.0,
                rils_stdlib::storage::KeyPolicy::Hash,
                HashSet::with_storage,
            )
        }
    }
    impl<T> std::ops::Deref for HashSet<T> {
        type Target = std::collections::HashSet<T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<T> std::ops::DerefMut for HashSet<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    /// An owning iterator over the collection's entries.
    #[rils_struct]
    pub struct HashSetIntoIter<T>(std::collections::hash_set::IntoIter<T>);
    #[rils_impl]
    impl<T> std::iter::Iterator for HashSetIntoIter<T> {
        type Item = T;
        /// Returns the next owned entry.
        fn next(&mut self) -> std::option::Option<T> {
            self.0.next()
        }
    }
    impl<T> From<std::collections::hash_set::IntoIter<T>> for HashSetIntoIter<T> {
        fn from(cursor: std::collections::hash_set::IntoIter<T>) -> Self {
            Self(cursor)
        }
    }
}
pub use native::{HashSet, HashSetIntoIter};
