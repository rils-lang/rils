//! Runtime-composed storage for options whose items have registered Rust layouts.

use std::rc::Rc;

use rils_value::{DynamicType, DynamicValue};

use crate::Type;

use super::{DynamicObject, EnumType, StructType, Value, record_layout::RecordLayoutResolver};

/// Construct `None` using the concrete item layout, including nominal types.
pub fn none_with_definitions(
    item_type: &Type,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Value, String> {
    let mut resolver = RecordLayoutResolver::with_enums(structs, enums);
    let option_type = Type::Option(Box::new(item_type.clone()));
    let layout = resolver.resolve(&option_type)?;
    let payload = DynamicValue::none(layout.clone())?;
    Ok(Value::Dynamic(DynamicObject::new(
        Rc::new(DynamicType::new(layout)),
        payload,
    )?))
}

/// Consumes an Option and transfers its initialized child without cloning it.
pub fn take_owned(value: Value) -> Result<Option<Value>, String> {
    take_owned_with_definitions(value, &[], &[])
}

pub fn take_owned_with_definitions(
    value: Value,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Option<Value>, String> {
    match value {
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Option(_)) =>
        {
            let codec = super::sum::codec(&object, structs, enums)?;
            let value = object.into_value().map_err(|failure| failure.1)?;
            value
                .take_option()?
                .map(|item| codec.from_native(item))
                .transpose()
        }
        value => Err(format!("expected Option, found {}", value.type_name())),
    }
}
