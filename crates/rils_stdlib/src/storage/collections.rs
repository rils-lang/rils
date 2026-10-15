//! Collection capabilities selected by standard-library method bodies.
use super::KeyPolicy;

pub trait SetInsert<T> {
    type InsertOutput;
    fn insert(&mut self, value: T, policy: KeyPolicy) -> Self::InsertOutput;
}

pub trait SetRemove<Q> {
    type RemoveOutput;
    fn remove(&mut self, value: &Q, policy: KeyPolicy) -> Self::RemoveOutput;
}

pub trait SetSearch<Q> {
    type SearchOutput;
    fn contains(&self, value: &Q, policy: KeyPolicy) -> Self::SearchOutput;
}

pub trait SetRelations {
    type RelationOutput;
    fn is_subset(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput;
    fn is_superset(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput;
    fn is_disjoint(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput;
}

pub trait SetAlgebra {
    type Storage;
    type AlgebraOutput<R>;
    fn union<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R>;
    fn intersection<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R>;
    fn difference<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R>;
    fn symmetric_difference<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R>;
}

pub trait MapInsert<K, V> {
    type InsertOutput;
    fn insert(&mut self, key: K, value: V, policy: KeyPolicy) -> Self::InsertOutput;
}

pub trait MapRemove<Q> {
    type RemoveOutput;
    fn remove(&mut self, key: &Q, policy: KeyPolicy) -> Self::RemoveOutput;
}

pub trait MapSearch<Q> {
    type SearchOutput;
    fn contains_key(&self, key: &Q, policy: KeyPolicy) -> Self::SearchOutput;
}

pub trait MapClone<Q> {
    type CloneOutput;
    fn get_cloned(&self, key: &Q, policy: KeyPolicy) -> Self::CloneOutput;
}

pub trait MapKeys {
    type KeysOutput;
    fn keys_cloned(&self) -> Self::KeysOutput;
}

pub trait MapBoundary {
    type BoundaryOutput;
    fn first_key_cloned(&self) -> Self::BoundaryOutput;
    fn last_key_cloned(&self) -> Self::BoundaryOutput;
}

pub trait MapValues {
    type ValuesOutput;
    fn values_cloned(&self) -> Self::ValuesOutput;
}
