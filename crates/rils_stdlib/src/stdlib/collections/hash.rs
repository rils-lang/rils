//! Hash collections exported as Rils built-ins.

use rils_stdlib_macros::decl_rils;

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

    #[rils_impl]
    impl<T> IntoIterator for HashSet<T> {
        type Item = T;
        type IntoIter = Iterator<T>;
        /// Consumes the set and iterates over its values.
        #[rils_native_bridge]
        #[rils_legacy_id(core::hash_set::into_iter)]
        fn into_iter(self) -> Self::IntoIter {
            Iterator(self.0.into_iter().collect())
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
        #[rils_legacy_id(core::hash_set::len)]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns true when the set has no elements.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::is_empty)]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all elements.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::clear)]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Returns true when the value is present.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::contains)]
        pub fn contains(&self, value: &T) -> bool {
            self.0.contains(value)
        }
        /// Inserts a value and reports whether it was new.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::insert)]
        #[rils_native_bridge]
        pub fn insert(&mut self, value: T) -> bool {
            self.0.insert(value)
        }
        /// Removes a value and reports whether it was present.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::remove)]
        pub fn remove(&mut self, value: &T) -> bool {
            self.0.remove(value)
        }
        /// Returns true when every element is in the other set.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::is_subset)]
        pub fn is_subset(&self, other: &HashSet<T>) -> bool {
            self.0.is_subset(&other.0)
        }
        /// Returns true when the set contains every element of the other set.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::is_superset)]
        pub fn is_superset(&self, other: &HashSet<T>) -> bool {
            self.0.is_superset(&other.0)
        }
        /// Returns true when the sets share no elements.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::is_disjoint)]
        pub fn is_disjoint(&self, other: &HashSet<T>) -> bool {
            self.0.is_disjoint(&other.0)
        }
        /// Borrows each element without consuming the set.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::iter)]
        pub fn iter(&self) -> Iter<&T> {
            Iter::from(self.0.iter().collect::<std::vec::Vec<_>>())
        }
    }

    impl<T: Eq + Hash + Clone> HashSet<T> {
        /// Clones the union of two sets.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::union)]
        pub fn union(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.union(&other.0).cloned().collect())
        }
        /// Clones the intersection of two sets.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::intersection)]
        pub fn intersection(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.intersection(&other.0).cloned().collect())
        }
        /// Clones values that are not in the other set.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::difference)]
        pub fn difference(&self, other: &HashSet<T>) -> HashSet<T> {
            Self(self.0.difference(&other.0).cloned().collect())
        }
        /// Clones values present in exactly one set.
        #[export_rils]
        #[rils_legacy_id(core::hash_set::symmetric_difference)]
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

    #[rils_impl]
    impl<K, V> IntoIterator for HashMap<K, V> {
        type Item = (K, V);
        type IntoIter = Iterator<(K, V)>;
        /// Consumes the map and iterates over owned key-value pairs.
        #[rils_native_bridge]
        #[rils_legacy_id(core::hash_map::into_iter)]
        fn into_iter(self) -> Self::IntoIter {
            Iterator(self.0.into_iter().collect())
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
        #[rils_legacy_id(core::hash_map::len)]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns true when the map has no entries.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::is_empty)]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all entries.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::clear)]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Returns true when the key is present.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::contains_key)]
        pub fn contains_key(&self, key: &K) -> bool {
            self.0.contains_key(key)
        }
        /// Inserts a key-value pair and returns the previous value.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::insert)]
        #[rils_native_bridge]
        pub fn insert(&mut self, key: K, value: V) -> Option<V> {
            self.0.insert(key, value).map_or(Option::None, Option::Some)
        }
        /// Removes a key and returns its value.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::remove)]
        pub fn remove(&mut self, key: &K) -> Option<V> {
            self.0.remove(key).map_or(Option::None, Option::Some)
        }
        /// Borrows each key-value pair without consuming the map.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::iter)]
        pub fn iter(&self) -> Iter<(&K, &V)> {
            Iter::from(self.0.iter().collect::<std::vec::Vec<_>>())
        }
    }

    impl<K: Eq + Hash, V: Clone> HashMap<K, V> {
        /// Clones the value stored for a key.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::get_cloned)]
        pub fn get_cloned(&self, key: &K) -> Option<V> {
            self.0.get(key).cloned().map_or(Option::None, Option::Some)
        }
        /// Clones all values into an owned iterator.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::values_cloned)]
        pub fn values_cloned(&self) -> Iterator<V> {
            Iterator(self.0.values().cloned().collect())
        }
    }

    impl<K: Eq + Hash + Clone, V> HashMap<K, V> {
        /// Clones all keys into an owned iterator.
        #[export_rils]
        #[rils_legacy_id(core::hash_map::keys_cloned)]
        pub fn keys_cloned(&self) -> Iterator<K> {
            Iterator(self.0.keys().cloned().collect())
        }
    }
}

pub use native::{HashMap, HashSet};

mod hashset_layout {
    use rils_stdlib_macros::decl_rils_layout;
    hashset_definition!(decl_rils_layout);
}

pub(super) const NATIVE_LAYOUT_HASHSET: rils_native::LayoutRegistration =
    rils_native::LayoutRegistration {
        matches: hashset_layout::matches,
        layout: hashset_layout::layout,
    };

mod hashmap_layout {
    use rils_stdlib_macros::decl_rils_layout;
    hashmap_definition!(decl_rils_layout);
}

pub(super) const NATIVE_LAYOUT_HASHMAP: rils_native::LayoutRegistration =
    rils_native::LayoutRegistration {
        matches: hashmap_layout::matches,
        layout: hashmap_layout::layout,
    };
