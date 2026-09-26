//! Adapters between general native method registration and current Value APIs.

use super::{NativeObject, Value};

pub const CLONE: &str = "Clone::clone";
pub const EQUAL: &str = "PartialEq::eq";
pub const DISPLAY: &str = "Display::fmt";
pub const NEXT: &str = "Iterator::next";

pub fn is_iterator(value: &Value) -> bool {
    matches!(value, Value::Native(object) if object.descriptor().has_method(NEXT))
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
        Some(Ok(Value::Bool(true)))
    )
}

pub fn display(object: &NativeObject) -> String {
    match object.call(DISPLAY, &[]) {
        Some(Ok(value)) => value
            .as_string()
            .unwrap_or_else(|| format!("<{}>", object.descriptor().rils_type())),
        _ => format!("<{}>", object.descriptor().rils_type()),
    }
}

pub fn next(object: &NativeObject) -> Option<Result<Option<Value>, String>> {
    object.call(NEXT, &[]).map(|result| {
        let value = result?;
        match value {
            Value::Option { value, .. } => value
                .map(|value| {
                    std::rc::Rc::try_unwrap(value)
                        .map_err(|_| "native iterator returned a shared item".to_owned())
                })
                .transpose(),
            _ => Err("native Iterator::next must return Option".into()),
        }
    })
}
