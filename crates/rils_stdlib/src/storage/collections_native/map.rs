use super::{find, insertion_index};
use crate::storage::{
    KeyPolicy, KeyQuery, MapBoundary, MapClone, MapInsert, MapKeys, MapRemove, MapSearch,
    MapValues, MutableAccess, SharedAccess,
};
use rils_value::DynamicValue;

impl MapInsert<DynamicValue, DynamicValue> for MutableAccess<'_> {
    type InsertOutput = Result<Option<DynamicValue>, String>;
    fn insert(
        &mut self,
        key: DynamicValue,
        value: DynamicValue,
        policy: KeyPolicy,
    ) -> Self::InsertOutput {
        let identity = policy.identity(key.view(), self.services)?;
        let layout = self.view.view().layout()?;
        let entry = self.schema.build_entry(
            layout
                .sequence_item()
                .ok_or("collection requires sequence storage")?
                .clone(),
            vec![key, value],
        )?;
        self.view.sequence_borrows()?.check_structural_mutation()?;
        if let Some(index) = find(
            &self.view.view(),
            self.schema,
            &identity,
            policy,
            self.services,
        )? {
            let previous = self.view.replace_sequence_item(index, entry)?;
            self.schema.take(previous, 1).map(Some)
        } else {
            let index = insertion_index(
                &self.view.view(),
                self.schema,
                &identity,
                policy,
                self.services,
            )?;
            self.view.insert_sequence_item(index, entry)?;
            Ok(None)
        }
    }
}

impl<Q: KeyQuery> MapRemove<Q> for MutableAccess<'_> {
    type RemoveOutput = Result<Option<DynamicValue>, String>;
    fn remove(&mut self, key: &Q, policy: KeyPolicy) -> Self::RemoveOutput {
        let layout = self
            .schema
            .parameter_layout(self.view.view().layout()?.as_ref(), 0)?;
        let key = key.identity(&layout, policy)?;
        find(&self.view.view(), self.schema, &key, policy, self.services)?
            .map(|index| {
                let entry = self.view.take_sequence_item(index)?;
                self.schema.take(entry, 1)
            })
            .transpose()
    }
}

impl<Q: KeyQuery> MapSearch<Q> for SharedAccess<'_> {
    type SearchOutput = Result<bool, String>;
    fn contains_key(&self, key: &Q, policy: KeyPolicy) -> Self::SearchOutput {
        let layout = self
            .schema
            .parameter_layout(self.view.layout()?.as_ref(), 0)?;
        let key = key.identity(&layout, policy)?;
        find(&self.view, self.schema, &key, policy, self.services).map(|index| index.is_some())
    }
}

impl<Q: KeyQuery> MapClone<Q> for SharedAccess<'_> {
    type CloneOutput = Result<Option<DynamicValue>, String>;
    fn get_cloned(&self, key: &Q, policy: KeyPolicy) -> Self::CloneOutput {
        let layout = self
            .schema
            .parameter_layout(self.view.layout()?.as_ref(), 0)?;
        let key = key.identity(&layout, policy)?;
        find(&self.view, self.schema, &key, policy, self.services)?
            .map(|index| self.clone_parameter(index, 1))
            .transpose()
    }
}

impl MapKeys for SharedAccess<'_> {
    type KeysOutput = Result<Vec<DynamicValue>, String>;
    fn keys_cloned(&self) -> Self::KeysOutput {
        self.clone_parameters(0)
    }
}

impl MapValues for SharedAccess<'_> {
    type ValuesOutput = Result<Vec<DynamicValue>, String>;
    fn values_cloned(&self) -> Self::ValuesOutput {
        self.clone_parameters(1)
    }
}

impl MapBoundary for SharedAccess<'_> {
    type BoundaryOutput = Result<Option<DynamicValue>, String>;
    fn first_key_cloned(&self) -> Self::BoundaryOutput {
        self.clone_parameter_boundary(0, false)
    }
    fn last_key_cloned(&self) -> Self::BoundaryOutput {
        self.clone_parameter_boundary(0, true)
    }
}

impl SharedAccess<'_> {
    fn clone_parameter(&self, index: usize, parameter: usize) -> Result<DynamicValue, String> {
        self.services.clone_value(
            self.schema
                .project(self.view.sequence_item(index)?, parameter)?,
        )
    }

    fn clone_parameters(&self, parameter: usize) -> Result<Vec<DynamicValue>, String> {
        (0..self.view.sequence_len()?)
            .map(|index| self.clone_parameter(index, parameter))
            .collect()
    }

    fn clone_parameter_boundary(
        &self,
        parameter: usize,
        last: bool,
    ) -> Result<Option<DynamicValue>, String> {
        let len = self.view.sequence_len()?;
        if len == 0 {
            return Ok(None);
        }
        self.clone_parameter(if last { len - 1 } else { 0 }, parameter)
            .map(Some)
    }
}
