//! Host type mappings borrow either standalone wrappers or composed leaves.

use super::{ReferenceTarget, ReferenceValue};
use crate::host_value::{RilsHostType, with_host_value};

impl ReferenceValue {
    pub(crate) fn with_host_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        match &self.target {
            ReferenceTarget::Storage(target) => target
                .borrow()
                .with_value(|value| with_host_value(value, callback)),
            ReferenceTarget::StructField { instance, index } => {
                let fields = instance.fields.borrow();
                let value = fields
                    .get_index(*index)
                    .and_then(|field| field.value.as_ref())
                    .ok_or("reference target field has been moved")?;
                with_host_value(value, callback)
            }
            ReferenceTarget::IndexedElement { sequence, index } => {
                let elements = sequence.elements.borrow();
                let value = elements
                    .get(*index)
                    .and_then(|field| field.value.as_ref())
                    .ok_or("reference target element has been moved")?;
                with_host_value(value, callback)
            }
            ReferenceTarget::DynamicIndexedElement { .. }
            | ReferenceTarget::DynamicCell { .. }
            | ReferenceTarget::DynamicField(_) => {
                self.with_native_view(|view| T::with_composed_ref(view, callback))?
            }
            _ => self.with_rust::<T::Native, _>(|value| callback(T::as_native_ref(value))),
        }
    }
}
