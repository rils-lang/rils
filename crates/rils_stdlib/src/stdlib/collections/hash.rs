//! Hash collections exported as Rils built-ins.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::collections)]
mod native {
    use super::super::{Iter, Iterator, Option};
    use std::hash::Hash;

    /// An owned hash set.
    #[rils_struct]
    pub struct HashSet<T>(std::collections::HashSet<T>);

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

    impl<T: Eq + Hash> Default for HashSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> IntoIterator for HashSet<T> {
        type Item = T;
        type IntoIter = std::collections::hash_set::IntoIter<T>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<T: Eq + Hash> HashSet<T> {
        /// Creates an empty HashSet.
        #[export_rils]
        #[rils_import(core::hash_set::new)]
        pub fn new() -> Self {
            Self(std::collections::HashSet::new())
        }
        /// Returns the element count.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns true when the set has no elements.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Returns true when the value is present.
        #[export_rils]
        pub fn contains(&self, value: &T) -> bool {
            self.0.contains(value)
        }
        /// Inserts a value and reports whether it was new.
        #[export_rils]
        pub fn insert(&mut self, value: T) -> bool {
            self.0.insert(value)
        }
        /// Removes a value and reports whether it was present.
        #[export_rils]
        pub fn remove(&mut self, value: &T) -> bool {
            self.0.remove(value)
        }
        /// Returns true when every element is in the other set.
        #[export_rils]
        pub fn is_subset(&self, other: &HashSet<T>) -> bool {
            self.0.is_subset(&other.0)
        }
        /// Returns true when the set contains every element of the other set.
        #[export_rils]
        pub fn is_superset(&self, other: &HashSet<T>) -> bool {
            self.0.is_superset(&other.0)
        }
        /// Returns true when the sets share no elements.
        #[export_rils]
        pub fn is_disjoint(&self, other: &HashSet<T>) -> bool {
            self.0.is_disjoint(&other.0)
        }
        /// Consumes the set and iterates over its values.
        #[export_rils]
        #[allow(clippy::should_implement_trait)]
        pub fn into_iter(self) -> Iterator<T> {
            Iterator(self.0.into_iter().collect())
        }
        /// Borrows each element without consuming the set.
        #[export_rils]
        pub fn iter(&self) -> Iter<&T> {
            Iter::from(self.0.iter().collect::<std::vec::Vec<_>>())
        }
    }

    impl<T: Eq + Hash + Clone> HashSet<T> {
        /// Clones the union of two sets.
        #[export_rils]
        pub fn union(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.union(&other.0).cloned().collect())
        }
        /// Clones the intersection of two sets.
        #[export_rils]
        pub fn intersection(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.intersection(&other.0).cloned().collect())
        }
        /// Clones values that are not in the other set.
        #[export_rils]
        pub fn difference(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.difference(&other.0).cloned().collect())
        }
        /// Clones values present in exactly one set.
        #[export_rils]
        pub fn symmetric_difference(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.symmetric_difference(&other.0).cloned().collect())
        }
    }

    /// An owned hash map.
    #[rils_struct]
    pub struct HashMap<K, V>(std::collections::HashMap<K, V>);

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

    impl<K: Eq + Hash, V> Default for HashMap<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<K, V> IntoIterator for HashMap<K, V> {
        type Item = (K, V);
        type IntoIter = std::collections::hash_map::IntoIter<K, V>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<K: Eq + Hash, V> HashMap<K, V> {
        /// Creates an empty HashMap.
        #[export_rils]
        #[rils_import(core::hash_map::new)]
        pub fn new() -> Self {
            Self(std::collections::HashMap::new())
        }
        /// Returns the entry count.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns true when the map has no entries.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all entries.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Returns true when the key is present.
        #[export_rils]
        pub fn contains_key(&self, key: &K) -> bool {
            self.0.contains_key(key)
        }
        /// Inserts a key-value pair and returns the previous value.
        #[export_rils]
        pub fn insert(&mut self, key: K, value: V) -> Option<V> {
            self.0.insert(key, value).map_or(Option::None, Option::Some)
        }
        /// Removes a key and returns its value.
        #[export_rils]
        pub fn remove(&mut self, key: &K) -> Option<V> {
            self.0.remove(key).map_or(Option::None, Option::Some)
        }
        /// Consumes the map and iterates over owned key-value pairs.
        #[export_rils]
        #[allow(clippy::should_implement_trait)]
        pub fn into_iter(self) -> Iterator<(K, V)> {
            Iterator(self.0.into_iter().collect())
        }
        /// Borrows each key-value pair without consuming the map.
        #[export_rils]
        pub fn iter(&self) -> Iter<(&K, &V)> {
            Iter::from(self.0.iter().collect::<std::vec::Vec<_>>())
        }
    }

    impl<K: Eq + Hash, V: Clone> HashMap<K, V> {
        /// Clones the value stored for a key.
        #[export_rils]
        pub fn get_cloned(&self, key: &K) -> Option<V> {
            self.0.get(key).cloned().map_or(Option::None, Option::Some)
        }
        /// Clones all values into an owned iterator.
        #[export_rils]
        pub fn values_cloned(&self) -> Iterator<V> {
            Iterator(self.0.values().cloned().collect())
        }
    }

    impl<K: Eq + Hash + Clone, V> HashMap<K, V> {
        /// Clones all keys into an owned iterator.
        #[export_rils]
        pub fn keys_cloned(&self) -> Iterator<K> {
            Iterator(self.0.keys().cloned().collect())
        }
    }
}

pub use native::{HashMap, HashSet};
