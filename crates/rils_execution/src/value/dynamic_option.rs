//! Runtime-composed storage for options whose items have registered Rust layouts.

use std::rc::Rc;

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use crate::Type;

use super::{
    DynamicObject, EnumType, StructType, Value, native_layouts, record_codec::NativeRecordCodec,
    record_layout::RecordLayoutResolver,
};

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

/// A native option, or the original item when its type has no native layout.
pub enum Construction {
    Native(Value),
    Unsupported(Option<Value>),
}

fn item_layout(item_type: &Type) -> Option<Rc<DynamicLayout>> {
    RecordLayoutResolver::new(&[]).resolve(item_type).ok()
}

pub fn supports(item_type: &Type) -> bool {
    item_layout(item_type).is_some()
}

/// Consume an item to construct an Option with its concrete native layout.
/// Unsupported types return the original item for the legacy value path.
pub fn construct(value: Option<Value>, item_type: &Type) -> Result<Construction, String> {
    let Some(item_layout) = item_layout(item_type) else {
        return Ok(Construction::Unsupported(value));
    };
    let option_layout = native_layouts::option::layout(item_layout.clone())?;
    let item = value
        .map(|value| NativeRecordCodec::new().into_native(value, item_layout))
        .transpose()?;
    item.map_or_else(
        || DynamicValue::none(option_layout.clone()),
        |item| DynamicValue::some(option_layout.clone(), item),
    )
    .and_then(|value| {
        let descriptor = Rc::new(DynamicType::new(option_layout));
        DynamicObject::new(descriptor, value)
            .map(|object| Construction::Native(Value::Dynamic(object)))
    })
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
            if object
                .descriptor()
                .has_owned_operation(super::owned_sum::DECODE_OPERATION)
            {
                let value = object.call_owned(super::owned_sum::DECODE_OPERATION)?;
                return take_owned_with_definitions(value, structs, enums);
            }
            let value = object.into_value().map_err(|failure| failure.1)?;
            value
                .take_option()?
                .map(|item| {
                    if matches!(item.descriptor().rils_type(), Type::Option(_)) {
                        let layout = item.layout_handle();
                        let descriptor = Rc::new(DynamicType::new(layout));
                        DynamicObject::new(descriptor, item).map(Value::Dynamic)
                    } else {
                        NativeRecordCodec::with_definitions(structs, enums).from_native(item)
                    }
                })
                .transpose()
        }
        Value::Option { value, .. } => value
            .map(|value| {
                let value =
                    Rc::try_unwrap(value).map_err(|_| "Option item is shared".to_owned())?;
                super::record_codec::restore_owned_nominal(value, structs, enums)
            })
            .transpose(),
        value => Err(format!("expected Option, found {}", value.type_name())),
    }
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
        })
        .unwrap_or_else(|| view_composite(object));
    Some(item.map(|item| (item, item_type)))
}

fn view_composite(object: &DynamicObject) -> Result<Option<Value>, String> {
    let Type::Option(item_type) = object.descriptor().layout().rils_type() else {
        return Err("dynamic value is not an option".into());
    };
    let nested_option = matches!(item_type.as_ref(), Type::Option(_));
    object.with(|payload| {
        let view = payload.view();
        if !view.option_is_some()? {
            return Ok(None);
        }
        let item = crate::value::runtime_layouts::clone_borrowed_view(view.option_item()?)?;
        let value = if nested_option {
            let layout = item.layout_handle();
            Value::Dynamic(DynamicObject::new(Rc::new(DynamicType::new(layout)), item)?)
        } else {
            NativeRecordCodec::new().from_native(item)?
        };
        Ok(Some(value))
    })?
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
