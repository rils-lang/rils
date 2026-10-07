//! Move an active sum payload out of a shared execution owner.

use std::ptr;

use super::{DropKind, DynamicValue};

impl DynamicValue {
    /// Transfer the active child without cloning. The source becomes moved and
    /// rejects subsequent reads. Any live reference into it prevents the move.
    /// The returned index is 0 for Some and the active tag for a variant.
    pub fn take_active_payload(&mut self) -> Result<Option<(usize, Self)>, String> {
        self.ensure_initialized()?;
        self.check_path_move(&[])?;
        if self.view().is_partially_moved()? {
            return Err("cannot move a partially moved sum payload".into());
        }
        let (index, child, offset) = match &self.descriptor.drop_kind {
            DropKind::Option { item, item_offset } => {
                if !self.is_some()? {
                    self.initialized = false;
                    return Ok(None);
                }
                (0, item.clone(), *item_offset)
            }
            DropKind::Variant(variant) => {
                let index = self.variant_index()?;
                let child = variant
                    .alternatives
                    .get(index)
                    .ok_or("variant tag is out of bounds")?
                    .clone();
                (index, child, variant.payload_offset)
            }
            _ => return Err("value is not a sum".into()),
        };
        let mut payload = Self::uninitialized(child.clone());
        // SAFETY: the active tag and layout establish a live, aligned child.
        // Moving into a fresh buffer and clearing the root's live bit transfers
        // its sole destructor. The borrow ledger rejects all live projections.
        unsafe {
            ptr::copy_nonoverlapping(
                self.storage.pointer().add(offset),
                payload.storage.pointer_mut(),
                child.layout.size(),
            );
        }
        self.initialized = false;
        payload.initialized = true;
        Ok(Some((index, payload)))
    }
}
