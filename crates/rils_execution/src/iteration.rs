//! Shared dispatch for built-in iterable values and iterators.

use std::rc::Rc;

use crate::{
    Type, hash_collections, runtime_builtins,
    value::{IndexedStorage, OwnedIteratorValue, Value, native_ops},
};

pub enum IntoIteratorResult {
    Ready(Value),
    UserDefined(Value),
}

pub fn into_iterator(value: Value) -> Result<IntoIteratorResult, String> {
    let iterator = match value {
        value @ Value::Native(_) if native_ops::is_iterator(&value) => value,
        value @ (Value::OwnedIterator(_)
        | Value::BorrowedIndexedIterator(_)
        | Value::BorrowedMapIterator(_)
        | Value::BorrowedSetIterator(_)) => value,
        Value::Array(storage) | Value::Vec(storage) => owned_indexed_iterator(storage)?,
        Value::HashMap(map) => hash_collections::call(
            rils_builtins::BuiltinId::HashMapIntoIter,
            &[Value::HashMap(map)],
        )?,
        Value::BTreeMap(map) => runtime_builtins::call(
            rils_builtins::BuiltinId::BtreeMapIntoIter,
            &[Value::BTreeMap(map)],
        )?,
        Value::BTreeSet(set) => runtime_builtins::call(
            rils_builtins::BuiltinId::BtreeSetIntoIter,
            &[Value::BTreeSet(set)],
        )?,
        Value::HashSet(set) => hash_collections::call(
            rils_builtins::BuiltinId::HashSetIntoIter,
            &[Value::HashSet(set)],
        )?,
        value => return Ok(IntoIteratorResult::UserDefined(value)),
    };
    Ok(IntoIteratorResult::Ready(iterator))
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
