//! Homogeneous owned collections with runtime-known element layouts.

use std::{
    alloc::Layout,
    cell::{Cell, RefCell},
    ptr,
    rc::Rc,
};

use rils_syntax::Type;

use super::{DropKind, DynamicLayout, DynamicValue};

/// Tracks Rils references independently of temporary Rust borrows of the
/// sequence payload. Clones of a dynamic object share this ledger.
pub struct SequenceBorrowLedger {
    references: RefCell<Vec<Rc<Cell<usize>>>>,
    iterators: Rc<Cell<usize>>,
}

/// One Rils reference to a sequence element. Dropping it never needs to
/// borrow the payload or the ledger's vector of slots.
pub struct SequenceItemLease(Rc<Cell<usize>>);

impl Drop for SequenceItemLease {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

/// An iterator keeps structure stable until it and its yielded references
/// have been dropped.
pub struct SequenceIteratorLease(Rc<Cell<usize>>);

impl Drop for SequenceIteratorLease {
    fn drop(&mut self) {
        self.0.set(self.0.get().saturating_sub(1));
    }
}

impl SequenceBorrowLedger {
    fn new(length: usize) -> Self {
        Self {
            references: RefCell::new((0..length).map(|_| Rc::new(Cell::new(0))).collect()),
            iterators: Rc::new(Cell::new(0)),
        }
    }

    pub fn reference(&self, index: usize) -> Result<SequenceItemLease, String> {
        let references = self
            .references
            .try_borrow()
            .map_err(|_| "sequence references are already accessed".to_owned())?;
        let count = references
            .get(index)
            .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
        count.set(
            count
                .get()
                .checked_add(1)
                .ok_or("too many sequence references")?,
        );
        Ok(SequenceItemLease(count.clone()))
    }

    pub fn begin_iteration(&self) -> Result<SequenceIteratorLease, String> {
        self.iterators.set(
            self.iterators
                .get()
                .checked_add(1)
                .ok_or("too many sequence iterators")?,
        );
        Ok(SequenceIteratorLease(self.iterators.clone()))
    }

    pub fn check_structural_mutation(&self) -> Result<(), String> {
        let references = self
            .references
            .try_borrow()
            .map_err(|_| "sequence references are already accessed".to_owned())?;
        if self.iterators.get() > 0 || references.iter().any(|count| count.get() > 0) {
            Err("cannot structurally mutate a sequence while an element is referenced".into())
        } else {
            Ok(())
        }
    }

    pub fn has_active(&self) -> bool {
        self.iterators.get() > 0
            || self.references.try_borrow().map_or(true, |references| {
                references.iter().any(|count| count.get() > 0)
            })
    }

    pub fn check_element_write(&self, index: usize) -> Result<(), String> {
        let references = self
            .references
            .try_borrow()
            .map_err(|_| "sequence references are already accessed".to_owned())?;
        if index >= references.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        if self.iterators.get() > 0 {
            return Err(
                "cannot mutate a sequence element while it is borrowed by an iterator".into(),
            );
        }
        Ok(())
    }

    pub fn check_element_replace(&self, index: usize) -> Result<(), String> {
        self.check_element_write(index)?;
        let references = self
            .references
            .try_borrow()
            .map_err(|_| "sequence references are already accessed".to_owned())?;
        if references[index].get() > 0 {
            return Err(format!(
                "cannot replace element {index} while it is referenced"
            ));
        }
        Ok(())
    }

    fn push(&self) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .push(Rc::new(Cell::new(0)));
        Ok(())
    }

    fn push_front(&self) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .insert(0, Rc::new(Cell::new(0)));
        Ok(())
    }

    fn insert(&self, index: usize) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .insert(index, Rc::new(Cell::new(0)));
        Ok(())
    }

    fn remove(&self, index: usize) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .remove(index);
        Ok(())
    }

    fn swap_remove(&self, index: usize) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .swap_remove(index);
        Ok(())
    }

    fn truncate(&self, length: usize) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .truncate(length);
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        self.references
            .try_borrow_mut()
            .map_err(|_| "sequence references are already accessed".to_owned())?
            .clear();
        Ok(())
    }
}

pub(super) struct SequenceStorage {
    pub(super) items: Vec<DynamicValue>,
    pub(super) borrows: Rc<SequenceBorrowLedger>,
}

