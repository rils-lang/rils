//! Native storage for the standard-library owned string.

use rils_stdlib::stdlib::string::String as NativeString;

use crate::Type;

use super::{NativeObject, Value};

pub fn native_string(value: impl Into<std::string::String>) -> Value {
    Value::Native(
        NativeObject::new(
            super::native_layouts::string::descriptor(),
            NativeString::from(value.into()),
        )
        .expect("generated string descriptor matches its Rust payload"),
    )
}

pub fn string_payload(value: &Value) -> Option<std::string::String> {
    match value {
        Value::String(text) => Some(text.to_string()),
        Value::Native(object) if object.descriptor().rils_type() == &Type::String => object
            .with::<NativeString, _>(|value| std::string::String::from(value.clone()))
            .ok(),
        _ => None,
    }
}
