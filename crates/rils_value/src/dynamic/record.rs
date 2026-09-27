//! Composed layouts for user-defined records with individually movable fields.

use std::{alloc::Layout, any::TypeId, collections::HashMap, ptr, rc::Rc};

use rils_syntax::Type;

use super::{DropKind, DynamicLayout, DynamicValue};

/// A field's declaration-order position and aligned offset within a record.
pub struct DynamicField {
    name: String,
    layout: Rc<DynamicLayout>,
    offset: usize,
}

impl DynamicField {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn layout(&self) -> &DynamicLayout {
        &self.layout
    }

    pub fn offset(&self) -> usize {
        self.offset
    }
}

pub(super) struct RecordLayout {
    fields: Vec<DynamicField>,
    indices: HashMap<String, usize>,
}

impl RecordLayout {
    fn field(&self, index: usize) -> Result<&DynamicField, String> {
        self.fields
            .get(index)
            .ok_or_else(|| format!("record field index {index} is out of bounds"))
    }

    pub(super) unsafe fn drop_value(&self, pointer: *mut u8) {
        for (index, field) in self.fields.iter().enumerate().rev() {
            // SAFETY: record constructors initialize the tag prefix before
            // exposing the value, and a live tag owns an initialized field.
            if unsafe { ptr::read(pointer.add(index)) } == 1 {
                // SAFETY: Layout::extend established the field's alignment.
                unsafe { field.layout.drop_value(pointer.add(field.offset)) };
            }
        }
    }
}

impl DynamicLayout {
    /// Compose a concrete record layout after all generic arguments are known.
    /// Field tags live in the allocation so nested moves preserve their state.
    pub fn record(rils_type: Type, fields: Vec<(String, Rc<Self>)>) -> Result<Rc<Self>, String> {
        if !matches!(rils_type, Type::Named { .. }) {
            return Err("record layout requires a named Rils type".into());
        }
        let mut layout = Layout::array::<u8>(fields.len())
            .map_err(|_| "record field tags exceed the address space".to_owned())?;
        let mut copy = true;
        let mut metadata = Vec::with_capacity(fields.len());
        let mut indices = HashMap::with_capacity(fields.len());
        for (name, field_layout) in fields {
            if indices.insert(name.clone(), metadata.len()).is_some() {
                return Err(format!("duplicate record field `{name}`"));
            }
            let (extended, offset) = layout
                .extend(field_layout.layout)
                .map_err(|_| format!("record field `{name}` exceeds the address space"))?;
            layout = extended;
            copy &= field_layout.copy;
            metadata.push(DynamicField {
                name,
                layout: field_layout,
                offset,
            });
        }
        Ok(Rc::new(Self {
            rils_type,
            layout: layout.pad_to_align(),
            copy,
            drop_kind: DropKind::Record(RecordLayout {
                fields: metadata,
                indices,
            }),
        }))
    }

    pub fn record_fields(&self) -> Option<&[DynamicField]> {
        let DropKind::Record(record) = &self.drop_kind else {
            return None;
        };
        Some(&record.fields)
    }

    pub fn record_field_index(&self, name: &str) -> Option<usize> {
        let DropKind::Record(record) = &self.drop_kind else {
            return None;
        };
        record.indices.get(name).copied()
    }
}

impl DynamicValue {
    /// Move fields into declaration-order slots of a composed record.
    pub fn record(descriptor: Rc<DynamicLayout>, mut values: Vec<Self>) -> Result<Self, String> {
        let DropKind::Record(record) = &descriptor.drop_kind else {
            return Err("record constructor requires a record layout".into());
        };
        if values.len() != record.fields.len() {
            return Err(format!(
                "record requires {} fields, found {}",
                record.fields.len(),
                values.len()
            ));
        }
        for (index, (field, value)) in record.fields.iter().zip(&values).enumerate() {
            if !Rc::ptr_eq(&field.layout, &value.descriptor) {
                return Err(format!(
                    "record field `{}` at index {index} has a different layout",
                    field.name
                ));
            }
        }
        let mut result = Self::uninitialized(descriptor.clone());
        // SAFETY: every field tag is inside the prefix allocated by record().
        unsafe { ptr::write_bytes(result.storage.pointer_mut(), 0, values.len()) };
        result.initialized = true;
        for (index, (field, value)) in record.fields.iter().zip(&mut values).enumerate() {
            // SAFETY: the destination has the exact checked child layout.
            // Copying transfers ownership; clearing the old live bit prevents
            // its destructor from running when the source storage is released.
            unsafe {
                ptr::copy_nonoverlapping(
                    value.storage.pointer(),
                    result.storage.pointer_mut().add(field.offset),
                    value.storage.size(),
                );
                value.initialized = false;
                ptr::write(result.storage.pointer_mut().add(index), 1);
            }
        }
        Ok(result)
    }

