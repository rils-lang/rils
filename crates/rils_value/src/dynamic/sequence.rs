//! Homogeneous owned collections with runtime-known element layouts.

use std::{alloc::Layout, ptr, rc::Rc};

use rils_syntax::Type;

use super::{DropKind, DynamicLayout, DynamicValue};

impl DynamicLayout {
    /// Register a variable-length owned sequence. The header uses Rust's Vec
    /// layout; each element owns bytes described by the concrete item layout.
    /// The caller chooses the Rils collection type through its declaration.
    pub fn sequence(ty: Type, item: Rc<Self>) -> Rc<Self> {
        Rc::new(Self {
            rils_type: ty,
            layout: Layout::new::<Vec<DynamicValue>>(),
            copy: false,
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
        // SAFETY: sequence descriptors allocate exactly Vec<DynamicValue>.
        unsafe { ptr::write(result.storage.pointer_mut().cast::<Vec<Self>>(), items) };
        result.initialized = true;
        Ok(result)
    }

    fn sequence_items(&self) -> Result<&Vec<Self>, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Sequence { .. }) {
            return Err("value is not a sequence".into());
        }
        // SAFETY: the constructor initialized exactly one Vec<Self>.
        Ok(unsafe { &*self.storage.pointer().cast::<Vec<Self>>() })
    }

    fn sequence_items_mut(&mut self) -> Result<&mut Vec<Self>, String> {
        if !matches!(self.descriptor.drop_kind, DropKind::Sequence { .. }) {
            return Err("value is not a sequence".into());
        }
        // SAFETY: the constructor initialized exactly one Vec<Self> and the
        // exclusive parent borrow protects access to it.
        Ok(unsafe { &mut *self.storage.pointer_mut().cast::<Vec<Self>>() })
    }

    pub fn sequence_len(&self) -> Result<usize, String> {
        Ok(self.sequence_items()?.len())
    }

    pub fn with_sequence_item<R>(
        &self,
        index: usize,
        f: impl FnOnce(&Self) -> R,
    ) -> Result<R, String> {
        let item = self
            .sequence_items()?
            .get(index)
            .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
        Ok(f(item))
    }

    pub fn push_sequence_item(&mut self, item: Self) -> Result<(), String> {
        let DropKind::Sequence { item: expected } = &self.descriptor.drop_kind else {
            return Err("value is not a sequence".into());
        };
        if !expected.compatible_with(&item.descriptor) {
            return Err("sequence item has a different layout".into());
        }
        self.sequence_items_mut()?.push(item);
        Ok(())
    }

    pub fn take_sequence_item(&mut self, index: usize) -> Result<Self, String> {
        let items = self.sequence_items_mut()?;
        if index >= items.len() {
            return Err(format!("sequence index {index} is out of bounds"));
        }
        Ok(items.remove(index))
    }
}
