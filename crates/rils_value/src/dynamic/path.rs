//! Checked access to leaves inside heterogeneous native compositions.

use std::{any::TypeId, ptr, rc::Rc};

use super::{DropKind, DynamicLayout, DynamicValue, sequence::SequenceStorage};

/// One checked projection into a runtime-composed native value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicPathStep {
    Field(usize),
    Some,
    Variant(usize),
    Index(usize),
}

/// A borrowed view of any native layout, including composed generic values.
/// It retains the owning value's borrow and never materializes an interpreter value.
pub struct DynamicValueRef<'a> {
    root: &'a DynamicValue,
    path: Vec<DynamicPathStep>,
}

impl<'a> DynamicValueRef<'a> {
    pub fn layout(&self) -> Result<Rc<DynamicLayout>, String> {
        self.root.project(&self.path).map(|(_, layout)| layout)
    }

    pub fn with_rust<T: 'static, R>(&self, callback: impl FnOnce(&T) -> R) -> Result<R, String> {
        self.root.with_path(&self.path, callback)
    }

    pub fn copy_owned(&self) -> Result<DynamicValue, String> {
        self.root.copy_path(&self.path)
    }

    pub fn project(&self, step: DynamicPathStep) -> Result<Self, String> {
        let mut path = self.path.clone();
        path.push(step);
        self.root.project(&path)?;
        Ok(Self {
            root: self.root,
            path,
        })
    }

    pub fn field(&self, index: usize) -> Result<Self, String> {
        self.project(DynamicPathStep::Field(index))
    }

    pub fn option_item(&self) -> Result<Self, String> {
        self.project(DynamicPathStep::Some)
    }

    pub fn variant_payload(&self) -> Result<Self, String> {
        self.project(DynamicPathStep::Variant(self.variant_index()?))
    }

    pub fn sequence_item(&self, index: usize) -> Result<Self, String> {
        self.project(DynamicPathStep::Index(index))
    }

    pub fn option_is_some(&self) -> Result<bool, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        if !matches!(layout.drop_kind, DropKind::Option { .. }) {
            return Err(format!("{} is not optional", layout.rils_type()));
        }
        // SAFETY: a constructed option always initializes its tag byte.
        Ok(unsafe { ptr::read(pointer) == 1 })
    }

    pub fn variant_index(&self) -> Result<usize, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        let DropKind::Variant(variant) = &layout.drop_kind else {
            return Err(format!("{} is not a variant", layout.rils_type()));
        };
        // SAFETY: a constructed variant initializes a u32 tag.
        let index = unsafe { ptr::read(pointer.cast::<u32>()) } as usize;
        if index >= variant.alternatives.len() {
            return Err(format!("variant index {index} is out of bounds"));
        }
        Ok(index)
    }

    pub fn sequence_len(&self) -> Result<usize, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        if !matches!(layout.drop_kind, DropKind::Sequence { .. }) {
            return Err(format!("{} is not a sequence", layout.rils_type()));
        }
        // SAFETY: a constructed sequence initializes exactly one SequenceStorage.
        Ok(unsafe { &*pointer.cast::<SequenceStorage>() }.items.len())
    }
}

