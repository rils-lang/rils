//! Inspect and transfer sum payloads while retaining their native layout.

use std::rc::Rc;

use rils_value::{DynamicPathStep, DynamicValueRef};

use crate::Type;

use super::{
    DynamicObject, EnumType, StructType, Value,
    borrowed_sum::{self, Branch},
    native_instance::NativeInstancePlace,
    record_codec::NativeRecordCodec,
};

pub(crate) fn codec(
    object: &DynamicObject,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<NativeRecordCodec, String> {
    let mut codec = NativeRecordCodec::with_definitions(structs, enums);
    if let Some(source) = object.descriptor().metadata::<NativeRecordCodec>() {
        codec.retain_layout_declarations(&source, object.descriptor().layout())?;
    }
    Ok(codec)
}

/// Read only the discriminant; no child is copied or cloned.
pub fn branch(value: &Value) -> Result<Option<Branch>, String> {
    if let Value::Dynamic(object) = value {
        return object.with(|payload| branch_view(payload.view()))?;
    }
    borrowed_sum::branch(value)
}

fn branch_view(view: DynamicValueRef<'_>) -> Result<Option<Branch>, String> {
    Ok(match view.layout()?.rils_type() {
        Type::Option(_) => Some(if view.option_is_some()? {
            Branch::Some
        } else {
            Branch::None
        }),
        Type::Result(_, _) => Some(match view.variant_index()? {
            0 => Branch::Ok,
            1 => Branch::Err,
            _ => return Err("Result has an invalid variant tag".into()),
        }),
        _ => None,
    })
}

fn step(branch: Branch) -> Result<DynamicPathStep, String> {
    match branch {
        Branch::Some => Ok(DynamicPathStep::Some),
        Branch::Ok => Ok(DynamicPathStep::Variant(0)),
        Branch::Err => Ok(DynamicPathStep::Variant(1)),
        Branch::None => Err("None has no payload".into()),
    }
}

fn owner(object: &DynamicObject) -> Result<NativeInstancePlace, String> {
    NativeInstancePlace::with_codec(
        object.clone().into_shared(),
        Rc::new(codec(object, &[], &[])?),
    )
}

/// Project the active payload for a pattern probe without cloning its bytes.
pub fn borrow_payload(value: &Value, expected: Branch) -> Result<Value, String> {
    if branch(value)? != Some(expected) {
        return Err("sum payload does not match its branch".into());
    }
    match value {
        Value::Dynamic(object) => Ok(Value::Reference(Rc::new(
            owner(object)?
                .project(step(expected)?)?
                .borrow(false, None)?,
        ))),
        Value::Reference(_) => borrowed_sum::payload(value, expected),
        _ => Err("sum payload has no native storage".into()),
    }
}

/// Move a bound payload from an owned sum. Borrowed sums retain their source.
pub fn bind_payload(value: &Value, expected: Branch) -> Result<Value, String> {
    if branch(value)? != Some(expected) {
        return Err("sum payload does not match its branch".into());
    }
    match value {
        Value::Dynamic(object) => {
            let codec = codec(object, &[], &[])?;
            let payload = if object.descriptor().layout().is_copy() {
                object.with(|value| value.view_path(&[step(expected)?])?.copy_owned())??
            } else {
                object
                    .with_mut(|value| value.take_active_payload())??
                    .ok_or("None has no payload")?
                    .1
            };
            codec.from_native(payload)
        }
        _ => borrow_payload(value, expected),
    }
}

/// `?` consumes only Ok. Err keeps the original storage until the caller's
/// return declaration supplies the destination's complete generic arguments.
pub fn try_result(
    value: Value,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
) -> Result<Result<Value, Value>, String> {
    match &value {
        Value::Result { value: Err(_), .. } => return Ok(Err(value)),
        Value::Result { value: Ok(_), .. } => {}
        _ => match branch(&value)? {
            Some(Branch::Err) => return Ok(Err(value)),
            Some(Branch::Ok) => {}
            _ => {
                return Err(format!(
                    "the `?` operator requires Result, found {}",
                    value.type_name()
                ));
            }
        },
    }
    super::dynamic_result::take_owned_with_definitions(value, structs, enums)
}