impl DynamicLayout {
    /// Register a variable-length owned sequence. The header owns a Rust Vec
    /// and a reference ledger; each element uses its concrete item layout.
    /// The caller chooses the Rils collection type through its declaration.
    pub fn sequence(ty: Type, item: Rc<Self>) -> Rc<Self> {
        Rc::new(Self {
            rils_type: ty,
            layout: Layout::new::<SequenceStorage>(),
            copy: false,
            bitwise_copy: false,
            drop_kind: DropKind::Sequence { item },
        })
    }

    pub fn sequence_item(&self) -> Option<&Rc<Self>> {
        match &self.drop_kind {
            DropKind::Sequence { item } => Some(item),
            _ => None,
        }
    }
}

impl DynamicValue {
    /// Transfer each element into a checked native sequence.
    pub fn sequence(descriptor: Rc<DynamicLayout>, items: Vec<Self>) -> Result<Self, String> {
        let DropKind::Sequence { item } = &descriptor.drop_kind else {
            return Err("sequence constructor requires a sequence layout".into());
        };
        if items
            .iter()
            .any(|value| !item.compatible_with(&value.descriptor))
        {
            return Err("sequence item has a different layout".into());
        }
        let mut result = Self::uninitialized(descriptor);
        let storage = SequenceStorage {
            borrows: Rc::new(SequenceBorrowLedger::new(items.len())),
            items,
        };
        // SAFETY: sequence descriptors allocate exactly SequenceStorage.
        unsafe {
            ptr::write(
                result.storage.pointer_mut().cast::<SequenceStorage>(),
                storage,
            )
        };
        result.initialized = true;
        Ok(result)
    }

    fn sequence_storage(&self) -> Result<&SequenceStorage, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Sequence { .. }) {
            return Err("value is not a sequence".into());
        }
        // SAFETY: the constructor initialized exactly one SequenceStorage.
        Ok(unsafe { &*self.storage.pointer().cast::<SequenceStorage>() })
    }

    fn sequence_storage_mut(&mut self) -> Result<&mut SequenceStorage, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Sequence { .. }) {
            return Err("value is not a sequence".into());
        }
        // SAFETY: the constructor initialized exactly one SequenceStorage and the
        // exclusive parent borrow protects access to it.
        Ok(unsafe { &mut *self.storage.pointer_mut().cast::<SequenceStorage>() })
    }

    pub fn sequence_borrows(&self) -> Result<Rc<SequenceBorrowLedger>, String> {
        Ok(self.sequence_storage()?.borrows.clone())
    }

    pub fn sequence_len(&self) -> Result<usize, String> {
        Ok(self.sequence_storage()?.items.len())
    }

    pub fn with_sequence_item<R>(
        &self,
        index: usize,
        f: impl FnOnce(&Self) -> R,
    ) -> Result<R, String> {
        let item = self
            .sequence_storage()?
            .items
            .get(index)
            .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
        Ok(f(item))
    }

    /// Replace one item without moving the sequence or invalidating its
    /// reference handles. Each write takes only a short Rust borrow.
    pub fn replace_sequence_item(&mut self, index: usize, item: Self) -> Result<Self, String> {
        self.check_path_write(&[super::DynamicPathStep::Index(index)], true)?;
        let DropKind::Sequence { item: expected } = &self.descriptor.drop_kind else {
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

    pub fn push_sequence_item(&mut self, item: Self) -> Result<(), String> {
        let DropKind::Sequence { item: expected } = &self.descriptor.drop_kind else {
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

    pub fn push_sequence_front(&mut self, item: Self) -> Result<(), String> {
        let DropKind::Sequence { item: expected } = &self.descriptor.drop_kind else {
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

    pub fn insert_sequence_item(&mut self, index: usize, item: Self) -> Result<(), String> {
        let DropKind::Sequence { item: expected } = &self.descriptor.drop_kind else {
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

    pub fn take_all_sequence_items(&mut self) -> Result<Vec<Self>, String> {
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

    pub fn take_sequence_item(&mut self, index: usize) -> Result<Self, String> {
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

    pub fn swap_remove_sequence_item(&mut self, index: usize) -> Result<Self, String> {
        let storage = self.sequence_storage_mut()?;
        storage.borrows.check_structural_mutation()?;
        if index >= storage.items.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        storage.borrows.swap_remove(index)?;
        Ok(storage.items.swap_remove(index))
    }
}
