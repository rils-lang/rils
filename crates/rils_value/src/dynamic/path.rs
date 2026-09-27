//! Checked access to leaves inside heterogeneous native compositions.

use std::{any::TypeId, ptr, rc::Rc};

use super::{DropKind, DynamicLayout, DynamicValue};

/// One checked projection into a runtime-composed native value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DynamicPathStep {
    Field(usize),
    Some,
    Variant(usize),
    Index(usize),
}

impl DynamicValue {
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
                    // SAFETY: this descriptor initializes a Vec<DynamicValue>
                    // and the owner is borrowed for the entire callback.
                    let items = unsafe { &*pointer.cast::<Vec<DynamicValue>>() };
                    let child = items
                        .get(*index)
                        .ok_or_else(|| format!("sequence index {index} is out of bounds"))?;
                    debug_assert!(Rc::ptr_eq(item, &child.descriptor));
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
}
