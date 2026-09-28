//! Native storage for results with concrete, readable standard-library payloads.

use std::rc::Rc;

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicType, DynamicValue};

use crate::{FloatType, IntegerType, Type};

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
    if !alternatives
        .iter()
        .all(|child| child.is_copy() || child.rils_type() == &Type::String)
    {
        return Ok(value);
    }
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

fn read_item(payload: &DynamicValue, index: usize, ty: &Type) -> Result<Value, String> {
    macro_rules! scalar {
        ($rust:ty, $construct:expr) => {
            payload.with_variant::<$rust, _>(index, |item| $construct(*item))
        };
    }
    match ty {
        Type::Unit => scalar!((), |_| Value::Unit),
        Type::Bool => scalar!(bool, Value::Bool),
        Type::Char => scalar!(char, Value::Char),
        Type::Integer(IntegerType::I8) => scalar!(i8, Value::from_i8),
        Type::Integer(IntegerType::I16) => scalar!(i16, Value::from_i16),
        Type::Integer(IntegerType::I32) => scalar!(i32, Value::from_i32),
        Type::Integer(IntegerType::I64) => scalar!(i64, Value::from_i64),
        Type::Integer(IntegerType::I128) => scalar!(i128, Value::from_i128),
        Type::Integer(IntegerType::Isize) => scalar!(isize, Value::from_isize),
        Type::Integer(IntegerType::U8) => scalar!(u8, Value::from_u8),
        Type::Integer(IntegerType::U16) => scalar!(u16, Value::from_u16),
        Type::Integer(IntegerType::U32) => scalar!(u32, Value::from_u32),
        Type::Integer(IntegerType::U64) => scalar!(u64, Value::from_u64),
        Type::Integer(IntegerType::U128) => scalar!(u128, Value::from_u128),
        Type::Integer(IntegerType::Usize) => scalar!(usize, Value::from_usize),
        Type::Float(FloatType::F32) => scalar!(f32, Value::from_f32),
        Type::Float(FloatType::F64) => scalar!(f64, Value::from_f64),
        Type::String => payload.with_variant::<NativeString, _>(index, |item| {
            Value::from_string(std::string::String::from(item.clone()))
        }),
        _ => NativeRecordCodec::new().from_native(payload.copy_variant_payload()?),
    }
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
                let item = read_item(
                    payload,
                    index,
                    if index == 0 { &ok_type } else { &error_type },
                )?;
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
