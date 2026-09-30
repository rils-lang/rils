//! Shared receiver queries for native sequence backed containers.

use crate::{Type, Value};
use rils_value::DynamicValue;

fn receiver(
    owner: &str,
    arguments: &[Value],
) -> Option<Result<crate::value::DynamicObject, String>> {
    let value = match &arguments[0] {
        Value::Reference(reference) => match reference.read() {
            Ok(value) => value,
            Err(error) => return Some(Err(error)),
        },
        value => value.clone(),
    };
    let Value::Dynamic(object) = value else {
        return None;
    };
    let layout = object.descriptor().layout();
    let matches = matches!(layout.rils_type(), Type::Named { name, .. } if name == owner)
        && layout.sequence_item().is_some();
    matches.then_some(Ok(object))
}

pub(super) fn query(
    owner: &str,
    method: &str,
    arguments: &[Value],
) -> Option<Result<Value, String>> {
    if arguments.len() != 1 {
        return Some(Err(format!(
            "{owner}::{method} expects one receiver, found {} arguments",
            arguments.len()
        )));
    }
    let object = match receiver(owner, arguments)? {
        Ok(object) => object,
        Err(error) => return Some(Err(error)),
    };
    Some(
        object
            .with(|value| value.sequence_len())
            .and_then(|length| {
                length.map(|length| match method {
                    "len" => crate::numeric::native_usize(length),
                    "is_empty" => Value::Bool(length == 0),
                    _ => unreachable!("generated sequence query"),
                })
            }),
    )
}

pub(super) fn clear(owner: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    if arguments.len() != 1 {
        return Some(Err(format!(
            "{owner}::clear expects one receiver, found {} arguments",
            arguments.len()
        )));
    }
    let object = match receiver(owner, arguments)? {
        Ok(object) => object,
        Err(error) => return Some(Err(error)),
    };
    if !matches!(&arguments[0], Value::Reference(reference) if reference.mutable) {
        return Some(Err(format!("{owner}::clear requires `&mut self`")));
    }
    Some(
        object
            .with_mut(DynamicValue::clear_sequence)
            .and_then(|result| result.map(|()| Value::Unit)),
    )
}
