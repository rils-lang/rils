//! Runtime-composed storage for options whose items have registered Rust layouts.

use std::rc::Rc;

use rils_stdlib::stdlib::string::String as NativeString;
use rils_value::{DynamicLayout, DynamicType, DynamicValue};

use crate::Type;

use super::{
    DynamicObject, Value, native_layouts, record_codec::NativeRecordCodec,
    record_layout::RecordLayoutResolver,
};

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
    let layout = object.descriptor().layout_handle();
    let Type::Option(item_type) = layout.rils_type() else {
        return Err("dynamic value is not an option".into());
    };
    let item_type = item_type.as_ref().clone();
    object.with_mut(|payload| {
        let absent = DynamicValue::none(layout.clone())?;
        let owned = std::mem::replace(payload, absent);
        let mut codec = NativeRecordCodec::new();
        let legacy = codec.from_native(owned)?;
        let result = match &legacy {
            Value::Option { value, .. } => value
                .as_ref()
                .map(|item| {
                    let cloned = item.clone_owned()?;
                    if !matches!(item_type, Type::Option(_)) {
                        return Ok(cloned);
                    }
                    let item_layout = RecordLayoutResolver::new(&[]).resolve(&item_type)?;
                    let native = codec.into_native(cloned, item_layout.clone())?;
                    DynamicObject::new(Rc::new(DynamicType::new(item_layout)), native)
                        .map(Value::Dynamic)
                })
                .transpose(),
            _ => Err("native option decoded to a different value kind".into()),
        };
        let restored = codec.into_native(legacy, layout)?;
        *payload = restored;
        result
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
