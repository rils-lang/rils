//! Consume fixed native aggregates while preserving element layouts and declarations.
use super::{DynamicObject, Value, record_codec::NativeRecordCodec};
use crate::{Type, runtime_builtins::NativeOwnedContext};
use rils_value::DynamicValue;
use std::rc::Rc;

fn consume(
    object: DynamicObject,
    context: &NativeOwnedContext,
) -> Result<(DynamicValue, NativeRecordCodec, Type, usize), String> {
    let Type::Array { element, length } = object.descriptor().layout().rils_type() else {
        return Err("expected native array".into());
    };
    let element = element.as_ref().clone();
    let length = *length;
    if !element.is_concrete_type() {
        return Err("native array requires a complete element type".into());
    }
    object.with(|payload| {
        if payload.view().is_partially_moved()? {
            return Err("cannot consume a partially moved array".into());
        }
        if payload.has_path_references() {
            return Err("cannot consume an array while an element is referenced".into());
        }
        Ok::<_, String>(())
    })??;
    let codec = super::sum::codec(&object, &context.structs, &context.enums)?;
    let payload = match object.into_value() {
        Ok(payload) => payload,
        Err(failure) if failure.0.descriptor().layout().is_copy() => {
            failure.0.with(DynamicValue::copy_owned)??
        }
        Err(failure) => return Err(failure.1),
    };
    Ok((payload, codec, element, length))
}

pub(crate) fn into_iterator(
    object: DynamicObject,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    let ty = object.descriptor().layout().rils_type().clone();
    let (mut payload, codec, element, length) = consume(object, context)?;
    let items = (0..length)
        .map(|index| payload.take_field(index))
        .collect::<Result<Vec<_>, _>>()?;
    let mut iterator = super::OwnedIteratorValue::from_native(
        items,
        codec.resolve_layout(&element, &context.hosts)?,
        codec,
    )?;
    if let Some(ty) = crate::iteration::declared_iterator_type(&ty) {
        iterator = iterator.with_iterator_type(ty);
    }
    Ok(Value::from_owned_iterator(iterator))
}

pub(crate) fn into_vec(
    object: DynamicObject,
    context: &NativeOwnedContext,
) -> Result<Value, String> {
    let (mut payload, codec, element, length) = consume(object, context)?;
    let ty = Type::Named {
        name: "Vec".into(),
        arguments: vec![element],
    };
    let layout = codec.resolve_layout(&ty, &context.hosts)?;
    let items = (0..length)
        .map(|index| payload.take_field(index))
        .collect::<Result<Vec<_>, _>>()?;
    super::native_instance::from_native(DynamicValue::sequence(layout, items)?, Rc::new(codec))
}
