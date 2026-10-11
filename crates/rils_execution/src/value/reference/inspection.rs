//! Compatibility referents can be inspected while their original slots stay borrowed.

use super::super::borrowed::{Read, with_read};
use super::{ReferenceTarget, ReferenceValue};

impl ReferenceValue {
    pub(crate) fn referent_type(&self) -> Result<crate::Type, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) => Ok(view.layout()?.rils_type().clone()),
            Read::Leaf(leaf) => Ok(leaf.rils_type().clone()),
            Read::Legacy(value) => crate::Type::of_value(value)
                .ok_or_else(|| "reference target has no runtime type".into()),
        })?
    }

    pub(crate) fn accepts_referent(&self, expected: &crate::Type) -> Result<bool, String> {
        self.with_borrowed_target(|value| match value {
            Read::View(view) => super::super::runtime_layouts::accepts_view(view, expected),
            Read::Leaf(leaf) => Ok(crate::types::merge_types(expected, leaf.rils_type()).is_some()),
            Read::Legacy(value) => Ok(expected.accepts(value)),
        })?
    }

    /// Inspect one reference target, preserving any reference stored inside it.
    pub(crate) fn with_borrowed_target<R>(
        &self,
        callback: impl for<'a> FnOnce(Read<'a>) -> R,
    ) -> Result<R, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| with_read(value, false, callback)),
            ReferenceTarget::IndexedElement { sequence, index } => {
                let elements = sequence
                    .elements
                    .try_borrow()
                    .map_err(|_| "referenced sequence is already mutably accessed".to_owned())?;
                let item = elements
                    .get(*index)
                    .and_then(|slot| slot.value.as_ref())
                    .ok_or("referenced element was moved")?;
                with_read(item, false, callback)
            }
            ReferenceTarget::DynamicIndexedElement { .. }
            | ReferenceTarget::DynamicCell { .. }
            | ReferenceTarget::DynamicField(_) => {
                self.with_native_view(|view| callback(Read::View(view)))
            }
            ReferenceTarget::MapKey { map, key } => {
                map.with_entry(key, |key, _| key.with_read(callback))?
            }
            ReferenceTarget::MapValue { map, key } => map.with_entry(key, |_, slot| {
                with_read(
                    slot.value
                        .as_ref()
                        .ok_or("referenced map value was moved")?,
                    false,
                    callback,
                )
            })?,
            ReferenceTarget::SetItem { set, key } => {
                set.with_item(key, |key| key.with_read(callback))?
            }
        }
    }
}
