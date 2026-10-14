//! Native storage for the standard-library owned string.

use rils_stdlib::stdlib::string::String as NativeString;

use super::{NativeObject, Value};

pub fn native_string(value: impl Into<std::string::String>) -> NativeObject {
    NativeObject::new(
        super::native_layouts::string::descriptor(),
        NativeString::from(value.into()),
    )
    .expect("generated string descriptor matches its Rust payload")
}

pub fn string_payload(value: &Value) -> Option<std::string::String> {
    value.get_cloned::<std::string::String>().ok()
}
