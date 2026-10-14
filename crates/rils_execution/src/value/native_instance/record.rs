//! Shared record inspection and lexical field projections for both backends.

use std::rc::Rc;

use super::{NativeInstancePlace, Value, value_definition};
use crate::value::StructType;

pub fn record_definition(value: &Value) -> Result<Option<Rc<StructType>>, String> {
    if !matches!(value, Value::Dynamic(_)) {
        return Ok(None);
    }
    match value_definition(value)? {
        Some(crate::value::Declaration::StructType(definition)) if !definition.opaque_native => {
            Ok(Some(definition))
        }
        _ => Ok(None),
    }
}

pub fn record_field_names(value: &Value) -> Result<Vec<String>, String> {
    let layout = match value {
        __reference_value if __reference_value.as_reference().is_some() => {
            let reference = __reference_value
                .as_reference()
                .expect("checked reference handle");
            reference
                .native_layout()?
                .ok_or("record has no native layout")?
        }
        Value::Dynamic(object) => object.descriptor().layout_handle(),

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
        __reference_value if __reference_value.as_reference().is_some() => {
            let reference = __reference_value
                .as_reference()
                .expect("checked reference handle");
            reference
                .project_native_field(name)?
                .ok_or_else(|| format!("record has no field `{name}`"))?
                .reborrow(false)?
        }
        Value::Dynamic(object) => NativeInstancePlace::new(object.clone())?
            .field(name)?
            .borrow(false, None)?,

        _ => return Err("value is not a native record".into()),
    };
    Ok(Value::from_reference(Rc::new(reference)))
}
