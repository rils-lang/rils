//! Borrow readers retain original storage guards across recursive inspections.

use rils_value::{DynamicValueRef, NativeLeafRef};

use super::Value;

pub(crate) fn native_codec(
    value: &Value,
) -> Result<Option<std::rc::Rc<super::record_codec::NativeRecordCodec>>, String> {
    match value {
        __reference_value if __reference_value.as_reference().is_some() => {
            let reference = __reference_value
                .as_reference()
                .expect("checked reference handle");
            reference.native_codec()
        }
        Value::Dynamic(object) => Ok(object.descriptor().metadata()),

        _ => Ok(None),
    }
}

pub(crate) enum Read<'a> {
    View(DynamicValueRef<'a>),
    Leaf(NativeLeafRef<'a>),
    Host(&'a std::rc::Rc<super::HostObject>),
}

pub(crate) fn with_read<R>(
    value: &Value,
    dereference: bool,
    callback: impl for<'a> FnOnce(Read<'a>) -> R,
) -> Result<R, String> {
    if dereference && let Some(reference) = value.as_reference() {
        return reference.with_borrowed_target(callback);
    }
    with_raw_read(value, callback)
}

/// Follow receiver references while retaining every original storage guard.
/// Element comparisons use `with_read` instead to preserve reference identity.
pub(crate) fn with_referent<R>(
    value: &Value,
    callback: impl for<'a> FnOnce(Read<'a>) -> R,
) -> Result<R, String> {
    let mut callback = Some(callback);
    let mut output = None;
    with_raw_read(value, |target| {
        dereference_read(target, &mut |target| {
            output = Some(callback.take().expect("called once")(target));
        })
    })??;
    Ok(output.expect("borrow reader was called"))
}

pub(crate) fn with_raw_read<R>(
    value: &Value,
    callback: impl for<'a> FnOnce(Read<'a>) -> R,
) -> Result<R, String> {
    match value {
        Value::Dynamic(object) => object.with(|value| callback(Read::View(value.view()))),
        Value::Native(object) => object.with_leaf(|leaf| callback(Read::Leaf(leaf))),
        Value::HostObject(object) => Ok(callback(Read::Host(object))),
    }
}

fn dereference_read(value: Read<'_>, callback: &mut dyn FnMut(Read<'_>)) -> Result<(), String> {
    use super::ReferenceValue;
    use std::rc::Rc;
    match value {
        Read::View(view) if view.layout()?.is_rust_type::<Rc<ReferenceValue>>() => {
            view.with_rust::<Rc<ReferenceValue>, _>(|reference| {
                reference.with_borrowed_target(|target| dereference_read(target, callback))
            })???;
        }
        Read::Leaf(leaf) if leaf.is_rust_type::<Rc<ReferenceValue>>() => {
            leaf.with_rust::<Rc<ReferenceValue>, _>(|reference| {
                reference.with_borrowed_target(|target| dereference_read(target, callback))
            })???;
        }
        value => callback(value),
    }
    Ok(())
}
