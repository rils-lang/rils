//! Native collection operations.

use rils_builtins_macros::decl_rils;

use super::{prelude::Option, string::Iterator};

/// Borrowed iterator returned by collection views while the Rils iterator API is migrated.
pub struct Iter<T>(pub Vec<T>);

/// Values accepted by the Rils max-priority queue.
pub trait HeapElement: Ord + Clone {}

macro_rules! heap_elements {
    ($($ty:ty),* $(,)?) => {
        $(impl HeapElement for $ty {})*
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
    super::string::String
);

#[decl_rils(core::collections)]
mod native {
    use super::{HeapElement, Iter, Iterator, Option};

    /// A growable double-ended queue.
    #[rils_struct]
    pub struct VecDeque<T>(std::collections::VecDeque<T>);

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
            Self(values)
        }
    }

    impl<T> From<VecDeque<T>> for std::collections::VecDeque<T> {
        fn from(values: VecDeque<T>) -> Self {
            values.0
        }
    }

    impl<T> VecDeque<T> {
        pub fn clone_front_with<E>(
            values: &std::collections::VecDeque<T>,
            clone: impl FnOnce(&T) -> std::result::Result<T, E>,
        ) -> std::result::Result<Option<T>, E> {
            match values.front() {
                Some(value) => clone(value).map(Option::Some),
                None => Ok(Option::None),
            }
        }

        pub fn clone_back_with<E>(
            values: &std::collections::VecDeque<T>,
            clone: impl FnOnce(&T) -> std::result::Result<T, E>,
        ) -> std::result::Result<Option<T>, E> {
            match values.back() {
                Some(value) => clone(value).map(Option::Some),
                None => Ok(Option::None),
            }
        }

        /// Creates an empty queue.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::VecDeque::new())
        }

        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns whether the queue is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Adds an element at the front.
        #[export_rils]
        pub fn push_front(&mut self, value: T) {
            self.0.push_front(value);
        }

        /// Adds an element at the back.
        #[export_rils]
        pub fn push_back(&mut self, value: T) {
            self.0.push_back(value);
        }

        /// Removes the front element.
        #[export_rils]
        pub fn pop_front(&mut self) -> Option<T> {
            match self.0.pop_front() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Removes the back element.
        #[export_rils]
        pub fn pop_back(&mut self) -> Option<T> {
            match self.0.pop_back() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Clones the front element.
        #[export_rils]
        pub fn front_cloned(&self) -> Option<T>
        where
            T: Clone,
        {
            Self::clone_front_with(&self.0, |value| {
                Ok::<T, std::convert::Infallible>(value.clone())
            })
            .unwrap_or_else(|never| match never {})
        }

        /// Clones the back element.
        #[export_rils]
        pub fn back_cloned(&self) -> Option<T>
        where
            T: Clone,
        {
            Self::clone_back_with(&self.0, |value| {
                Ok::<T, std::convert::Infallible>(value.clone())
            })
            .unwrap_or_else(|never| match never {})
        }

        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl<T> std::default::Default for VecDeque<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// An owned max-priority queue. Elements must be orderable integers, char, or string.
    #[rils_struct]
    pub struct BinaryHeap<T>(std::collections::BinaryHeap<T>);

    impl<T: HeapElement> BinaryHeap<T> {
        /// Creates an empty max-priority queue.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::BinaryHeap::new())
        }

        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }

        /// Returns whether the queue is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }

        /// Inserts an element, rejecting unsupported ordering types.
        #[export_rils]
        pub fn push(&mut self, value: T) {
            self.0.push(value);
        }

        /// Removes and returns the greatest element.
        #[export_rils]
        pub fn pop(&mut self) -> Option<T> {
            match self.0.pop() {
                Some(value) => Option::Some(value),
                None => Option::None,
            }
        }

        /// Explicitly clones the highest-priority element.
        #[export_rils]
        pub fn peek_cloned(&self) -> Option<T> {
            match self.0.peek() {
                Some(value) => Option::Some(value.clone()),
                None => Option::None,
            }
        }

        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
    }

    impl<T: HeapElement> std::default::Default for BinaryHeap<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    /// An ordered set backed by a B-tree.
    #[rils_struct]
    pub struct BTreeSet<T>(std::collections::BTreeSet<T>);

    impl<T> std::ops::Deref for BTreeSet<T> {
        type Target = std::collections::BTreeSet<T>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for BTreeSet<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T: Ord> std::default::Default for BTreeSet<T> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<T> std::iter::IntoIterator for BTreeSet<T> {
        type Item = T;
        type IntoIter = std::collections::btree_set::IntoIter<T>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<T: Ord> BTreeSet<T> {
        /// Creates an empty set.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::BTreeSet::new())
        }
        /// Returns the number of elements.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns whether the set is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all elements.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Tests whether an element is present.
        #[export_rils]
        pub fn contains(&self, value: &T) -> bool {
            self.0.contains(value)
        }
        /// Inserts an element and reports whether it was new.
        #[export_rils]
        pub fn insert(&mut self, value: T) -> bool {
            self.0.insert(value)
        }
        /// Removes an element and reports whether it was present.
        #[export_rils]
        pub fn remove(&mut self, value: &T) -> bool {
            self.0.remove(value)
        }
        /// Tests whether every element is present in the other set.
        #[export_rils]
        pub fn is_subset(&self, other: &BTreeSet<T>) -> bool {
            self.0.is_subset(&other.0)
        }
        /// Tests whether this set contains every element of the other set.
        #[export_rils]
        pub fn is_superset(&self, other: &BTreeSet<T>) -> bool {
            self.0.is_superset(&other.0)
        }
        /// Tests whether the sets share no elements.
        #[export_rils]
        pub fn is_disjoint(&self, other: &BTreeSet<T>) -> bool {
            self.0.is_disjoint(&other.0)
        }
        /// Consumes the set and iterates over owned elements in order.
        #[export_rils]
        #[allow(clippy::should_implement_trait)]
        pub fn into_iter(self) -> Iterator<T> {
            Iterator(self.0.into_iter().collect())
        }
        /// Borrows each element in ascending order.
        #[export_rils]
        pub fn iter(&self) -> Iter<&T> {
            Iter(self.0.iter().collect())
        }
    }

    impl<T: Ord + Clone> BTreeSet<T> {
        /// Clones the smallest element.
        #[export_rils]
        pub fn first_cloned(&self) -> Option<T> {
            self.0.first().cloned().map_or(Option::None, Option::Some)
        }
        /// Clones the largest element.
        #[export_rils]
        pub fn last_cloned(&self) -> Option<T> {
            self.0.last().cloned().map_or(Option::None, Option::Some)
        }
        /// Clones the union into a new ordered set.
        #[export_rils]
        pub fn union(&self, other: &BTreeSet<T>) -> BTreeSet<T> {
            Self(self.0.union(&other.0).cloned().collect())
        }
        /// Clones the intersection into a new ordered set.
        #[export_rils]
        pub fn intersection(&self, other: &BTreeSet<T>) -> BTreeSet<T> {
            Self(self.0.intersection(&other.0).cloned().collect())
        }
        /// Clones elements absent from the other set.
        #[export_rils]
        pub fn difference(&self, other: &BTreeSet<T>) -> BTreeSet<T> {
            Self(self.0.difference(&other.0).cloned().collect())
        }
        /// Clones elements present in exactly one set.
        #[export_rils]
        pub fn symmetric_difference(&self, other: &BTreeSet<T>) -> BTreeSet<T> {
            Self(self.0.symmetric_difference(&other.0).cloned().collect())
        }
    }

    /// An ordered map backed by a B-tree.
    #[rils_struct]
    pub struct BTreeMap<K, V>(std::collections::BTreeMap<K, V>);

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

    impl<K: Ord, V> std::default::Default for BTreeMap<K, V> {
        fn default() -> Self {
            Self::new()
        }
    }

    impl<K, V> std::iter::IntoIterator for BTreeMap<K, V> {
        type Item = (K, V);
        type IntoIter = std::collections::btree_map::IntoIter<K, V>;
        fn into_iter(self) -> Self::IntoIter {
            self.0.into_iter()
        }
    }

    impl<K: Ord, V> BTreeMap<K, V> {
        /// Creates an empty map.
        #[export_rils]
        pub fn new() -> Self {
            Self(std::collections::BTreeMap::new())
        }
        /// Returns the number of entries.
        #[export_rils]
        pub fn len(&self) -> usize {
            self.0.len()
        }
        /// Returns whether the map is empty.
        #[export_rils]
        pub fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        /// Removes all entries.
        #[export_rils]
        pub fn clear(&mut self) {
            self.0.clear();
        }
        /// Tests whether a key is present.
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
        /// Consumes the map and iterates over owned entries in key order.
        #[export_rils]
        #[allow(clippy::should_implement_trait)]
        pub fn into_iter(self) -> Iterator<(K, V)> {
            Iterator(self.0.into_iter().collect())
        }
        /// Borrows each key-value pair in key order.
        #[export_rils]
        pub fn iter(&self) -> Iter<(&K, &V)> {
            Iter(self.0.iter().collect())
        }
    }

    impl<K: Ord, V: Clone> BTreeMap<K, V> {
        /// Clones the value for a key.
        #[export_rils]
        pub fn get_cloned(&self, key: &K) -> Option<V> {
            self.0.get(key).cloned().map_or(Option::None, Option::Some)
        }
    }

    impl<K: Ord + Clone, V> BTreeMap<K, V> {
        /// Clones the smallest key.
        #[export_rils]
        pub fn first_key_cloned(&self) -> Option<K> {
            self.0
                .first_key_value()
                .map(|(key, _)| key.clone())
                .map_or(Option::None, Option::Some)
        }
        /// Clones the largest key.
        #[export_rils]
        pub fn last_key_cloned(&self) -> Option<K> {
            self.0
                .last_key_value()
                .map(|(key, _)| key.clone())
                .map_or(Option::None, Option::Some)
        }
    }
}

pub use native::{BTreeMap, BTreeSet, BinaryHeap, VecDeque};
