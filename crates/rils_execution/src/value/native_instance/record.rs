//! Shared record inspection and lexical field projections for both backends.

use std::rc::Rc;

use super::{NativeInstancePlace, Value, value_definition};
use crate::{Type, value::StructType};

pub fn record_definition(value: &Value) -> Result<Option<Rc<StructType>>, String> {
    if !matches!(value, Value::Dynamic(_) | Value::Reference(_)) {
        return Ok(None);
    }
    match value_definition(value)? {
        Some(Value::StructType(definition)) if !definition.opaque_native => Ok(Some(definition)),
        _ => Ok(None),
    }
}

pub fn record_field_names(value: &Value) -> Result<Vec<String>, String> {
    let layout = match value {
        Value::Dynamic(object) => object.descriptor().layout_handle(),
        Value::Reference(reference) => reference
            .native_layout()?
            .ok_or("record has no native layout")?,
        _ => return Err("value is not a native record".into()),
    };
    Ok(layout
        .record_fields()
        .ok_or("value is not a record")?
        .iter()
        .map(|field| field.name().to_owned())
        .collect())
}

/// Field bindings and formatters retain a checked path into the actual owner.
pub fn borrow_field(value: &Value, name: &str) -> Result<Value, String> {
    let reference = match value {
        Value::Dynamic(object) => NativeInstancePlace::new(object.clone())?
            .field(name)?
            .borrow(false, None)?,
        Value::Reference(reference) => reference
            .project_native_field(name)?
            .ok_or_else(|| format!("record has no field `{name}`"))?
            .reborrow(false)?,
        _ => return Err("value is not a native record".into()),
    };
    Ok(Value::Reference(Rc::new(reference)))
}

pub(crate) fn equal(left: &Value, right: &Value) -> Option<bool> {
    let definition = record_definition(left)
        .ok()
        .flatten()
        .or_else(|| record_definition(right).ok().flatten())?;
    let actual_type = |value: &Value| match value {
        Value::Reference(reference) => reference
            .native_layout()
            .ok()
            .flatten()
            .map(|layout| layout.rils_type().clone()),
        _ => Type::of_value(value),
    };
    if actual_type(left) != actual_type(right) {
        return Some(false);
    }
    Some(definition.fields.iter().all(|field| {
        let read = |value: &Value| -> Result<Value, String> {
            match value {
                Value::Struct(instance) => instance
                    .fields
                    .borrow()
                    .get(&field.name)
                    .and_then(|slot| slot.value.clone())
                    .ok_or("record field was moved".into()),
                value => borrow_field(value, &field.name),
            }
        };
        let (Ok(left), Ok(right)) = (read(left), read(right)) else {
            return false;
        };
        if let Some(equal) = equal(&left, &right) {
            return equal;
        }
        let read = |value: Value| match value {
            Value::Reference(reference) => reference.read(),
            value => Ok(value),
        };
        matches!((read(left), read(right)), (Ok(left), Ok(right)) if left == right)
    }))
}
