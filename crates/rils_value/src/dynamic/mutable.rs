//! Mutable access to a checked native projection. The owner remains in place.

use super::{DropKind, DynamicPathStep, DynamicValue, DynamicValueRef, sequence::SequenceStorage};

/// A scoped mutable projection that cannot replace its layout or detach its owner.
pub struct DynamicValueMut<'a> {
    pub(super) root: &'a mut DynamicValue,
    pub(super) path: Vec<DynamicPathStep>,
}

impl DynamicValue {
    pub fn view_mut(&mut self) -> DynamicValueMut<'_> {
        DynamicValueMut {
            root: self,
            path: Vec::new(),
        }
    }

    pub fn view_path_mut(
        &mut self,
        path: &[DynamicPathStep],
    ) -> Result<DynamicValueMut<'_>, String> {
        self.project(path)?;
        Ok(DynamicValueMut {
            root: self,
            path: path.to_vec(),
        })
    }
}

impl DynamicValueMut<'_> {
    /// Replace this projection with the same layout, transferring the previous
    /// payload. Root references retain their borrow ledger across replacement.
    pub fn replace_value(&mut self, value: DynamicValue) -> Result<DynamicValue, String> {
        value.ensure_initialized()?;
        if value.has_path_references() {
            return Err("cannot transfer a referenced native replacement".into());
        }
        if !self.view().layout()?.compatible_with(value.descriptor()) {
            return Err("native replacement has a different layout".into());
        }
        if !self.path.is_empty() {
            return self
                .root
                .replace_path_reference(&self.path, value)?
                .ok_or("native projection was moved".into());
        }
        self.root.check_path_write(&[], true)?;
        let ledger = self.root.path_borrows.take();
        let previous = std::mem::replace(self.root, value);
        self.root.path_borrows = ledger.map(std::cell::OnceCell::from).unwrap_or_default();
        Ok(previous)
    }

    pub fn view(&self) -> DynamicValueRef<'_> {
        DynamicValueRef {
            root: self.root,
            path: self.path.clone(),
        }
    }

    pub fn sequence_len(&self) -> Result<usize, String> {
        self.view().sequence_len()
    }

    pub fn sequence_borrows(&self) -> Result<std::rc::Rc<super::SequenceBorrowLedger>, String> {
        self.view().sequence_borrows()
    }

    fn sequence_storage_mut(&mut self) -> Result<&mut SequenceStorage, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        if !matches!(layout.drop_kind, DropKind::Sequence { .. }) {
            return Err("value is not a sequence".into());
        }
        // SAFETY: project validated every ancestor and the sequence header.
        // This view retains the exclusive root borrow for the returned lifetime.
        Ok(unsafe { &mut *pointer.cast_mut().cast::<SequenceStorage>() })
    }
    /// Replace one item without moving the sequence or invalidating its
    /// reference handles. Each write takes only a short Rust borrow.
    pub fn replace_sequence_item(
        &mut self,
        index: usize,
        item: DynamicValue,
    ) -> Result<DynamicValue, String> {
        item.ensure_initialized()?;
        let mut path = self.path.clone();
        path.push(super::DynamicPathStep::Index(index));
        self.root.check_path_write(&path, true)?;
        let layout = self.view().layout()?;
        let DropKind::Sequence { item: expected } = &layout.drop_kind else {
            return Err("value is not a sequence".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("sequence item has a different layout".into());
        }
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_element_write(index)?;
        let slot = storage
            .items
            .get_mut(index)
            .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
        Ok(std::mem::replace(slot, item))
    }

    pub fn push_sequence_item(&mut self, item: DynamicValue) -> Result<(), String> {
        item.ensure_initialized()?;
        let layout = self.view().layout()?;
        let DropKind::Sequence { item: expected } = &layout.drop_kind else {
            return Err("value is not a sequence".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("sequence item has a different layout".into());
        }
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        storage.borrows.push()?;
        storage.items.push(item);
        Ok(())
    }

    pub fn push_sequence_front(&mut self, item: DynamicValue) -> Result<(), String> {
        item.ensure_initialized()?;
        let layout = self.view().layout()?;
        let DropKind::Sequence { item: expected } = &layout.drop_kind else {
            return Err("value is not a sequence".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("sequence item has a different layout".into());
        }
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        storage.borrows.push_front()?;
        storage.items.insert(0, item);
        Ok(())
    }

    pub fn insert_sequence_item(&mut self, index: usize, item: DynamicValue) -> Result<(), String> {
        item.ensure_initialized()?;
        let layout = self.view().layout()?;
        let DropKind::Sequence { item: expected } = &layout.drop_kind else {
            return Err("value is not a sequence".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("sequence item has a different layout".into());
        }
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        if index > storage.items.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        storage.borrows.insert(index)?;
        storage.items.insert(index, item);
        Ok(())
    }

    pub fn truncate_sequence(&mut self, length: usize) -> Result<(), String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        storage.borrows.truncate(length)?;
        storage.items.truncate(length);
        Ok(())
    }

    pub fn take_all_sequence_items(&mut self) -> Result<Vec<DynamicValue>, String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        storage.borrows.clear()?;
        Ok(std::mem::take(&mut storage.items))
    }

    pub fn clear_sequence(&mut self) -> Result<(), String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        storage.borrows.clear()?;
        storage.items.clear();
        Ok(())
    }

    pub fn swap_sequence_items(&mut self, left: usize, right: usize) -> Result<(), String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        let items = &mut storage.items;
        if left >= items.len() || right >= items.len() {
            return Err("sequence swap index is out of bounds".into());
        }
        items.swap(left, right);
        Ok(())
    }

    pub fn take_sequence_item(&mut self, index: usize) -> Result<DynamicValue, String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        let items = &mut storage.items;
        if index >= items.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        storage.borrows.remove(index)?;
        let item = items.remove(index);
        Ok(item)
    }

    pub fn swap_remove_sequence_item(&mut self, index: usize) -> Result<DynamicValue, String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        if index >= storage.items.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        storage.borrows.swap_remove(index)?;
        Ok(storage.items.swap_remove(index))
    }
}