impl DynamicValue {
    pub fn view(&self) -> DynamicValueRef<'_> {
        DynamicValueRef {
            root: self,
            path: Vec::new(),
        }
    }
    fn project(&self, path: &[DynamicPathStep]) -> Result<(*const u8, Rc<DynamicLayout>), String> {
        let mut pointer = self.storage.pointer();
        let mut layout = self.descriptor.clone();
        for step in path {
            match (step, &layout.drop_kind) {
                (DynamicPathStep::Field(index), DropKind::Record(record)) => {
                    let field = record.field(*index)?;
                    // SAFETY: the field index is in the initialized tag prefix.
                    if unsafe { ptr::read(pointer.add(*index)) } != 1 {
                        return Err(format!("record field `{}` has been moved", field.name()));
                    }
                    // SAFETY: the checked field offset is aligned for its child.
                    pointer = unsafe { pointer.add(field.offset()) };
                    layout = field.layout_handle();
                }
                (DynamicPathStep::Some, DropKind::Option { item, item_offset }) => {
                    // SAFETY: every initialized option has a tag at byte zero.
                    if unsafe { ptr::read(pointer) } != 1 {
                        return Err("optional value is None".into());
                    }
                    // SAFETY: Layout::extend aligned the child offset.
                    pointer = unsafe { pointer.add(*item_offset) };
                    layout = item.clone();
                }
                (DynamicPathStep::Variant(index), DropKind::Variant(variant)) => {
                    let child = variant
                        .alternatives
                        .get(*index)
                        .ok_or_else(|| format!("variant index {index} is out of bounds"))?;
                    // SAFETY: every initialized variant has a u32 tag at byte zero.
                    if unsafe { ptr::read(pointer.cast::<u32>()) } as usize != *index {
                        return Err(format!("variant {index} is not active"));
                    }
                    // SAFETY: the payload offset was aligned for every variant.
                    pointer = unsafe { pointer.add(variant.payload_offset) };
                    layout = child.clone();
                }
                (DynamicPathStep::Index(index), DropKind::Sequence { item }) => {
                    // SAFETY: this descriptor initializes a SequenceStorage
                    // and the owner is borrowed for the entire callback.
                    let storage = unsafe { &*pointer.cast::<SequenceStorage>() };
                    let child = storage
                        .items
                        .get(*index)
                        .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
                    debug_assert!(item.compatible_with(&child.descriptor));
                    pointer = child.storage.pointer();
                    layout = item.clone();
                }
                _ => return Err(format!("cannot apply {step:?} to {}", layout.rils_type)),
            }
        }
        Ok((pointer, layout))
    }

    /// Borrow a typed leaf through records, options, variants and sequences.
    /// The returned reference is confined to the callback.
    pub fn with_path<T: 'static, R>(
        &self,
        path: &[DynamicPathStep],
        f: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        let (pointer, layout) = self.project(path)?;
        if !matches!(layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!("native leaf is not {}", std::any::type_name::<T>()));
        }
        // SAFETY: every projection checked its tag and the leaf descriptor
        // checked the exact Rust type before the reference was constructed.
        Ok(f(unsafe { &*pointer.cast::<T>() }))
    }

    /// Mutably borrow a typed nested leaf under an exclusive root borrow.
    pub fn with_path_mut<T: 'static, R>(
        &mut self,
        path: &[DynamicPathStep],
        f: impl FnOnce(&mut T) -> R,
    ) -> Result<R, String> {
        let (pointer, layout) = self.project(path)?;
        if !matches!(layout.drop_kind, DropKind::Rust { type_id, .. } if type_id == TypeId::of::<T>())
        {
            return Err(format!("native leaf is not {}", std::any::type_name::<T>()));
        }
        // SAFETY: every projection checked its tag and &mut self protects the
        // whole nested value for the duration of this callback.
        Ok(f(unsafe { &mut *pointer.cast_mut().cast::<T>() }))
    }

    /// Copy a live nested value when its complete concrete layout is Copy.
    /// This also works for composed records and optional values, without
    /// decoding them into an interpreter-level representation.
    pub fn copy_path(&self, path: &[DynamicPathStep]) -> Result<Self, String> {
        let (pointer, layout) = self.project(path)?;
        if !layout.is_copy() {
            return Err(format!("{} is not Copy", layout.rils_type()));
        }
        // None may own only its one-byte tag, even when its registered
        // descriptor reserves room for a larger Some payload.
        if matches!(layout.drop_kind, DropKind::Option { .. }) && unsafe { ptr::read(pointer) } == 0
        {
            return Self::none(layout);
        }
        let mut copy = Self::uninitialized(layout.clone());
        // SAFETY: project checked every parent tag and returned the aligned
        // pointer for this exact Copy layout. Both allocations are disjoint.
        unsafe {
            ptr::copy_nonoverlapping(pointer, copy.storage.pointer_mut(), layout.layout().size());
        }
        copy.initialized = true;
        Ok(copy)
    }

    fn field_parent(
        &self,
        path: &[DynamicPathStep],
    ) -> Result<(*const u8, usize, usize, Rc<DynamicLayout>), String> {
        let (&last, parents) = path
            .split_last()
            .ok_or_else(|| "native path is empty".to_owned())?;
        let DynamicPathStep::Field(index) = last else {
            return Err("a movable nested field path must end in Field".into());
        };
        let (pointer, layout) = self.project(parents)?;
        let DropKind::Record(record) = &layout.drop_kind else {
            return Err(format!("{} is not a record or aggregate", layout.rils_type));
        };
        let field = record.field(index)?;
        Ok((pointer, index, field.offset(), field.layout_handle()))
    }

    /// Resolve a record field's concrete layout, including when that field
    /// has already been moved out and is waiting for an assignment.
    pub fn path_field_layout(&self, path: &[DynamicPathStep]) -> Result<Rc<DynamicLayout>, String> {
        self.field_parent(path).map(|(_, _, _, layout)| layout)
    }

    /// Move a record field through any live option, variant or sequence parent.
    /// Only the final field becomes empty; all ancestors stay initialized.
    pub fn take_path_field(&mut self, path: &[DynamicPathStep]) -> Result<Self, String> {
        let (parent, index, offset, layout) = self.field_parent(path)?;
        // SAFETY: field_parent checked the index against an initialized record.
        if unsafe { ptr::read(parent.add(index)) } != 1 {
            return Err(format!("record field at path {path:?} has been moved"));
        }
        let mut child = Self::uninitialized(layout.clone());
        // SAFETY: the live field has the exact child layout and aligned offset.
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

    /// Restore an empty record field through a checked composite parent path.
    pub fn put_path_field(
        &mut self,
        path: &[DynamicPathStep],
        mut value: Self,
    ) -> Result<(), String> {
        let (parent, index, offset, layout) = self.field_parent(path)?;
        if !layout.compatible_with(&value.descriptor) {
            return Err(format!(
                "record field at path {path:?} has a different layout"
            ));
        }
        // SAFETY: field_parent checked the index against an initialized record.
        if unsafe { ptr::read(parent.add(index)) } == 1 {
            return Err(format!(
                "record field at path {path:?} is already initialized"
            ));
        }
        // SAFETY: the source has the exact child layout. Clearing its live bit
        // transfers its destructor to this field's live tag.
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

    /// Assign a record field through a checked composite path. The previous
    /// value, if present, remains owned by the caller and is dropped exactly
    /// once. A layout mismatch leaves the record untouched.
    pub fn replace_path_field(
        &mut self,
        path: &[DynamicPathStep],
        mut value: Self,
    ) -> Result<Option<Self>, String> {
        let (parent, index, offset, layout) = self.field_parent(path)?;
        if !layout.compatible_with(&value.descriptor) {
            return Err(format!(
                "record field at path {path:?} has a different layout"
            ));
        }
        // SAFETY: field_parent checked the record and index. Its tag owns the
        // old value if live; copying to a fresh owner and clearing the tag
        // transfers that ownership before installing the new value.
        let previous = if unsafe { ptr::read(parent.add(index)) } == 1 {
            let mut old = Self::uninitialized(layout.clone());
            unsafe {
                ptr::copy_nonoverlapping(
                    parent.add(offset),
                    old.storage.pointer_mut(),
                    layout.layout().size(),
                );
                ptr::write(parent.cast_mut().add(index), 0);
            }
            old.initialized = true;
            Some(old)
        } else {
            None
        };
        // SAFETY: the child descriptor was checked above, and the field is
        // now empty. Clearing the source's live bit transfers its destructor.
        unsafe {
            ptr::copy_nonoverlapping(
                value.storage.pointer(),
                parent.cast_mut().add(offset),
                value.storage.size(),
            );
            value.initialized = false;
            ptr::write(parent.cast_mut().add(index), 1);
        }
        Ok(previous)
    }
}
