//! Rust storage implementations of standard-library collection capabilities.
use super::*;
use crate::stdlib::collections::{
    BTreeMapIntoIter, BTreeSetIntoIter, HashMapIntoIter, HashSetIntoIter,
};
use crate::stdlib::iterator::Iter;
use crate::stdlib::option::Option as RilsOption;
use crate::storage::{
    SequenceClear, SequenceClone, SequenceIntoIter, SequenceIter, SequenceNew, SequenceRead,
};
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::hash::Hash;

fn option<T>(value: Option<T>) -> RilsOption<T> {
    value.map_or(RilsOption::None, RilsOption::Some)
}

impl<T> SequenceNew for HashSet<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T> SequenceRead for HashSet<T> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<T> SequenceClear for HashSet<T> {
    type ClearOutput = ();
    fn clear(&mut self) {
        self.clear();
    }
}
impl<T> SequenceIter for HashSet<T> {
    type IterOutput<'a>
        = Iter<&'a T>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        Iter::from(self.iter().collect::<Vec<_>>())
    }
}
impl<T> SequenceIntoIter for HashSet<T> {
    type Item = T;
    type IntoIter = HashSetIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}

impl<T: Eq + Hash> SetInsert<T> for HashSet<T> {
    type InsertOutput = bool;
    fn insert(&mut self, value: T, _: KeyPolicy) -> bool {
        self.insert(value)
    }
}
impl<T: Eq + Hash> SetRemove<T> for HashSet<T> {
    type RemoveOutput = bool;
    fn remove(&mut self, value: &T, _: KeyPolicy) -> bool {
        self.remove(value)
    }
}
impl<T: Eq + Hash> SetSearch<T> for HashSet<T> {
    type SearchOutput = bool;
    fn contains(&self, value: &T, _: KeyPolicy) -> bool {
        self.contains(value)
    }
}
impl<T: Eq + Hash> SetRelations for HashSet<T> {
    type RelationOutput = bool;
    fn is_subset(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_subset(other)
    }
    fn is_superset(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_superset(other)
    }
    fn is_disjoint(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_disjoint(other)
    }
}
impl<T: Eq + Hash + Clone> SetAlgebra for HashSet<T> {
    type Storage = Self;
    type AlgebraOutput<R> = R;
    fn union<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.union(other).cloned().collect())
    }
    fn intersection<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.intersection(other).cloned().collect())
    }
    fn difference<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.difference(other).cloned().collect())
    }
    fn symmetric_difference<R>(
        &self,
        other: &Self,
        _: KeyPolicy,
        wrap: impl FnOnce(Self) -> R,
    ) -> R {
        wrap(self.symmetric_difference(other).cloned().collect())
    }
}

impl<T> SequenceNew for BTreeSet<T> {
    fn new() -> Self {
        Self::new()
    }
}
impl<T> SequenceRead for BTreeSet<T> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<T> SequenceClear for BTreeSet<T> {
    type ClearOutput = ();
    fn clear(&mut self) {
        self.clear();
    }
}
impl<T> SequenceIter for BTreeSet<T> {
    type IterOutput<'a>
        = Iter<&'a T>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        Iter::from(self.iter().collect::<Vec<_>>())
    }
}
impl<T> SequenceIntoIter for BTreeSet<T> {
    type Item = T;
    type IntoIter = BTreeSetIntoIter<T>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}

impl<T: Ord> SetInsert<T> for BTreeSet<T> {
    type InsertOutput = bool;
    fn insert(&mut self, value: T, _: KeyPolicy) -> bool {
        self.insert(value)
    }
}
impl<T: Ord> SetRemove<T> for BTreeSet<T> {
    type RemoveOutput = bool;
    fn remove(&mut self, value: &T, _: KeyPolicy) -> bool {
        self.remove(value)
    }
}
impl<T: Ord> SetSearch<T> for BTreeSet<T> {
    type SearchOutput = bool;
    fn contains(&self, value: &T, _: KeyPolicy) -> bool {
        self.contains(value)
    }
}
impl<T: Ord> SetRelations for BTreeSet<T> {
    type RelationOutput = bool;
    fn is_subset(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_subset(other)
    }
    fn is_superset(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_superset(other)
    }
    fn is_disjoint(&self, other: &Self, _: KeyPolicy) -> bool {
        self.is_disjoint(other)
    }
}
impl<T: Ord + Clone> SetAlgebra for BTreeSet<T> {
    type Storage = Self;
    type AlgebraOutput<R> = R;
    fn union<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.union(other).cloned().collect())
    }
    fn intersection<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.intersection(other).cloned().collect())
    }
    fn difference<R>(&self, other: &Self, _: KeyPolicy, wrap: impl FnOnce(Self) -> R) -> R {
        wrap(self.difference(other).cloned().collect())
    }
    fn symmetric_difference<R>(
        &self,
        other: &Self,
        _: KeyPolicy,
        wrap: impl FnOnce(Self) -> R,
    ) -> R {
        wrap(self.symmetric_difference(other).cloned().collect())
    }
}

