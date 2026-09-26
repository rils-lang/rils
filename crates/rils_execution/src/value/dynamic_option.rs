//! Runtime-composed storage for options whose items have registered Rust layouts.

use std::rc::Rc;

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicType, DynamicValue};

use crate::Type;

use super::{DynamicObject, Value, native_layouts};

/// Construct an Option with a concrete native item layout when one is available.
/// Unsupported item types remain on the existing owned-value path.
pub fn construct(value: Option<&Value>, item_type: &Type) -> Option<Result<Value, String>> {
    let item_layout = native_layouts::integer::layout(item_type)
        .or_else(|| native_layouts::float::layout(item_type))
        .or_else(|| (item_type == &Type::String).then(native_layouts::string::layout))?;
    let option_layout = match native_layouts::option::layout(item_layout.clone()) {
        Ok(layout) => layout,
        Err(error) => return Some(Err(error)),
    };
    let item = value.map(|value| {
        if item_type == &Type::String {
            return DynamicValue::from_rust(
                item_layout.clone(),
                NativeString::from(value.as_string().ok_or("expected string option item")?),
            );
        }
        native_layouts::integer::option_item(value, item_type, item_layout.clone())
            .or_else(|| native_layouts::float::option_item(value, item_type, item_layout.clone()))
            .ok_or_else(|| format!("no native option item converter for {item_type}"))?
    });
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
    let item = native_layouts::integer::option_view(object, &item_type)
        .or_else(|| native_layouts::float::option_view(object, &item_type))
        .or_else(|| {
            (item_type == Type::String).then(|| {
                object
                    .with(|value| {
                        value.with_option::<NativeString, _>(|item| {
                            item.map(|text| {
                                super::native_string(std::string::String::from(text.clone()))
                            })
                        })
                    })
                    .and_then(|item| item)
            })
        })?;
    Some(item.map(|item| (item, item_type)))
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
