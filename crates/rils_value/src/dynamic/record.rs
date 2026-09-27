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

    pub(super) fn layout_handle(&self) -> Rc<DynamicLayout> {
        self.layout.clone()
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
    pub(super) fn field(&self, index: usize) -> Result<&DynamicField, String> {
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
        Self::aggregate(rils_type, fields)
    }

    /// Compose a tuple or fixed array using the same movable, aligned slots as
    /// records. Each child keeps its own layout and destructor.
    pub fn aggregate(rils_type: Type, fields: Vec<(String, Rc<Self>)>) -> Result<Rc<Self>, String> {
        match &rils_type {
            Type::Named { .. } => {}
            Type::Tuple(elements) if elements.len() == fields.len() => {
                for (index, ((_, field), expected)) in fields.iter().zip(elements).enumerate() {
                    if field.rils_type() != expected {
                        return Err(format!("tuple element {index} has a different type"));
                    }
                }
            }
            Type::Array { element, length } if *length == fields.len() => {
                if fields
                    .iter()
                    .any(|(_, field)| field.rils_type() != element.as_ref())
                {
                    return Err("array element has a different type".into());
                }
            }
            _ => {
                return Err(
                    "aggregate layout requires matching record, tuple, or array fields".into(),
                );
            }
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

    fn field_path(&self, path: &[usize]) -> Result<(*const u8, Rc<DynamicLayout>), String> {
        if path.is_empty() {
            return Err("record field path is empty".into());
        }
        let mut pointer = self.storage.pointer();
        let mut layout = self.descriptor.clone();
        for &index in path {
            let DropKind::Record(record) = &layout.drop_kind else {
                return Err(format!("{} is not a record or aggregate", layout.rils_type));
            };
            let field = record.field(index)?;
            // SAFETY: every traversed pointer belongs to an initialized
            // aggregate. A live tag proves its child bytes are initialized.
            if unsafe { ptr::read(pointer.add(index)) } != 1 {
                return Err(format!("record field `{}` has been moved", field.name));
            }
            // SAFETY: Layout::extend established this aligned child offset.
            pointer = unsafe { pointer.add(field.offset) };
            layout = field.layout.clone();
        }
        Ok((pointer, layout))
    }

    fn field_path_parent(
        &self,
        path: &[usize],
    ) -> Result<(*const u8, usize, usize, Rc<DynamicLayout>), String> {
        let (&index, parents) = path
            .split_last()
            .ok_or_else(|| "record field path is empty".to_owned())?;
        let (pointer, layout) = if parents.is_empty() {
            (self.storage.pointer(), self.descriptor.clone())
        } else {
            self.field_path(parents)?
        };
        let DropKind::Record(record) = &layout.drop_kind else {
            return Err(format!("{} is not a record or aggregate", layout.rils_type));
        };
        let field = record.field(index)?;
        Ok((pointer, index, field.offset, field.layout.clone()))
    }

    /// Borrow a native leaf through any number of nested records, tuples or
    /// fixed arrays. The reference cannot outlive the callback.
    pub fn with_field_path<T: 'static, R>(
        &self,
        path: &[usize],
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        let (pointer, layout) = self.field_path(path)?;
        if !matches!(layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "record field is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: all path tags are live and the final descriptor checked T.
        Ok(f(unsafe { &*pointer.cast::<T>() }))
    }

    /// Mutably borrow a nested native leaf while holding the root exclusively.
    pub fn with_field_path_mut<T: 'static, R>(
        &mut self,
        path: &[usize],
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        let (pointer, layout) = self.field_path(path)?;
        if !matches!(layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!(
                "record field is not {}",
                std::any::type_name::<T>()
            ));
        }
        // SAFETY: all path tags are live, the descriptor checked T, and the
        // exclusive root borrow covers the entire callback.
        Ok(f(unsafe { &mut *pointer.cast_mut().cast::<T>() }))
    }

    /// Move one nested field while leaving every ancestor and sibling live.
    pub fn take_field_path(&mut self, path: &[usize]) -> Result<Self, String> {
        let (parent, index, offset, layout) = self.field_path_parent(path)?;
        // SAFETY: field_path_parent checked the index against the parent tags.
        if unsafe { ptr::read(parent.add(index)) } != 1 {
            return Err(format!("record field at path {path:?} has been moved"));
        }
        let mut child = Self::uninitialized(layout.clone());
        // SAFETY: the live child has the exact layout and aligned offset.
        // Clearing only its tag transfers its destructor to the new owner.
        unsafe {
            ptr::copy_nonoverlapping(
                parent.add(offset),
                child.storage.pointer_mut(),
                layout.layout.size(),
            );
            ptr::write(parent.cast_mut().add(index), 0);
        }
        child.initialized = true;
        Ok(child)
    }

    /// Restore one empty nested field using its registered concrete layout.
    pub fn put_field_path(&mut self, path: &[usize], mut value: Self) -> Result<(), String> {
        let (parent, index, offset, layout) = self.field_path_parent(path)?;
        if !Rc::ptr_eq(&layout, &value.descriptor) {
            return Err(format!(
                "record field at path {path:?} has a different layout"
            ));
        }
        // SAFETY: field_path_parent checked the index against the parent tags.
        if unsafe { ptr::read(parent.add(index)) } == 1 {
            return Err(format!(
                "record field at path {path:?} is already initialized"
            ));
        }
        // SAFETY: the empty slot has the exact child layout. Clearing the
        // source's live bit transfers ownership to the parent record.
        unsafe {
            ptr::copy_nonoverlapping(
                value.storage.pointer(),
                parent.cast_mut().add(offset),
                value.storage.size(),
            );
            value.initialized = false;
            ptr::write(parent.cast_mut().add(index), 1);
        }
        Ok(())
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
