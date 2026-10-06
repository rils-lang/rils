//! Native receiver context that retains the original owner and projection.

use super::{DynamicObject, ReferenceValue, Value};
use rils_value::{DynamicType, DynamicValueMut, DynamicValueRef};
use std::rc::Rc;

enum Storage {
    Object(DynamicObject),
    Reference(Rc<ReferenceValue>),
}

pub(crate) struct NativeReceiver {
    descriptor: Rc<DynamicType<Value>>,
    storage: Storage,
}

impl NativeReceiver {
    pub(crate) fn from_value(value: &Value) -> Result<Option<Self>, String> {
        match value {
            Value::Dynamic(object) => Ok(Some(Self {
                descriptor: object.descriptor_handle(),
                storage: Storage::Object(object.clone()),
            })),
            Value::Reference(reference) => {
                let Some(layout) = reference.native_layout()? else {
                    return Ok(None);
                };
                Ok(Some(Self {
                    descriptor: Rc::new(DynamicType::new(layout)),
                    storage: Storage::Reference(reference.clone()),
                }))
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn descriptor(&self) -> &DynamicType<Value> {
        &self.descriptor
    }

    pub(crate) fn with<R>(
        &self,
        callback: impl FnOnce(DynamicValueRef<'_>) -> R,
    ) -> Result<R, String> {
        match &self.storage {
            Storage::Object(object) => object.with(|value| callback(value.view())),
            Storage::Reference(reference) => reference.with_native_view(callback),
        }
    }

    pub(crate) fn with_mut<R>(
        &self,
        callback: impl FnOnce(DynamicValueMut<'_>) -> R,
    ) -> Result<R, String> {
        match &self.storage {
            Storage::Object(object) => object.with_mut(|value| callback(value.view_mut())),
            Storage::Reference(reference) => reference.with_native_mut(callback),
        }
    }

    pub(crate) fn same_storage(&self, other: &Self) -> bool {
        match (
            self.with(|view| view.sequence_borrows()),
            other.with(|view| view.sequence_borrows()),
        ) {
            (Ok(Ok(left)), Ok(Ok(right))) => Rc::ptr_eq(&left, &right),
            _ => false,
        }
    }

    pub(crate) fn owned_object(&self) -> Result<DynamicObject, String> {
        match &self.storage {
            Storage::Object(object) => Ok(object.clone()),
            Storage::Reference(_) => Err("cannot consume a borrowed native receiver".into()),
        }
    }
}
