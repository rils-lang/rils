//! Native storage for results with concrete, readable standard-library payloads.

use std::rc::Rc;

use crate::Type;

use super::{EnumType, StructType, Value};

/// Consume a Result and transfer its active branch without cloning its payload.
pub fn take_owned(value: Value) -> Result<Result<Value, Value>, String> {
    take_owned_with_definitions(value, &[], &[])
}

pub fn take_owned_with_definitions(
    value: Value,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Result<Value, Value>, String> {
    match value {
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Result(_, _)) =>
        {
            let codec = super::sum::codec(&object, structs, enums)?;
            let value = object.into_value().map_err(|failure| failure.1)?;
            let (index, item) = value.take_variant()?;
            let item = codec.from_native(item)?;
            match index {
                0 => Ok(Ok(item)),
                1 => Ok(Err(item)),
                _ => Err("Result has an invalid variant tag".into()),
            }
        }
        value => Err(format!("expected Result, found {}", value.type_name())),
    }
}
