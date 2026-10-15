//! Consume fixed native aggregates while preserving element layouts and declarations.
use super::{DynamicObject, record_codec::NativeRecordCodec};
use crate::{Type, runtime_builtins::NativeOwnedContext};
use rils_value::DynamicValue;

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

pub(crate) fn into_cursor(
    object: DynamicObject,
    context: &NativeOwnedContext,
) -> Result<(rils_value::OwnedElements, NativeRecordCodec), String> {
    let (payload, codec, element, _) = consume(object, context)?;
    let cursor = rils_value::OwnedElements::from_array(
        payload,
        codec.resolve_layout(&element, &context.hosts)?,
    )?;
    Ok((cursor, codec))
}
