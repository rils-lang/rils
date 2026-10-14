//! Adapters between general native method registration and current Value APIs.

use super::{NativeObject, Value};

pub const CLONE: &str = "Clone::clone";
pub const EQUAL: &str = "PartialEq::eq";
pub const DISPLAY: &str = "Display::fmt";
pub const NEXT: &str = "Iterator::next";

pub fn is_iterator(value: &Value) -> bool {
    has_method(value, NEXT).unwrap_or(false)
}

/// Inspect method registration without cloning or decoding the receiver payload.
pub fn has_method(value: &Value, symbol: &str) -> Result<bool, String> {
    if let Some(reference) = value.as_reference() {
        return reference.has_native_method(symbol);
    }
    match value {
        Value::Native(object) => Ok(object.descriptor().has_method(symbol)),
        Value::Dynamic(object) => {
            let Some(codec) = object
                .descriptor()
                .metadata::<super::record_codec::NativeRecordCodec>()
            else {
                return Ok(false);
            };
            Ok(codec
                .registered_type(object.descriptor().layout())?
                .is_some_and(|descriptor| descriptor.has_method(symbol)))
        }
        Value::HostObject(_) => Ok(false),
    }
}

pub fn clone_owned(object: &NativeObject) -> Result<NativeObject, String> {
    if object.descriptor().is_copy() {
        return object.copy_owned();
    }
    match object.call(CLONE, &[]).ok_or_else(|| {
        format!(
            "native type {} does not implement Clone",
            object.descriptor().rils_type()
        )
    })?? {
        Value::Native(clone) => Ok(clone),
        _ => Err("native Clone must return a native value".into()),
    }
}

pub fn equal(left: &NativeObject, right: &NativeObject) -> bool {
    if left.descriptor().rils_type() != right.descriptor().rils_type() {
        return false;
    }
    matches!(
        left.call(EQUAL, &[Value::Native(right.clone())]),
        Some(Ok(value)) if value.get_cloned::<bool>().ok() == Some(true)
    )
}

pub fn display(object: &NativeObject) -> String {
    match object.call(DISPLAY, &[]) {
        Some(Ok(value)) => value
            .get_cloned::<String>()
            .ok()
            .unwrap_or_else(|| format!("<{}>", object.descriptor().rils_type())),
        _ => format!("<{}>", object.descriptor().rils_type()),
    }
}

pub fn next(object: &NativeObject) -> Option<Result<Option<Value>, String>> {
    object
        .call(NEXT, &[])
        .map(|result| super::dynamic_option::take_owned(result?))
}
