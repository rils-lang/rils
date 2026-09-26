//! Runtime-composed storage for options whose items have registered Rust layouts.

use std::rc::Rc;

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicType, DynamicValue};

use crate::Type;

use super::{DynamicObject, Value, native_layouts};

/// Construct an Option with a concrete native item layout when one is available.
/// Unsupported item types remain on the existing owned-value path.
pub fn construct(value: Option<&Value>, item_type: &Type) -> Option<Result<Value, String>> {
    let item_layout = match item_type.clone() {
        Type::I32 | Type::USIZE => native_layouts::integer::layout(item_type)?,
        Type::String => native_layouts::string::layout(),
        _ => return None,
    };
    let option_layout = match native_layouts::option::layout(item_layout.clone()) {
        Ok(layout) => layout,
        Err(error) => return Some(Err(error)),
    };
    let item = match (item_type.clone(), value) {
        (_, None) => None,
        (Type::I32, Some(value)) => Some(DynamicValue::from_rust(item_layout, value.as_i32()?)),
        (Type::USIZE, Some(value)) => Some(DynamicValue::from_rust(item_layout, value.as_usize()?)),
        (Type::String, Some(value)) => Some(DynamicValue::from_rust(
            item_layout,
            NativeString::from(value.as_string()?),
        )),
        _ => return None,
    };
    Some(
        item.map_or_else(
            || DynamicValue::none(option_layout.clone()),
            |item| DynamicValue::some(option_layout.clone(), item?),
        )
        .and_then(|value| {
            let descriptor = Rc::new(DynamicType::new(option_layout));
            DynamicObject::new(descriptor, value).map(Value::Dynamic)
        }),
    )
}

pub fn view(value: &Value) -> Option<Result<(Option<Value>, Type), String>> {
    let Value::Dynamic(object) = value else {
        return None;
    };
    let Type::Option(item_type) = object.descriptor().layout().rils_type() else {
        return None;
    };
    let item_type = item_type.as_ref().clone();
    let item = match item_type {
        Type::I32 => object.with(|value| {
            value.with_option::<i32, _>(|item| item.copied().map(crate::numeric::native_i32))
        }),
        Type::USIZE => object.with(|value| {
            value.with_option::<usize, _>(|item| item.copied().map(crate::numeric::native_usize))
        }),
        Type::String => object.with(|value| {
            value.with_option::<NativeString, _>(|item| {
                item.map(|text| super::native_string(std::string::String::from(text.clone())))
            })
        }),
        _ => return None,
    };
    Some(item.and_then(|item| item).map(|item| (item, item_type)))
}

pub fn view_any(value: &Value) -> Option<Result<(Option<Value>, Type), String>> {
    match value {
        Value::Option {
            value,
            element_type,
        } => Some(Ok((
            value.as_ref().map(|value| value.as_ref().clone()),
            element_type.clone().unwrap_or(Type::Unknown),
        ))),
        Value::Dynamic(_) => view(value),
        _ => None,
    }
}

pub fn materialize(value: &Value) -> Option<Result<Value, String>> {
    view(value).map(|result| {
        result.map(|(item, item_type)| Value::Option {
            value: item.map(Rc::new),
            element_type: Some(item_type),
        })
    })
}
