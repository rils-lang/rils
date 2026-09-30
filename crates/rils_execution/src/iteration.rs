//! Shared dispatch for built-in iterable values and iterators.

use std::rc::Rc;

use crate::{
    Type, hash_collections, runtime_builtins,
    value::{
        DynamicObject, IndexedStorage, OwnedIteratorValue, Value, native_ops,
        record_codec::NativeRecordCodec,
    },
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
        },
    )
}

pub fn into_iterator_with_context(
    value: Value,
    context: &runtime_builtins::NativeOwnedContext,
) -> Result<IntoIteratorResult, String> {
    let iterator = match value {
        value @ Value::Native(_) if native_ops::is_iterator(&value) => value,
        value @ (Value::OwnedIterator(_)
        | Value::BorrowedIndexedIterator(_)
        | Value::BorrowedMapIterator(_)
        | Value::BorrowedSetIterator(_)) => value,
        Value::Array(storage) | Value::Vec(storage) => owned_indexed_iterator(storage)?,
        Value::VecDeque(queue) => {
            let element_type = queue.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let items = std::mem::take(&mut *queue.elements.borrow_mut());
            Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(items, element_type)))
        }
        Value::BinaryHeap(heap) => {
            let element_type = heap.element_type.borrow().clone().unwrap_or(Type::Unknown);
            let items = std::mem::take(&mut *heap.elements.borrow_mut());
            Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(
                items.into(),
                element_type,
            )))
        }
        Value::Dynamic(object)
            if crate::value::native_layouts::vec::matches(
                object.descriptor().layout().rils_type(),
            ) =>
        {
            runtime_builtins::vector_dynamic::into_iterator_with_context(object, context)?
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
        Value::HashMap(map) => hash_collections::into_iter_map(map)?,
        Value::BTreeMap(map) => runtime_builtins::btree_map::into_iter(map)?,
        Value::BTreeSet(set) => runtime_builtins::btree_set::into_iter(set)?,
        Value::HashSet(set) => hash_collections::into_iter_set(set)?,
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
        .rils_type()
        .clone();
    let items = object.with_mut(|payload| payload.take_all_sequence_items())??;
    let codec = NativeRecordCodec::with_definitions(&context.structs, &context.enums);
    let mut iterator = OwnedIteratorValue::from_native(items, item_type, codec);
    if let Some(iterator_type) = iterator_type {
        iterator = iterator.with_iterator_type(iterator_type);
    }
    Ok(Value::OwnedIterator(Rc::new(iterator)))
}

pub(crate) fn declared_iterator_type(collection_type: &Type) -> Option<Type> {
    let method =
        rils_frontend::standard_library::builtin_member_type(collection_type, "into_iter")?;
    match method {
        Type::Function { return_type, .. } => Some(*return_type),
        _ => None,
    }
}

pub(crate) fn generated_collection_iterator(
    items: impl Iterator<Item = Value> + 'static,
    item_type: Type,
    collection_type: &Type,
) -> Value {
    let mut iterator = OwnedIteratorValue::from_generator(items, item_type);
    if let Some(iterator_type) = declared_iterator_type(collection_type) {
        iterator = iterator.with_iterator_type(iterator_type);
    }
    Value::OwnedIterator(Rc::new(iterator))
}

fn owned_indexed_iterator(storage: Rc<IndexedStorage>) -> Result<Value, String> {
    if storage.active_iterators.get() > 0
        || storage
            .elements
            .borrow()
            .iter()
            .any(|slot| slot.references > 0)
    {
        return Err("cannot iterate a collection while an element is referenced".into());
    }
    if storage
        .elements
        .borrow()
        .iter()
        .any(|slot| slot.value.is_none())
    {
        return Err("cannot iterate a partially moved collection".into());
    }
    let element_type = storage
        .element_type
        .borrow()
        .clone()
        .unwrap_or(Type::Unknown);
    Ok(Value::OwnedIterator(Rc::new(
        OwnedIteratorValue::from_indexed(storage, element_type),
    )))
}

/// Returns `None` only when a user-defined iterator method must be dispatched.
pub fn next_builtin(value: &mut Value) -> Option<Result<Option<Value>, String>> {
    match value {
        Value::Native(object) => native_ops::next(object),
        Value::OwnedIterator(iterator) => Some(iterator.next()),
        Value::BorrowedIndexedIterator(iterator) => Some(iterator.next()),
        Value::BorrowedMapIterator(iterator) => Some(iterator.next()),
        Value::BorrowedSetIterator(iterator) => Some(iterator.next()),
        _ => None,
    }
}
