//! Move compatibility wrappers without sharing mutable non-Copy key payloads.
use crate::value::{DynamicObject, Value};
use std::rc::Rc;
pub(super) fn own(value: Value) -> Result<Value, String> {
    if matches!(value, Value::Reference(_)) {
        return Err("an owned key cannot contain a reference".into());
    }
    match value {
        Value::Native(object) => object
            .into_unique()
            .map(Value::Native)
            .map_err(|failure| failure.1),
        Value::Dynamic(object) => {
            let descriptor = object.descriptor_handle();
            let payload = match object.into_value() {
                Ok(value) => value,
                Err(failure) if failure.0.descriptor().layout().is_copy() => {
                    failure.0.with(|value| value.copy_owned())??
                }
                Err(failure) => return Err(failure.1),
            };
            DynamicObject::new(descriptor, payload).map(Value::Dynamic)
        }
        Value::Option {
            value,
            element_type,
        } => Ok(Value::Option {
            value: value
                .map(|value| unwrap(value).and_then(own).map(Rc::new))
                .transpose()?,
            element_type,
        }),
        Value::Result {
            value,
            ok_type,
            error_type,
        } => Ok(Value::Result {
            value: match value {
                Ok(value) => Ok(Rc::new(own(unwrap(value)?)?)),
                Err(value) => Err(Rc::new(own(unwrap(value)?)?)),
            },
            ok_type,
            error_type,
        }),
        Value::Tuple(storage) => own_sequence(storage).map(Value::Tuple),
        Value::Array(storage) => own_sequence(storage).map(Value::Array),
        value => Ok(value),
    }
}
fn unwrap(value: Rc<Value>) -> Result<Value, String> {
    Rc::try_unwrap(value).or_else(|value| {
        if value.is_copy() {
            value.clone_owned()
        } else {
            Err("cannot move a shared non-Copy key payload".into())
        }
    })
}
fn own_sequence(
    storage: Rc<crate::value::IndexedStorage>,
) -> Result<Rc<crate::value::IndexedStorage>, String> {
    let storage = Rc::try_unwrap(storage).or_else(|storage| {
        let value = Value::Tuple(storage);
        if !value.is_copy() {
            return Err("cannot move a shared non-Copy key sequence".to_owned());
        }
        let Value::Tuple(copy) = value.clone_owned()? else {
            unreachable!()
        };
        Rc::try_unwrap(copy).map_err(|_| "copied key sequence is shared".to_owned())
    })?;
    if storage.active_iterators.get() != 0 {
        return Err("cannot move a borrowed key sequence".into());
    }
    let mut fields = storage.elements.into_inner();
    for slot in &mut fields {
        if slot.references != 0 {
            return Err("cannot move a referenced key field".into());
        }
        slot.value = Some(own(slot.value.take().ok_or("moved key field")?)?);
    }
    Ok(Rc::new(crate::value::IndexedStorage {
        elements: std::cell::RefCell::new(fields),
        element_type: storage.element_type,
        active_iterators: storage.active_iterators,
    }))
}
