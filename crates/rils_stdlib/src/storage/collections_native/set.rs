use super::{find, identity, insertion_index};
use crate::storage::{
    KeyPolicy, KeyQuery, MutableAccess, OwnedSequence, SetAlgebra, SetInsert, SetRelations,
    SetRemove, SetSearch, SharedAccess,
};
use rils_native::NativeKey;
use rils_value::DynamicValue;
use std::collections::{BTreeMap, BTreeSet};

impl SetInsert<DynamicValue> for MutableAccess<'_> {
    type InsertOutput = Result<bool, String>;
    fn insert(&mut self, value: DynamicValue, policy: KeyPolicy) -> Self::InsertOutput {
        let key = policy.identity(value.view(), self.services)?;
        let layout = self.view.view().layout()?;
        let entry = self.schema.build_entry(
            layout
                .sequence_item()
                .ok_or("collection requires sequence storage")?
                .clone(),
            vec![value],
        )?;
        if find(&self.view.view(), self.schema, &key, policy, self.services)?.is_some() {
            return Ok(false);
        }
        let index = insertion_index(&self.view.view(), self.schema, &key, policy, self.services)?;
        self.view.insert_sequence_item(index, entry)?;
        Ok(true)
    }
}

impl<Q: KeyQuery> SetRemove<Q> for MutableAccess<'_> {
    type RemoveOutput = Result<bool, String>;
    fn remove(&mut self, value: &Q, policy: KeyPolicy) -> Self::RemoveOutput {
        let layout = self
            .schema
            .parameter_layout(self.view.view().layout()?.as_ref(), 0)?;
        let key = value.identity(&layout, policy)?;
        if let Some(index) = find(&self.view.view(), self.schema, &key, policy, self.services)? {
            self.view.take_sequence_item(index)?;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl<Q: KeyQuery> SetSearch<Q> for SharedAccess<'_> {
    type SearchOutput = Result<bool, String>;
    fn contains(&self, value: &Q, policy: KeyPolicy) -> Self::SearchOutput {
        let layout = self
            .schema
            .parameter_layout(self.view.layout()?.as_ref(), 0)?;
        let key = value.identity(&layout, policy)?;
        find(&self.view, self.schema, &key, policy, self.services).map(|index| index.is_some())
    }
}

impl SetRelations for SharedAccess<'_> {
    type RelationOutput = Result<bool, String>;
    fn is_subset(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput {
        Ok(self.keys(policy)?.is_subset(&other.keys(policy)?))
    }
    fn is_superset(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput {
        Ok(self.keys(policy)?.is_superset(&other.keys(policy)?))
    }
    fn is_disjoint(&self, other: &Self, policy: KeyPolicy) -> Self::RelationOutput {
        Ok(self.keys(policy)?.is_disjoint(&other.keys(policy)?))
    }
}

impl SetAlgebra for SharedAccess<'_> {
    type Storage = OwnedSequence;
    type AlgebraOutput<R> = Result<R, String>;
    fn union<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        self.algebra(other, policy, |left, right| {
            left.union(right).cloned().collect()
        })
        .map(wrap)
    }
    fn intersection<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        self.algebra(other, policy, |left, right| {
            left.intersection(right).cloned().collect()
        })
        .map(wrap)
    }
    fn difference<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        self.algebra(other, policy, |left, right| {
            left.difference(right).cloned().collect()
        })
        .map(wrap)
    }
    fn symmetric_difference<R>(
        &self,
        other: &Self,
        policy: KeyPolicy,
        wrap: impl FnOnce(Self::Storage) -> R,
    ) -> Self::AlgebraOutput<R> {
        self.algebra(other, policy, |left, right| {
            left.symmetric_difference(right).cloned().collect()
        })
        .map(wrap)
    }
}

impl SharedAccess<'_> {
    fn indices(&self, policy: KeyPolicy) -> Result<BTreeMap<NativeKey, usize>, String> {
        (0..self.view.sequence_len()?)
            .map(|index| {
                identity(
                    self.view.sequence_item(index)?,
                    self.schema,
                    policy,
                    self.services,
                )
                .map(|key| (key, index))
            })
            .collect()
    }

    fn keys(&self, policy: KeyPolicy) -> Result<BTreeSet<NativeKey>, String> {
        self.indices(policy)
            .map(|indices| indices.into_keys().collect())
    }

    /// Select identities before cloning and preserve each operand's own services.
    fn algebra(
        &self,
        other: &Self,
        policy: KeyPolicy,
        select: impl FnOnce(&BTreeSet<NativeKey>, &BTreeSet<NativeKey>) -> BTreeSet<NativeKey>,
    ) -> Result<OwnedSequence, String> {
        if !self
            .view
            .layout()?
            .compatible_with(other.view.layout()?.as_ref())
        {
            return Err("set operands have different native layouts".into());
        }
        let left = self.indices(policy)?;
        let right = other.indices(policy)?;
        let selected = select(
            &left.keys().cloned().collect(),
            &right.keys().cloned().collect(),
        );
        selected
            .into_iter()
            .map(|key| {
                // Equal identities retain the right operand's value and declaration context.
                let (source, index) = if let Some(index) = right.get(&key) {
                    (other, *index)
                } else {
                    (self, left[&key])
                };
                source
                    .services
                    .clone_value(source.view.sequence_item(index)?)
            })
            .collect::<Result<Vec<_>, _>>()
            .map(OwnedSequence::from_elements)
    }
}
