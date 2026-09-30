//! Float intrinsic dispatch into the Rust standard-library definitions.

use crate::Value;

pub(super) fn constant(
    target: crate::FloatType,
    constant: rils_builtins::FloatConstantId,
) -> Value {
    super::native::float::constant(target, constant)
        .expect("every float constant has a native binding")
        .expect("float constants cannot fail")
}

pub(super) fn handles(symbol: &str) -> bool {
    rils_builtins::FLOAT_INTRINSICS
        .iter()
        .any(|item| item.symbol == symbol)
}

pub(super) fn execute(symbol: &str, values: &[Value]) -> Result<Value, String> {
    super::native::float::call(symbol, values).unwrap_or_else(|| match values.first() {
        Some(value) => Err(format!(
            "float intrinsic expects a float receiver, found {}",
            value.type_name()
        )),
        None => Err("float intrinsic is missing its receiver".into()),
    })
}
