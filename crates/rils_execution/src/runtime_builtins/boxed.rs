//! Owned generic heap indirection backed by a composed native child layout.

use std::{any::Any, rc::Rc};

use rils_value::{DynamicType, DynamicValue};

use crate::{
    Type,
    runtime_builtins::NativeOwnedContext,
    value::{
        DynamicObject, Value, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    },
};

struct BoxPayload {
    value: DynamicValue,
    codec: NativeRecordCodec,
}

pub(super) fn new(
    mut arguments: Vec<Value>,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    let value = arguments.pop().ok_or("Box::new expects one value")?;
    let item_type = Type::of_value(&value).ok_or("Box::new cannot infer the item type")?;
    let box_type = Type::Named {
        name: "Box".into(),
        arguments: vec![item_type.clone()],
    };
    let mut resolver = RecordLayoutResolver::with_enums(&context.structs, &context.enums);
    let item_layout = resolver.resolve(&item_type)?;
    let box_layout = resolver.resolve(&box_type)?;
    let mut codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
    let value = codec.into_native(value, item_layout)?;
    let payload: Box<dyn Any> = Box::new(BoxPayload { value, codec });
    let value = DynamicValue::from_rust(box_layout.clone(), payload)?;
    let descriptor = Rc::new(DynamicType::new(box_layout));
    Ok(Value::Dynamic(DynamicObject::new(descriptor, value)?))
}

pub(super) fn into_inner(
    mut arguments: Vec<Value>,
    _: &NativeOwnedContext,
) -> Result<Value, String> {
    let Some(Value::Dynamic(object)) = arguments.pop() else {
        return Err("Box::into_inner expects a Box receiver".into());
    };
    if !matches!(object.descriptor().layout().rils_type(), Type::Named { name, arguments } if name == "Box" && arguments.len() == 1)
    {
        return Err("Box::into_inner expects a Box receiver".into());
    }
    let value = object.into_value().map_err(|failure| failure.1)?;
    let value = value
        .into_rust::<Box<dyn Any>>()
        .map_err(|failure| failure.1)?;
    let payload = value
        .downcast::<BoxPayload>()
        .map_err(|_| "Box payload has an unexpected native type".to_owned())?;
    payload.codec.from_native(payload.value)
}
