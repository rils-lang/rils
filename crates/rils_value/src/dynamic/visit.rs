//! Inspect live native leaves without copying or decoding the owning value.

use std::ptr;

use super::{DropKind, DynamicValueRef};

impl DynamicValueRef<'_> {
    /// Check the initialized field tags of the active composition. An empty
    /// Option and inactive enum alternatives do not contain moved fields.
    pub fn is_partially_moved(&self) -> Result<bool, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        match &layout.drop_kind {
            DropKind::Rust { .. } => Ok(false),
            DropKind::Option { .. } => {
                Ok(self.option_is_some()? && self.option_item()?.is_partially_moved()?)
            }
            DropKind::Variant(_) => self.variant_payload()?.is_partially_moved(),
            DropKind::Record(record) => {
                for index in 0..record.fields.len() {
                    // SAFETY: project checked the live parent and each tag is
                    // inside the record prefix. Never visit an empty field.
                    if unsafe { ptr::read(pointer.add(index)) } != 1
                        || self.field(index)?.is_partially_moved()?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            DropKind::Sequence { .. } => {
                for index in 0..self.sequence_len()? {
                    if self.sequence_item(index)?.is_partially_moved()? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }

    /// Apply a predicate to live Rust leaves. Inactive branches and moved
    /// fields are skipped; the traversal stops at the first matching leaf.
    pub fn any_leaf(
        &self,
        predicate: &mut dyn FnMut(DynamicValueRef<'_>) -> Result<bool, String>,
    ) -> Result<bool, String> {
        let (pointer, layout) = self.root.project(&self.path)?;
        match &layout.drop_kind {
            DropKind::Rust { .. } => predicate(self.root.view_path(&self.path)?),
            DropKind::Option { .. } => {
                if self.option_is_some()? {
                    self.option_item()?.any_leaf(predicate)
                } else {
                    Ok(false)
                }
            }
            DropKind::Variant(_) => self.variant_payload()?.any_leaf(predicate),
            DropKind::Record(record) => {
                for index in 0..record.fields.len() {
                    // SAFETY: project retained the record owner and checked
                    // every parent tag; each field tag is in the prefix.
                    if unsafe { ptr::read(pointer.add(index)) } == 1
                        && self.field(index)?.any_leaf(predicate)?
                    {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
            DropKind::Sequence { .. } => {
                for index in 0..self.sequence_len()? {
                    if self.sequence_item(index)?.any_leaf(predicate)? {
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        }
    }
}
