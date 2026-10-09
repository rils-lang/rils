// HashKey compares and hashes its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::{
    types::{Type, merge_types},
    value::{
        FieldSlot, HashKey, HashMapValue, HashSetValue, IndexedStorage, KeyIdentity,
        OwnedIteratorValue, Value,
    },
};

pub(crate) fn call_map(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let map = hash_map(
        arguments
            .first()
            .ok_or_else(|| "missing HashMap receiver".to_string())?,
    )?;
    match method {
        "len" => Ok(crate::numeric::native_usize(map.entries.borrow().len())),
        "is_empty" => Ok(Value::Bool(map.entries.borrow().is_empty())),
        "clear" => {
            reject_referenced_map(&map)?;
            map.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        "contains_key" => {
            let key = query_argument(arguments, 1, &map.key_type.borrow())?;
            Ok(Value::Bool(map.entries.borrow().contains_key(&key)))
        }
        "insert" => Err("insert requires owned arguments".into()),
        "get_cloned" => {
            let key = query_argument(arguments, 1, &map.key_type.borrow())?;
            let value = map
                .entries
                .borrow()
                .get(&key)
                .and_then(|slot| slot.value.as_ref())
                .map(Value::clone_owned)
                .transpose()?;
            option(value, map.value_type.borrow().clone())
        }
        "remove" => {
            reject_referenced_map(&map)?;
            let key = query_argument(arguments, 1, &map.key_type.borrow())?;
            let value = map
                .entries
                .borrow_mut()
                .remove(&key)
                .and_then(|slot| slot.value);
            option(value, map.value_type.borrow().clone())
        }
        "keys_cloned" => Ok(iterator(
            map.entries
                .borrow()
                .keys()
                .map(HashKey::to_value)
                .collect::<Result<_, _>>()?,
            map.key_type.borrow().clone(),
        )),
        "values_cloned" => Ok(iterator(
            map.entries
                .borrow()
                .values()
                .map(|slot| {
                    slot.value
                        .as_ref()
                        .ok_or_else(|| "HashMap contains a moved value".to_string())?
                        .clone_owned()
                })
                .collect::<Result<_, _>>()?,
            map.value_type.borrow().clone(),
        )),
        _ => Err(format!("unknown HashMap method `{method}`")),
    }
}

pub(crate) fn call_set(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let set = hash_set(
        arguments
            .first()
            .ok_or_else(|| "missing HashSet receiver".to_string())?,
    )?;
    if matches!(method, "clear" | "insert" | "remove") && set.borrowed.get() > 0 {
        return Err("cannot mutate HashSet while it is borrowed by an iterator".into());
    }
    match method {
        "len" => Ok(crate::numeric::native_usize(set.entries.borrow().len())),
        "is_empty" => Ok(Value::Bool(set.entries.borrow().is_empty())),
        "clear" => {
            set.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        "contains" => {
            let key = query_argument(arguments, 1, &set.element_type.borrow())?;
            Ok(Value::Bool(set.entries.borrow().contains(&key)))
        }
        "insert" => Err("insert requires owned arguments".into()),
        "remove" => {
            let key = query_argument(arguments, 1, &set.element_type.borrow())?;
            Ok(Value::Bool(set.entries.borrow_mut().remove(&key)))
        }
        "is_subset" | "is_superset" | "is_disjoint" => {
            let other = hash_set(
                arguments
                    .get(1)
                    .ok_or_else(|| "missing other HashSet".to_string())?,
            )?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            Ok(Value::Bool(match method {
                "is_subset" => left.is_subset(&right),
                "is_superset" => left.is_superset(&right),
                "is_disjoint" => left.is_disjoint(&right),
                _ => unreachable!(),
            }))
        }
        "union" | "intersection" | "difference" | "symmetric_difference" => {
            let other = hash_set(
                arguments
                    .get(1)
                    .ok_or_else(|| "missing other HashSet".to_string())?,
            )?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            let entries = match method {
                "union" => left
                    .union(&right)
                    .map(HashKey::clone_owned)
                    .collect::<Result<_, _>>()?,
                "intersection" => left
                    .intersection(&right)
                    .map(HashKey::clone_owned)
                    .collect::<Result<_, _>>()?,
                "difference" => left
                    .difference(&right)
                    .map(HashKey::clone_owned)
                    .collect::<Result<_, _>>()?,
                "symmetric_difference" => left
                    .symmetric_difference(&right)
                    .map(HashKey::clone_owned)
                    .collect::<Result<_, _>>()?,
                _ => unreachable!(),
            };
            let element_type =
                merge_types(&set.element_type.borrow(), &other.element_type.borrow())
                    .ok_or_else(|| "HashSet element types do not match".to_string())?;
            Ok(Value::HashSet(Rc::new(HashSetValue {
                borrowed: std::cell::Cell::new(0),
                entries: RefCell::new(entries),
                element_type: RefCell::new(element_type),
            })))
        }
        _ => Err(format!("unknown HashSet method `{method}`")),
    }
}

pub(crate) fn into_iter_map(map: Rc<HashMapValue>) -> Result<Value, String> {
    reject_referenced_map(&map)?;
    let key_type = map.key_type.borrow().clone();
    let value_type = map.value_type.borrow().clone();
    let mut entries = map
        .entries
        .try_borrow_mut()
        .map_err(|_| "cannot consume HashMap while its entries are accessed")?;
    if entries.values().any(|slot| slot.value.is_none()) {
        return Err("cannot iterate a partially moved HashMap".into());
    }
    for key in entries.keys() {
        key.check_move()?;
    }
    let entries = std::mem::take(&mut *entries);
    let values = entries.into_iter().map(|(key, slot)| {
        Ok(tuple(vec![
            key.into_value()?,
            slot.value.expect("unreferenced HashMap value is present"),
        ]))
    });
    let collection_type = Type::Named {
        name: "HashMap".into(),
        arguments: vec![key_type.clone(), value_type.clone()],
    };
    Ok(crate::iteration::generated_collection_iterator(
        values,
        Type::Tuple(vec![key_type, value_type]),
        &collection_type,
    ))
}

pub(crate) fn into_iter_set(set: Rc<HashSetValue>) -> Result<Value, String> {
    if set.borrowed.get() > 0 {
        return Err("cannot mutate HashSet while it is borrowed by an iterator".into());
    }
    let element_type = set.element_type.borrow().clone();
    let mut entries = set
        .entries
        .try_borrow_mut()
        .map_err(|_| "cannot consume HashSet while its entries are accessed")?;
    for key in entries.iter() {
        key.check_move()?;
    }
    let entries = std::mem::take(&mut *entries);
    let collection_type = Type::Named {
        name: "HashSet".into(),
        arguments: vec![element_type.clone()],
    };
    Ok(crate::iteration::generated_collection_iterator(
        entries.into_iter().map(HashKey::into_value),
        element_type,
        &collection_type,
    ))
}

fn hash_map(value: &Value) -> Result<Rc<HashMapValue>, String> {
    match read(value)? {
        Value::HashMap(map) => Ok(map),
        value => Err(format!("expected HashMap, found {}", value.type_name())),
    }
}

fn hash_set(value: &Value) -> Result<Rc<HashSetValue>, String> {
    match read(value)? {
        Value::HashSet(set) => Ok(set),
        value => Err(format!("expected HashSet, found {}", value.type_name())),
    }
}

fn read(value: &Value) -> Result<Value, String> {
    match value {
        Value::Reference(reference) => reference.read(),
        value => Ok(value.clone()),
    }
}

fn reject_referenced_map(map: &HashMapValue) -> Result<(), String> {
    if map.borrowed.get() > 0
        || map
            .entries
            .try_borrow()
            .map_err(|_| "cannot mutate HashMap while its entries are accessed")?
            .values()
            .any(|slot| {
                slot.references > 0
                    || slot
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
            })
    {
        Err("cannot mutate a HashMap while a value is referenced".into())
    } else {
        Ok(())
    }
}

fn option(value: Option<Value>, element_type: Type) -> Result<Value, String> {
    Ok(Value::Option {
        value: value.map(Rc::new),
        element_type: Some(element_type),
    })
}

fn iterator(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::OwnedIterator(Rc::new(OwnedIteratorValue::from_items(items, element_type)))
}

fn tuple(values: Vec<Value>) -> Value {
    Value::Tuple(Rc::new(IndexedStorage {
        active_iterators: std::cell::Cell::new(0),
        elements: RefCell::new(
            values
                .into_iter()
                .map(|value| FieldSlot::new(Type::of_value(&value).unwrap_or(Type::Unknown), value))
                .collect(),
        ),
        element_type: RefCell::new(None),
    }))
}

fn query_argument(
    arguments: &[Value],
    index: usize,
    expected: &Type,
) -> Result<KeyIdentity, String> {
    KeyIdentity::from_value(
        arguments.get(index).ok_or("missing hash collection key")?,
        Some(expected),
        false,
    )
}
