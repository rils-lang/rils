//! Copy native bytes and managed identity handles according to their layouts.

use std::{ptr, rc::Rc};

use super::{DropKind, DynamicLayout, DynamicValue};

pub(super) unsafe fn copy_handle<T: Clone>(source: *const u8, destination: *mut u8) {
    // SAFETY: registration binds both aligned addresses to exactly T. Clone
    // completes before writing, so a panic leaves the destination uninitialized.
    let value = unsafe { &*source.cast::<T>() }.clone();
    unsafe { ptr::write(destination.cast::<T>(), value) };
}

impl DynamicValue {
    /// The caller retains an initialized source matching this exact descriptor.
    pub(super) unsafe fn copy_at(
        descriptor: Rc<DynamicLayout>,
        source: *const u8,
    ) -> Result<Self, String> {
        if !descriptor.is_copy() {
            return Err(format!("{} is not Copy", descriptor.rils_type()));
        }
        // None may allocate only its tag even when the descriptor reserves
        // room for an inactive child. Check this before the byte-copy fast path.
        if matches!(descriptor.drop_kind, DropKind::Option { .. })
            && unsafe { ptr::read(source) } == 0
        {
            return Self::none(descriptor);
        }
        if descriptor.bitwise_copy {
            let mut result = Self::uninitialized(descriptor.clone());
            // SAFETY: this guarantee is propagated only from Rust Copy leaves.
            // The initialized source and destination have disjoint storage.
            unsafe {
                ptr::copy_nonoverlapping(
                    source,
                    result.storage.pointer_mut(),
                    descriptor.layout.size(),
                );
            }
            result.initialized = true;
            return Ok(result);
        }
        match &descriptor.drop_kind {
            DropKind::Rust { copy_handle, .. } => {
                let mut result = Self::uninitialized(descriptor.clone());
                if let Some(copy) = copy_handle {
                    // SAFETY: both allocations match the registered Rust type.
                    unsafe { copy(source, result.storage.pointer_mut()) };
                } else {
                    // SAFETY: copy_of requires Rust Copy; these bytes own no
                    // managed resources and the allocations are disjoint.
                    unsafe {
                        ptr::copy_nonoverlapping(
                            source,
                            result.storage.pointer_mut(),
                            descriptor.layout.size(),
                        )
                    };
                }
                result.initialized = true;
                Ok(result)
            }
            DropKind::Option { item, item_offset } => {
                // SAFETY: an initialized option always has its tag byte, even
                // when None has no allocation for the inactive child.
                if unsafe { ptr::read(source) } == 0 {
                    return Self::none(descriptor.clone());
                }
                // SAFETY: Some owns a live aligned child at the layout's offset.
                let child = unsafe { Self::copy_at(item.clone(), source.add(*item_offset)) }?;
                Self::some(descriptor.clone(), child)
            }
            DropKind::Variant(variant) => {
                // SAFETY: a constructed variant owns its active child's bytes.
                let index = unsafe { ptr::read(source.cast::<u32>()) } as usize;
                let child = unsafe {
                    Self::copy_at(
                        variant.alternatives[index].clone(),
                        source.add(variant.payload_offset),
                    )
                }?;
                Self::variant(descriptor.clone(), index, child)
            }
            DropKind::Record(record) => {
                let mut result = Self::uninitialized(descriptor.clone());
                // SAFETY: the tag prefix is allocated even for an empty record.
                unsafe { ptr::write_bytes(result.storage.pointer_mut(), 0, record.fields.len()) };
                result.initialized = true;
                for (index, field) in record.fields.iter().enumerate() {
                    // SAFETY: only a live field may be copied. Moved fields keep
                    // their tag unset; a panic drops already copied children.
                    if unsafe { ptr::read(source.add(index)) } != 1 {
                        continue;
                    }
                    let mut child = unsafe {
                        Self::copy_at(field.layout_handle(), source.add(field.offset()))
                    }?;
                    // SAFETY: transfer the completed child's sole ownership
                    // into its aligned slot before setting that slot's live tag.
                    unsafe {
                        ptr::copy_nonoverlapping(
                            child.storage.pointer(),
                            result.storage.pointer_mut().add(field.offset()),
                            child.storage.size(),
                        );
                        child.initialized = false;
                        ptr::write(result.storage.pointer_mut().add(index), 1);
                    }
                }
                Ok(result)
            }
            DropKind::Sequence { .. } => unreachable!("owned sequences are not Copy"),
        }
    }
}
