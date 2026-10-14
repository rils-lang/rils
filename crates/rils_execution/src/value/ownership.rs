//! Ownership, lexical references, and explicit cloning of execution values.

use super::record_codec::native::{NativeChildState, any_native_child};
use super::*;

impl Value {
    pub fn is_copy(&self) -> bool {
        if let Some(callable) = self.callable() {
            return callable.is_copy();
        }
        match self {
            // Portable host handles are opaque, copyable identity tokens. The payload is
            // reference-counted internally, but copying the token must not copy
            // or transfer ownership of the host object itself.
            Self::HostObject(object) => object.type_definition.copy,
            Self::Native(object) => object.descriptor().is_copy(),
            Self::Dynamic(object) => {
                object.descriptor().layout().is_copy()
                    && !object
                        .with(|value| runtime_layouts::contains_noncopy_callable(value.view()))
                        .and_then(|result| result)
                        .unwrap_or(true)
            }
        }
    }

    pub fn contains_reference(&self) -> bool {
        match self {
            Self::Dynamic(object) => {
                any_native_child(object, Value::contains_reference, NativeChildState::None)
                    || object
                        .with(|value| runtime_layouts::contains_reference(value.view()))
                        .and_then(|result| result)
                        .unwrap_or(true)
            }

            Self::Native(object) => object.any_child(Value::contains_reference),

            _ => false,
        }
    }

    pub fn contains_local_reference(&self, environment: &EnvironmentRef) -> bool {
        match self {
            Self::Dynamic(object) => {
                any_native_child(
                    object,
                    |value| value.contains_local_reference(environment),
                    NativeChildState::None,
                ) || object
                    .with(|value| {
                        runtime_layouts::contains_local_reference(value.view(), environment)
                    })
                    .and_then(|result| result)
                    .unwrap_or(true)
            }

            Self::Native(object) => {
                object.any_child(|value| value.contains_local_reference(environment))
            }

            _ => false,
        }
    }

    pub fn has_active_references(&self) -> bool {
        match self {
            Self::Dynamic(object) => {
                any_native_child(
                    object,
                    Value::has_active_references,
                    NativeChildState::References,
                ) || object
                    .with(|payload| {
                        payload.has_path_references()
                            || payload
                                .sequence_borrows()
                                .is_ok_and(|ledger| ledger.has_active())
                    })
                    .unwrap_or(true)
            }
            Self::Native(object) => {
                object.has_active_references() || object.any_child(Value::has_active_references)
            }
            _ => false,
        }
    }

    pub fn is_partially_moved(&self) -> bool {
        match self {
            Self::Dynamic(object) => {
                any_native_child(object, Value::is_partially_moved, NativeChildState::Partial)
                    || object
                        .with(|value| value.view().is_partially_moved())
                        .and_then(|result| result)
                        .unwrap_or(true)
            }
            Self::Native(object) => {
                object.is_partially_moved() || object.any_child(Value::is_partially_moved)
            }

            _ => false,
        }
    }

    pub fn clone_owned(&self) -> Result<Self, String> {
        if let Self::Dynamic(object) = self
            && object.with(|value| runtime_layouts::contains_noncopy_callable(value.view()))??
        {
            return Err("cannot clone a callable with a non-Copy bound receiver".into());
        }
        Ok(match self {
            Self::Native(object) => Self::Native(native_ops::clone_owned(object)?),
            Self::Dynamic(object) => {
                let codec = object
                    .descriptor()
                    .metadata::<super::record_codec::NativeRecordCodec>()
                    .unwrap_or_default();
                let payload = object.with(|payload| codec.clone_view(payload.view()))??;
                Self::Dynamic(super::native_instance::with_descriptor(
                    object.descriptor_handle(),
                    payload,
                )?)
            }
            value => value.clone(),
        })
    }
}
