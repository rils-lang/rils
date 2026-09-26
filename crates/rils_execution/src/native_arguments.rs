//! Signature-driven conversion of formatted native arguments.
use crate::{FunctionSignature, Type, Value};
use rils_frontend::format::FormatSpec;

/// A native `T: Display` is represented by its rendered text at the Rust boundary.
/// The backend supplies trait dispatch, so user implementations participate too.
pub fn prepare<E>(
    signature: Option<&FunctionSignature>,
    arguments: &[Value],
    mut render: impl FnMut(&Value, &FormatSpec) -> Result<String, E>,
) -> Result<Vec<Value>, E> {
    let Some(parameters) = signature.and_then(|signature| signature.parameters.as_ref()) else {
        return Ok(arguments.to_vec());
    };
    arguments.iter().enumerate().map(|(index, value)| {
        let Some(Type::BoundVariable { bounds, .. }) = parameters.get(index) else { return Ok(value.clone()); };
        if bounds.iter().any(|bound| matches!(bound, Type::Named { name, .. } if name == "core::fmt::Display" || name == "Display")) {
            render(value, &FormatSpec::default()).map(|text| Value::String(text.into()))
        } else { Ok(value.clone()) }
    }).collect()
}
