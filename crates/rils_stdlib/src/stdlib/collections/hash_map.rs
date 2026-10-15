//! HashMap methods over a replaceable storage backend.
use rils_stdlib_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {
    /// An owned hash map.
    #[rils_struct]
    pub struct HashMap<
        K,
        V,
        #[rils_storage(sequence((K, V)), rils_stdlib::stdlib::collections::HashMap)] S = std::collections::HashMap<K, V>,
    >(S, std::marker::PhantomData<(K, V)>);

    impl<K, V, S> HashMap<K, V, S> {
        /// Wraps storage without copying its entries.
        pub fn with_storage(storage: S) -> Self {
            Self(storage, std::marker::PhantomData)
        }
        /// Returns the owned storage to its native caller.
        pub fn into_storage(self) -> S {
            self.0
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceNew> HashMap<K, V, S> {
        /// Creates an empty collection.
        #[export_rils]
        pub fn new() -> Self {
            Self::with_storage(S::new())
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceNew> Default for HashMap<K, V, S> {
        fn default() -> Self {
            Self::new()
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceRead> HashMap<K, V, S> {
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
    impl<K, V, S: rils_stdlib::storage::SequenceClear> HashMap<K, V, S> {
        /// Removes all entries.
        #[export_rils]
        #[rils_return(())]
        pub fn clear(&mut self) -> S::ClearOutput {
            self.0.clear()
        }
    }
    impl<K, V, S: rils_stdlib::storage::SequenceIter> HashMap<K, V, S> {
        /// Borrows each entry without consuming the collection.
        #[export_rils]
        #[rils_return(Iter<(&K, &V)>)]
        pub fn iter(&self) -> S::IterOutput<'_> {
            self.0.iter()
        }
    }
    #[rils_impl]
    impl<K, V, S: rils_stdlib::storage::SequenceIntoIter> IntoIterator for HashMap<K, V, S> {
        #[rils_associated((K, V))]
        type Item = S::Item;
        #[rils_associated(HashMapIntoIter<K, V>)]
        type IntoIter = S::IntoIter;
        /// Consumes the collection and yields owned entries.
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapInsert<K, V>> HashMap<K, V, S> {
        /// Inserts an entry and returns the previous value.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn insert(&mut self, key: K, value: V) -> S::InsertOutput {
            self.0
                .insert(key, value, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapRemove<K>> HashMap<K, V, S> {
        /// Removes a key and returns its value.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn remove(&mut self, key: &K) -> S::RemoveOutput {
            self.0.remove(key, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapSearch<K>> HashMap<K, V, S> {
        /// Returns whether a key is present.
        #[export_rils]
        #[rils_return(bool)]
        pub fn contains_key(&self, key: &K) -> S::SearchOutput {
            self.0
                .contains_key(key, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapClone<K>> HashMap<K, V, S> {
        /// Explicitly clones the value for a key.
        #[export_rils]
        #[rils_return(Option<V>)]
        pub fn get_cloned(&self, key: &K) -> S::CloneOutput {
            self.0
                .get_cloned(key, rils_stdlib::storage::KeyPolicy::Hash)
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapKeys> HashMap<K, V, S> {
        /// Clones all keys into an owned iterator.
        #[export_rils]
        #[rils_return(Iterator<K>)]
        pub fn keys_cloned(&self) -> S::KeysOutput {
            self.0.keys_cloned()
        }
    }
    impl<K, V, S: rils_stdlib::storage::MapValues> HashMap<K, V, S> {
        /// Clones all values into an owned iterator.
        #[export_rils]
        #[rils_return(Iterator<V>)]
        pub fn values_cloned(&self) -> S::ValuesOutput {
            self.0.values_cloned()
        }
    }
    impl<K, V> std::ops::Deref for HashMap<K, V> {
        type Target = std::collections::HashMap<K, V>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<K, V> std::ops::DerefMut for HashMap<K, V> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    /// An owning iterator over the collection's entries.
    #[rils_struct]
    pub struct HashMapIntoIter<K, V>(std::collections::hash_map::IntoIter<K, V>);
    #[rils_impl]
    impl<K, V> std::iter::Iterator for HashMapIntoIter<K, V> {
        type Item = (K, V);
        /// Returns the next owned entry.
        fn next(&mut self) -> std::option::Option<(K, V)> {
            self.0.next()
        }
    }
    impl<K, V> From<std::collections::hash_map::IntoIter<K, V>> for HashMapIntoIter<K, V> {
        fn from(cursor: std::collections::hash_map::IntoIter<K, V>) -> Self {
            Self(cursor)
        }
    }
}
pub use native::{HashMap, HashMapIntoIter};