impl<T: Ord + Clone> SequenceClone for BTreeSet<T> {
    type CloneOutput = RilsOption<T>;
    fn front_cloned(&self) -> Self::CloneOutput {
        option(self.first().cloned())
    }
    fn back_cloned(&self) -> Self::CloneOutput {
        option(self.last().cloned())
    }
}

impl<K, V> SequenceNew for HashMap<K, V> {
    fn new() -> Self {
        Self::new()
    }
}
impl<K, V> SequenceRead for HashMap<K, V> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<K, V> SequenceClear for HashMap<K, V> {
    type ClearOutput = ();
    fn clear(&mut self) {
        self.clear();
    }
}
impl<K, V> SequenceIter for HashMap<K, V> {
    type IterOutput<'a>
        = Iter<(&'a K, &'a V)>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        Iter::from(self.iter().collect::<Vec<_>>())
    }
}
impl<K, V> SequenceIntoIter for HashMap<K, V> {
    type Item = (K, V);
    type IntoIter = HashMapIntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}

impl<K: Eq + Hash, V> MapInsert<K, V> for HashMap<K, V> {
    type InsertOutput = RilsOption<V>;
    fn insert(&mut self, key: K, value: V, _: KeyPolicy) -> Self::InsertOutput {
        option(self.insert(key, value))
    }
}
impl<K: Eq + Hash, V> MapRemove<K> for HashMap<K, V> {
    type RemoveOutput = RilsOption<V>;
    fn remove(&mut self, key: &K, _: KeyPolicy) -> Self::RemoveOutput {
        option(self.remove(key))
    }
}
impl<K: Eq + Hash, V> MapSearch<K> for HashMap<K, V> {
    type SearchOutput = bool;
    fn contains_key(&self, key: &K, _: KeyPolicy) -> bool {
        self.contains_key(key)
    }
}
impl<K: Eq + Hash, V: Clone> MapClone<K> for HashMap<K, V> {
    type CloneOutput = RilsOption<V>;
    fn get_cloned(&self, key: &K, _: KeyPolicy) -> Self::CloneOutput {
        option(self.get(key).cloned())
    }
}

impl<K: Eq + Hash + Clone, V> MapKeys for HashMap<K, V> {
    type KeysOutput = Iter<K>;
    fn keys_cloned(&self) -> Self::KeysOutput {
        Iter::from(self.keys().cloned().collect::<Vec<_>>())
    }
}
impl<K: Eq + Hash, V: Clone> MapValues for HashMap<K, V> {
    type ValuesOutput = Iter<V>;
    fn values_cloned(&self) -> Self::ValuesOutput {
        Iter::from(self.values().cloned().collect::<Vec<_>>())
    }
}

impl<K, V> SequenceNew for BTreeMap<K, V> {
    fn new() -> Self {
        Self::new()
    }
}
impl<K, V> SequenceRead for BTreeMap<K, V> {
    fn len(&self) -> usize {
        self.len()
    }
}
impl<K, V> SequenceClear for BTreeMap<K, V> {
    type ClearOutput = ();
    fn clear(&mut self) {
        self.clear();
    }
}
impl<K, V> SequenceIter for BTreeMap<K, V> {
    type IterOutput<'a>
        = Iter<(&'a K, &'a V)>
    where
        Self: 'a;
    fn iter(&self) -> Self::IterOutput<'_> {
        Iter::from(self.iter().collect::<Vec<_>>())
    }
}
impl<K, V> SequenceIntoIter for BTreeMap<K, V> {
    type Item = (K, V);
    type IntoIter = BTreeMapIntoIter<K, V>;
    fn into_iter(self) -> Self::IntoIter {
        IntoIterator::into_iter(self).into()
    }
}

impl<K: Ord, V> MapInsert<K, V> for BTreeMap<K, V> {
    type InsertOutput = RilsOption<V>;
    fn insert(&mut self, key: K, value: V, _: KeyPolicy) -> Self::InsertOutput {
        option(self.insert(key, value))
    }
}
impl<K: Ord, V> MapRemove<K> for BTreeMap<K, V> {
    type RemoveOutput = RilsOption<V>;
    fn remove(&mut self, key: &K, _: KeyPolicy) -> Self::RemoveOutput {
        option(self.remove(key))
    }
}
impl<K: Ord, V> MapSearch<K> for BTreeMap<K, V> {
    type SearchOutput = bool;
    fn contains_key(&self, key: &K, _: KeyPolicy) -> bool {
        self.contains_key(key)
    }
}
impl<K: Ord, V: Clone> MapClone<K> for BTreeMap<K, V> {
    type CloneOutput = RilsOption<V>;
    fn get_cloned(&self, key: &K, _: KeyPolicy) -> Self::CloneOutput {
        option(self.get(key).cloned())
    }
}

impl<K: Ord + Clone, V> MapBoundary for BTreeMap<K, V> {
    type BoundaryOutput = RilsOption<K>;
    fn first_key_cloned(&self) -> Self::BoundaryOutput {
        option(self.first_key_value().map(|(key, _)| key.clone()))
    }
    fn last_key_cloned(&self) -> Self::BoundaryOutput {
        option(self.last_key_value().map(|(key, _)| key.clone()))
    }
}
