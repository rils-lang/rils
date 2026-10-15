//! BTreeMap methods over a replaceable storage backend.
use rils_stdlib_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {
    /// An ordered map backed by a B-tree.
    #[rils_struct]
    pub struct BTreeMap<
        K,
        V,
        #[rils_storage(sequence((K, V)), rils_stdlib::stdlib::collections::BTreeMap)] S = std::collections::BTreeMap<K, V>,
    >(S, std::marker::PhantomData<(K, V)>);

    impl<K, V, S> BTreeMap<K, V, S> {
        /// Wraps storage without copying its entries.
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
        /// Returns the owned storage to its native caller.
        pub fn into_storage(self) -> S {
            self.0
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceNew> BTreeMap<K, V, S> {
        /// Creates an empty collection.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceNew> Default for BTreeMap<K, V, S> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceRead> BTreeMap<K, V, S> {
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
    impl<K, V, S: rils_stdlib::storage::SequenceClear> BTreeMap<K, V, S> {
        /// Removes all entries.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::ClearOutput {
            self.0.clear()
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceIter> BTreeMap<K, V, S> {
        /// Borrows each entry without consuming the collection.
        #[export_rils]
        #[rils_return(Iter<(&K, &V)>)]
        pub fn iter(&self) -> S::IterOutput<'_> {
            self.0.iter()
        }
    }
    #[rils_impl]
    impl<K, V, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for BTreeMap<K, V, S> {
        #[rils_associated((K, V))]
        type Item = S::Item;
        #[rils_associated(BTreeMapIntoIter<K, V>)]
        type IntoIter = S::IntoIter;
        /// Consumes the collection and yields owned entries.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapInsert<K, V>> BTreeMap<K, V, S> {
        /// Inserts an entry and returns the previous value.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn insert(&mut self, key: K, value: V) -> S::InsertOutput {
            self.0
                .insert(key, value, rils_stdlib::storage::KeyPolicy::Ordered)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapRemove<K>> BTreeMap<K, V, S> {
        /// Removes a key and returns its value.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn remove(&mut self, key: &K) -> S::RemoveOutput {
            self.0.remove(key, rils_stdlib::storage::KeyPolicy::Ordered)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapSearch<K>> BTreeMap<K, V, S> {
        /// Returns whether a key is present.
        #[export_rils]
        #[rils_return(bool)]
        pub fn contains_key(&self, key: &K) -> S::SearchOutput {
            self.0
                .contains_key(key, rils_stdlib::storage::KeyPolicy::Ordered)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapClone<K>> BTreeMap<K, V, S> {
        /// Explicitly clones the value for a key.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn get_cloned(&self, key: &K) -> S::CloneOutput {
            self.0
                .get_cloned(key, rils_stdlib::storage::KeyPolicy::Ordered)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapBoundary> BTreeMap<K, V, S> {
        /// Clones the smallest key.
        #[export_rils]
        #[rils_return(Option<K>)]
        pub fn first_key_cloned(&self) -> S::BoundaryOutput {
            self.0.first_key_cloned()
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapBoundary> BTreeMap<K, V, S> {
        /// Clones the largest key.
        #[export_rils]
        #[rils_return(Option<K>)]
        pub fn last_key_cloned(&self) -> S::BoundaryOutput {
            self.0.last_key_cloned()
        }
    }
    impl<K, V> std::ops::Deref for BTreeMap<K, V> {
        type Target = std::collections::BTreeMap<K, V>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<K, V> std::ops::DerefMut for BTreeMap<K, V> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    /// An owning iterator over the collection's entries.
    #[rils_struct]
    pub struct BTreeMapIntoIter<K, V>(std::collections::btree_map::IntoIter<K, V>);
    #[rils_impl]
    impl<K, V> std::iter::Iterator for BTreeMapIntoIter<K, V> {
        type Item = (K, V);
        /// Returns the next owned entry.
        fn next(&mut self) -> std::option::Option<(K, V)> {
            self.0.next()
        }
    }
    impl<K, V> From<std::collections::btree_map::IntoIter<K, V>> for BTreeMapIntoIter<K, V> {
        fn from(cursor: std::collections::btree_map::IntoIter<K, V>) -> Self {
            Self(cursor)
        }
    }
}
pub use native::{BTreeMap, BTreeMapIntoIter};
