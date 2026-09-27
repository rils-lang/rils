//! Owned bridge for collection values with a declaration-derived sequence layout.

use std::rc::Rc;

use rils_value::{DynamicType, DynamicValue};

use crate::Type;

use super::{DynamicObject, Value, record_codec, record_layout::RecordLayoutResolver};

/// An unshared collection can acquire its concrete layout when a binding gives
/// an otherwise untyped constructor its type argument.
pub fn promote_empty(value: Value, expected: &Type) -> Value {
    let supported = match (&value, expected) {
        (Value::VecDeque(_), Type::Named { name, arguments }) => {
            name == "VecDeque" && arguments.len() == 1
        }
        (Value::BinaryHeap(_), Type::Named { name, arguments }) => {
            name == "BinaryHeap" && arguments.len() == 1
        }
        _ => false,
    };
    if !supported {
        return value;
    }
    let empty_and_unique = match &value {
        Value::VecDeque(queue) => {
            Rc::strong_count(queue) == 1 && queue.elements.borrow().is_empty()
        }
        Value::BinaryHeap(heap) => Rc::strong_count(heap) == 1 && heap.elements.borrow().is_empty(),
        _ => false,
    };
    if !empty_and_unique {
        return value;
    }
    let Ok(layout) = RecordLayoutResolver::new(&[]).resolve(expected) else {
        return value;
    };
    let Ok(payload) = DynamicValue::sequence(layout.clone(), Vec::new()) else {
        return value;
    };
    let descriptor = Rc::new(DynamicType::new(layout));
    match DynamicObject::new(descriptor, payload) {
        Ok(object) => Value::Dynamic(object),
        Err(_) => value,
    }
}

/// Invoke an existing owned collection operation while retaining native
/// storage between calls. The codec transfers every element; no Value is
/// stored inside the dynamic sequence payload.
pub fn with_legacy<R>(
    object: &DynamicObject,
    operation: impl FnOnce(&Value) -> Result<R, String>,
) -> Result<R, String> {
    let layout = object.descriptor().layout_handle();
    object.with_mut(|payload| {
        let empty = DynamicValue::sequence(layout.clone(), Vec::new())?;
        let owned = std::mem::replace(payload, empty);
        let mut codec = record_codec::NativeRecordCodec::new();
        let legacy = codec.from_native(owned)?;
        let result = operation(&legacy);
        let restored = codec.into_native(legacy, layout)?;
        *payload = restored;
        result
    })?
}

pub fn clone_owned(object: &DynamicObject) -> Result<DynamicObject, String> {
    let ty = object.descriptor().layout().rils_type();
    if !super::native_layouts::vec_deque::matches(ty)
        && !super::native_layouts::binary_heap::matches(ty)
    {
        return Err(format!("dynamic value {ty} does not support Clone"));
    }
    let layout = object.descriptor().layout_handle();
    let cloned = with_legacy(object, Value::clone_owned)?;
    let payload = record_codec::into_native(cloned, layout.clone())?;
    DynamicObject::new(Rc::new(DynamicType::new(layout)), payload)
}
