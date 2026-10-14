//! Shared dispatch for built-in iterable values and iterators.

use crate::{
    Type, runtime_builtins,
    value::{DynamicObject, OwnedIteratorValue, Value, native_ops},
};

pub enum IntoIteratorResult {
    Ready(Value),
    UserDefined(Value),
}

pub fn into_iterator(value: Value) -> Result<IntoIteratorResult, String> {
    into_iterator_with_context(
        value,
        &runtime_builtins::NativeOwnedContext {
            structs: Vec::new(),
            enums: Vec::new(),
            ..Default::default()
        },
    )
}

pub fn into_iterator_with_context(
    value: Value,
    context: &runtime_builtins::NativeOwnedContext,
) -> Result<IntoIteratorResult, String> {
    let iterator = match value {
        value @ Value::Native(_) if native_ops::is_iterator(&value) => value,
        Value::Dynamic(object)
            if matches!(object.descriptor().layout().rils_type(), Type::Array { .. }) =>
        {
            crate::value::native_array::into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::vec::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::btree_set::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::vec_deque::matches(
                object.descriptor().layout().rils_type(),
            ) || crate::value::native_layouts::binary_heap::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::hash_set::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::hash_map::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::btree_map::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            native_sequence_into_iterator(object, context)?
        }
        value => return Ok(IntoIteratorResult::UserDefined(value)),
    };
    Ok(IntoIteratorResult::Ready(iterator))
}

pub(crate) fn native_sequence_into_iterator(
    object: DynamicObject,
    context: &runtime_builtins::NativeOwnedContext,
) -> Result<Value, String> {
    let iterator_type = declared_iterator_type(object.descriptor().layout().rils_type());
    let item_type = object
        .descriptor()
        .layout()
        .sequence_item()
        .ok_or("native collection has no item layout")?
        .clone();
    let codec = crate::value::sum::codec(&object, &context.structs, &context.enums)?;
    let items = object.with_mut(|payload| payload.take_all_sequence_items())??;
    let mut iterator = OwnedIteratorValue::from_native(items, item_type, codec)?;
    if let Some(iterator_type) = iterator_type {
        iterator = iterator.with_iterator_type(iterator_type);
    }
    Ok(Value::from_owned_iterator(iterator))
}

pub(crate) fn declared_iterator_type(collection_type: &Type) -> Option<Type> {
    let method =
        rils_frontend::standard_library::builtin_member_type(collection_type, "into_iter")?;
    match method {
        Type::Function { return_type, .. } => Some(*return_type),
        _ => None,
    }
}

/// Returns `None` only when a user-defined iterator method must be dispatched.
pub fn next_builtin(value: &mut Value) -> Option<Result<Option<Value>, String>> {
    match value {
        Value::Native(object) => native_ops::next(object),
        _ => None,
    }
}
