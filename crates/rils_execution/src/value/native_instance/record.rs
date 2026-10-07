//! Shared record inspection and lexical field projections for both backends.

use std::rc::Rc;

use super::{NativeInstancePlace, Value, value_definition};
use crate::value::StructType;

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
