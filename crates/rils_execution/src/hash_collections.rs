// HashKey compares and hashes its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use crate::value::borrowed::with_legacy;

use crate::{
    types::{Type, merge_types},
    value::{
        FieldSlot, HashKey, HashMapValue, HashSetValue, IndexedStorage, KeyIdentity,
        OwnedIteratorValue, Value,
    },
};

pub(crate) fn call_map(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing HashMap receiver")?;
    if matches!(method, "clear" | "insert" | "remove")
        && !matches!(receiver, Value::Reference(reference) if reference.mutable)
    {
        return Err("HashMap mutation requires a mutable reference".into());
    }
    with_legacy(receiver, |value| {
        let Value::HashMap(map) = value else {
            return Err("expected HashMap receiver".into());
        };
        match method {
            "len" => Ok(crate::numeric::native_usize(
                map.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .len(),
            )),
            "is_empty" => Ok(Value::Bool(
                map.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .is_empty(),
            )),
            "clear" => {
                reject_referenced_map(map)?;
                map.entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?
                    .clear();
                Ok(Value::Unit)
            }
            "contains_key" => {
                let key = query_argument(
                    arguments,
                    1,
                    &*map
                        .key_type
                        .try_borrow()
                        .map_err(|_| "collection key type is accessed")?,
                )?;
                Ok(Value::Bool(
                    map.entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?
                        .contains_key(&key),
                ))
            }
            "insert" => Err("insert requires owned arguments".into()),
            "get_cloned" => {
                let key = query_argument(
                    arguments,
                    1,
                    &*map
                        .key_type
                        .try_borrow()
                        .map_err(|_| "collection key type is accessed")?,
                )?;
                let value = map
                    .entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .get(&key)
                    .and_then(|slot| slot.value.as_ref())
                    .map(Value::clone_owned)
                    .transpose()?;
                option(
                    value,
                    map.value_type
                        .try_borrow()
                        .map_err(|_| "collection value type is accessed")?
                        .clone(),
                )
            }
            "remove" => {
                reject_referenced_map(map)?;
                let key = query_argument(
                    arguments,
                    1,
                    &*map
                        .key_type
                        .try_borrow()
                        .map_err(|_| "collection key type is accessed")?,
                )?;
                let value_type = map
                    .value_type
                    .try_borrow()
                    .map_err(|_| "collection value type is accessed")?
                    .clone();
                let value = map
                    .entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?
                    .remove(&key)
                    .and_then(|slot| slot.value);
                option(value, value_type)
            }
            "keys_cloned" => Ok(iterator(
                map.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .keys()
                    .map(HashKey::to_value)
                    .collect::<Result<_, _>>()?,
                map.key_type
                    .try_borrow()
                    .map_err(|_| "collection key type is accessed")?
                    .clone(),
            )),
            "values_cloned" => Ok(iterator(
                map.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .values()
                    .map(|slot| {
                        slot.value
                            .as_ref()
                            .ok_or_else(|| "HashMap contains a moved value".to_string())?
                            .clone_owned()
                    })
                    .collect::<Result<_, _>>()?,
                map.value_type
                    .try_borrow()
                    .map_err(|_| "collection value type is accessed")?
                    .clone(),
            )),
            _ => Err(format!("unknown HashMap method `{method}`")),
        }
    })
}

pub(crate) fn call_set(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing HashSet receiver")?;
    if matches!(method, "clear" | "insert" | "remove")
        && !matches!(receiver, Value::Reference(reference) if reference.mutable)
    {
        return Err("HashSet mutation requires a mutable reference".into());
    }
    with_legacy(receiver, |value| {
        let Value::HashSet(set) = value else {
            return Err("expected HashSet receiver".into());
        };
        if matches!(method, "clear" | "insert" | "remove") && set.borrowed.get() > 0 {
            return Err("cannot mutate HashSet while it is borrowed by an iterator".into());
        }
        match method {
            "len" => Ok(crate::numeric::native_usize(
                set.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .len(),
            )),
            "is_empty" => Ok(Value::Bool(
                set.entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?
                    .is_empty(),
            )),
            "clear" => {
                set.entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?
                    .clear();
                Ok(Value::Unit)
            }
            "contains" => {
                let key = query_argument(
                    arguments,
                    1,
                    &*set
                        .element_type
                        .try_borrow()
                        .map_err(|_| "collection element type is accessed")?,
                )?;
                Ok(Value::Bool(
                    set.entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?
                        .contains(&key),
                ))
            }
            "insert" => Err("insert requires owned arguments".into()),
            "remove" => {
                let key = query_argument(
                    arguments,
                    1,
                    &*set
                        .element_type
                        .try_borrow()
                        .map_err(|_| "collection element type is accessed")?,
                )?;
                Ok(Value::Bool(
                    set.entries
                        .try_borrow_mut()
                        .map_err(|_| "collection entries are accessed")?
                        .remove(&key),
                ))
            }
            "is_subset" | "is_superset" | "is_disjoint" => {
                with_legacy(arguments.get(1).ok_or("missing other HashSet")?, |value| {
                    let Value::HashSet(other) = value else {
                        return Err("expected other HashSet".into());
                    };
                    let left = set
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
                    let right = other
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
                    Ok(Value::Bool(match method {
                        "is_subset" => left.is_subset(&right),
                        "is_superset" => left.is_superset(&right),
                        "is_disjoint" => left.is_disjoint(&right),
                        _ => unreachable!(),
                    }))
                })
            }
            "union" | "intersection" | "difference" | "symmetric_difference" => {
                with_legacy(arguments.get(1).ok_or("missing other HashSet")?, |value| {
                    let Value::HashSet(other) = value else {
                        return Err("expected other HashSet".into());
                    };
                    let left = set
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
                    let right = other
                        .entries
                        .try_borrow()
                        .map_err(|_| "collection entries are accessed")?;
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
                    let element_type = merge_types(
                        &*set
                            .element_type
                            .try_borrow()
                            .map_err(|_| "collection element type is accessed")?,
                        &*other
                            .element_type
                            .try_borrow()
                            .map_err(|_| "collection element type is accessed")?,
                    )
                    .ok_or_else(|| "HashSet element types do not match".to_string())?;
                    Ok(Value::HashSet(Rc::new(HashSetValue {
                        borrowed: std::cell::Cell::new(0),
                        entries: RefCell::new(entries),
                        element_type: RefCell::new(element_type),
                    })))
                })
            }
            _ => Err(format!("unknown HashSet method `{method}`")),
        }
    })
}

pub(crate) fn into_iter_map(map: Rc<HashMapValue>) -> Result<Value, String> {
    reject_referenced_map(&map)?;
    let key_type = map
        .key_type
        .try_borrow()
        .map_err(|_| "collection key type is accessed")?
        .clone();
    let value_type = map
        .value_type
        .try_borrow()
        .map_err(|_| "collection value type is accessed")?
        .clone();
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
    let element_type = set
        .element_type
        .try_borrow()
        .map_err(|_| "collection element type is accessed")?
        .clone();
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