    fn record_field(&self, index: usize) -> Result<&DynamicField, String> {
        let DropKind::Record(record) = &self.descriptor.drop_kind else {
            return Err("value is not a record".into());
        };
        record.field(index)
    }

    pub fn field_is_live(&self, index: usize) -> Result<bool, String> {
        self.record_field(index)?;
        // SAFETY: record_field checked the index against the tag prefix.
        Ok(unsafe { ptr::read(self.storage.pointer().add(index)) == 1 })
    }

    /// Borrow a native leaf field after checking its registered Rust type.
    pub fn with_field<T: 'static, R>(
        &self,
        index: usize,
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        let field = self.record_field(index)?;
        if !self.field_is_live(index)? {
            return Err(format!("record field `{}` has been moved", field.name));
        }
        if !matches!(field.layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "record field `{}` is not {}",
                field.name,
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: the live field has the checked Rust layout and an aligned offset.
        Ok(f(unsafe {
            &*self.storage.pointer().add(field.offset).cast::<T>()
        }))
    }

    /// Mutably access one native leaf field through an exclusive record borrow.
    pub fn with_field_mut<T: 'static, R>(
        &mut self,
        index: usize,
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        let field = self.record_field(index)?;
        if !self.field_is_live(index)? {
            return Err(format!("record field `{}` has been moved", field.name));
        }
        if !matches!(field.layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "record field `{}` is not {}",
                field.name,
                std::any::type_name::<T>()
            ));
        }
        let offset = field.offset;
        // SAFETY: the live field has the checked Rust layout and &mut self
        // prevents another access for the duration of this callback.
        Ok(f(unsafe {
            &mut *self.storage.pointer_mut().add(offset).cast::<T>()
        }))
    }

    /// Move a field out, leaving its tag empty for a later write.
    pub fn take_field(&mut self, index: usize) -> Result<Self, String> {
        let field = self.record_field(index)?;
        if !self.field_is_live(index)? {
            return Err(format!("record field `{}` has been moved", field.name));
        }
        let mut child = Self::uninitialized(field.layout.clone());
        // SAFETY: the live tag owns a value with this field layout. Clearing
        // it transfers the destructor to the returned child.
        unsafe {
            ptr::copy_nonoverlapping(
                self.storage.pointer().add(field.offset),
                child.storage.pointer_mut(),
                field.layout.layout.size(),
            );
            ptr::write(self.storage.pointer_mut().add(index), 0);
        }
        child.initialized = true;
        Ok(child)
    }

    /// Fill an empty field with a value of its exact registered layout.
    pub fn put_field(&mut self, index: usize, mut value: Self) -> Result<(), String> {
        let field = self.record_field(index)?;
        if !Rc::ptr_eq(&field.layout, &value.descriptor) {
            return Err(format!(
                "record field `{}` has a different layout",
                field.name
            ));
        }
        if self.field_is_live(index)? {
            return Err(format!(
                "record field `{}` is already initialized",
                field.name
            ));
        }
        let offset = field.offset;
        // SAFETY: the empty slot has the exact checked child layout. The
        // source's ownership transfers when its initialized bit is cleared.
        unsafe {
            ptr::copy_nonoverlapping(
                value.storage.pointer(),
                self.storage.pointer_mut().add(offset),
                value.storage.size(),
            );
            value.initialized = false;
            ptr::write(self.storage.pointer_mut().add(index), 1);
        }
        Ok(())
    }
}
