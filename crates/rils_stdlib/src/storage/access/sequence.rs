//! Existing sequence capabilities share the same checked call access as collections.
use super::{MutableAccess, SharedAccess};
use crate::storage::{
    ElementQuery, SequenceClone, SequenceDeque, SequenceExtend, SequenceHeap, SequenceSearch,
    SequenceTransfer, SequenceTruncate,
};
use rils_value::DynamicValue;

impl<Q: ElementQuery> SequenceSearch<Q> for SharedAccess<'_> {
    type SearchOutput = Result<bool, String>;
    fn contains(&self, value: &Q) -> Self::SearchOutput {
        self.view.indexed()?.contains(value)
    }
}

impl SequenceClone for SharedAccess<'_> {
    type CloneOutput = Result<Option<DynamicValue>, String>;
    fn front_cloned(&self) -> Self::CloneOutput {
        self.clone_boundary(false)
    }
    fn back_cloned(&self) -> Self::CloneOutput {
        self.clone_boundary(true)
    }
}
impl SharedAccess<'_> {
    fn clone_boundary(&self, back: bool) -> Result<Option<DynamicValue>, String> {
        let view = self.view.indexed()?;
        if view.is_empty() {
            return Ok(None);
        }
        self.services
            .clone_value(view.item(if back { view.len() - 1 } else { 0 })?)
            .map(Some)
    }
}

impl SequenceTruncate for MutableAccess<'_> {
    type TruncateOutput = Result<(), String>;
    fn truncate(&mut self, length: usize) -> Self::TruncateOutput {
        self.view.truncate(length)
    }
}

impl SequenceTransfer<DynamicValue> for MutableAccess<'_> {
    type WriteOutput = Result<(), String>;
    type TakeOutput = Result<DynamicValue, String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push(&mut self, value: DynamicValue) -> Self::WriteOutput {
        SequenceTransfer::push(&mut self.view, value)
    }
    fn insert(&mut self, index: usize, value: DynamicValue) -> Self::WriteOutput {
        SequenceTransfer::insert(&mut self.view, index, value)
    }
    fn pop(&mut self) -> Self::PopOutput {
        SequenceTransfer::pop(&mut self.view)
    }
    fn remove(&mut self, index: usize) -> Self::TakeOutput {
        SequenceTransfer::remove(&mut self.view, index)
    }
    fn swap_remove(&mut self, index: usize) -> Self::TakeOutput {
        SequenceTransfer::swap_remove(&mut self.view, index)
    }
}

impl SequenceDeque<DynamicValue> for MutableAccess<'_> {
    type WriteOutput = Result<(), String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push_front(&mut self, value: DynamicValue) -> Self::WriteOutput {
        self.view.push_front(value)
    }
    fn push_back(&mut self, value: DynamicValue) -> Self::WriteOutput {
        self.view.push_back(value)
    }
    fn pop_front(&mut self) -> Self::PopOutput {
        self.view.pop_front()
    }
    fn pop_back(&mut self) -> Self::PopOutput {
        self.view.pop_back()
    }
}

impl SequenceHeap<DynamicValue> for MutableAccess<'_> {
    type WriteOutput = Result<(), String>;
    type PopOutput = Result<Option<DynamicValue>, String>;
    fn push(&mut self, value: DynamicValue) -> Self::WriteOutput {
        SequenceHeap::push(&mut self.view, value)
    }
    fn pop(&mut self) -> Self::PopOutput {
        SequenceHeap::pop(&mut self.view)
    }
}

impl SequenceExtend for MutableAccess<'_> {
    type ExtendOutput = Result<(), String>;
    fn extend(&mut self, source: Self) -> Self::ExtendOutput {
        self.view.extend(source.view)
    }
}
