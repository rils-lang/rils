//! Compatibility referents can be inspected while their original slots stay borrowed.

use super::{DynamicValueRef, ReferenceTarget, ReferenceValue, Value};

pub(crate) enum BorrowedTarget<'a> {
    Value(&'a Value),
    Native(DynamicValueRef<'a>),
}

impl ReferenceValue {
    /// Inspect one reference target, preserving any reference stored inside it.
    pub(crate) fn with_borrowed_target<R>(
        &self,
        callback: impl for<'a> FnOnce(BorrowedTarget<'a>) -> R,
    ) -> Result<R, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .try_borrow()
                .map_err(|_| "reference target is already mutably accessed".to_owned())?
                .with_value(|value| Ok(callback(BorrowedTarget::Value(value)))),
            ReferenceTarget::IndexedElement { sequence, index } => {
                let elements = sequence
                    .elements
                    .try_borrow()
                    .map_err(|_| "referenced sequence is already mutably accessed".to_owned())?;
                let item = elements
                    .get(*index)
                    .and_then(|slot| slot.value.as_ref())
                    .ok_or("referenced element was moved")?;
                Ok(callback(BorrowedTarget::Value(item)))
            }
            ReferenceTarget::DynamicIndexedElement { .. }
            | ReferenceTarget::DynamicCell { .. }
            | ReferenceTarget::DynamicField(_) => {
                self.with_native_view(|view| callback(BorrowedTarget::Native(view)))
            }
            // Compatibility map/set projections still use their old read API.
            _ => self
                .read()
                .map(|value| callback(BorrowedTarget::Value(&value))),
        }
    }
}
