//! Native storage for results with concrete, readable standard-library payloads.

use std::rc::Rc;

use rils_value::DynamicValue;

use crate::Type;

use super::{EnumType, StructType, Value, record_codec::NativeRecordCodec};

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
            if object
                .descriptor()
                .has_owned_operation(super::owned_sum::DECODE_OPERATION)
            {
                let value = object.call_owned(super::owned_sum::DECODE_OPERATION)?;
                return take_owned_with_definitions(value, structs, enums);
            }
            let value = object.into_value().map_err(|failure| failure.1)?;
            let (index, item) = value.take_variant()?;
            let item = NativeRecordCodec::with_definitions(structs, enums).from_native(item)?;
            if index == 0 {
                Ok(Ok(item))
            } else {
                Ok(Err(item))
            }
        }
        Value::Result { value, .. } => match value {
            Ok(value) => {
                let value =
                    Rc::try_unwrap(value).map_err(|_| "Result payload is shared".to_owned())?;
                super::record_codec::restore_owned_nominal(value, structs, enums).map(Ok)
            }
            Err(value) => {
                let value =
                    Rc::try_unwrap(value).map_err(|_| "Result payload is shared".to_owned())?;
                super::record_codec::restore_owned_nominal(value, structs, enums).map(Err)
            }
        },
        value => Err(format!("expected Result, found {}", value.type_name())),
    }
}

fn read_item(payload: &DynamicValue) -> Result<Value, String> {
    let item =
        crate::value::runtime_layouts::clone_borrowed_view(payload.view().variant_payload()?)?;
    NativeRecordCodec::new().from_native(item)
}

type ResultView = (Result<Value, Value>, Type, Type);

pub fn view(value: &Value) -> Option<Result<ResultView, String>> {
    let Value::Dynamic(object) = value else {
        return None;
    };
    let Type::Result(ok_type, error_type) = object.descriptor().layout().rils_type() else {
        return None;
    };
    let ok_type = (**ok_type).clone();
    let error_type = (**error_type).clone();
    Some(
        object
            .with(|payload| {
                let index = payload.variant_index()?;
                let item = read_item(payload)?;
                Ok((
                    if index == 0 { Ok(item) } else { Err(item) },
                    ok_type,
                    error_type,
                ))
            })
            .and_then(|result| result),
    )
}

pub fn materialize(value: &Value) -> Option<Result<Value, String>> {
    view(value).map(|result| {
        result.map(|(branch, ok_type, error_type)| Value::Result {
            value: branch.map(Rc::new).map_err(Rc::new),
            ok_type: Some(ok_type),
            error_type: Some(error_type),
        })
    })
}
