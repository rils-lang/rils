//! Native storage for results with concrete, readable standard-library payloads.

use std::rc::Rc;

use rils_value::{DynamicType, DynamicValue};

use crate::Type;

use super::{
    DynamicObject, Value, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
};

/// Promote a result after both generic arguments have been determined.
pub fn promote(value: Value, expected: &Type) -> Result<Value, String> {
    let Type::Result(ok_type, error_type) = expected else {
        return Ok(value);
    };
    let Ok(layout) = RecordLayoutResolver::new(&[]).resolve(expected) else {
        return Ok(value);
    };
    let alternatives = layout
        .variant_alternatives()
        .ok_or("Result has no variant layout")?;
    let Value::Result { value: branch, .. } = value else {
        return Ok(value);
    };
    let (index, item) = match branch {
        Ok(item) => match Rc::try_unwrap(item) {
            Ok(item) => (0, item),
            Err(item) => {
                return Ok(Value::Result {
                    value: Ok(item),
                    ok_type: Some((**ok_type).clone()),
                    error_type: Some((**error_type).clone()),
                });
            }
        },
        Err(item) => match Rc::try_unwrap(item) {
            Ok(item) => (1, item),
            Err(item) => {
                return Ok(Value::Result {
                    value: Err(item),
                    ok_type: Some((**ok_type).clone()),
                    error_type: Some((**error_type).clone()),
                });
            }
        },
    };
    let item = NativeRecordCodec::new().into_native(item, alternatives[index].clone())?;
    let payload = DynamicValue::variant(layout.clone(), index, item)?;
    let object = DynamicObject::new(Rc::new(DynamicType::new(layout)), payload)?;
    Ok(Value::Dynamic(object))
}

/// Consume a Result and transfer its active branch without cloning its payload.
pub fn take_owned(value: Value) -> Result<Result<Value, Value>, String> {
    match value {
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Result(_, _)) =>
        {
            let value = object.into_value().map_err(|failure| failure.1)?;
            let (index, item) = value.take_variant()?;
            let item = NativeRecordCodec::new().from_native(item)?;
            if index == 0 {
                Ok(Ok(item))
            } else {
                Ok(Err(item))
            }
        }
        Value::Result { value, .. } => match value {
            Ok(value) => Rc::try_unwrap(value)
                .map(Ok)
                .map_err(|_| "Result payload is shared".into()),
            Err(value) => Rc::try_unwrap(value)
                .map(Err)
                .map_err(|_| "Result payload is shared".into()),
        },
        value => Err(format!("expected Result, found {}", value.type_name())),
    }
}

fn read_item(payload: &DynamicValue) -> Result<Value, String> {
    let item =
        rils_stdlib::native::registry().clone_borrowed_view(payload.view().variant_payload()?)?;
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
