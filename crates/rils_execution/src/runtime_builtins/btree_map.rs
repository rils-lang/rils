// HashKey orders its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, rc::Rc};

use crate::{
    types::{Type, merge_types},
    value::{BTreeMapValue, FieldSlot, HashKey, IndexedStorage, KeyIdentity, Value},
};

pub(super) fn call(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing BTreeMap receiver")?;
    let mutating = matches!(method, "clear" | "insert" | "remove");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeMap mutation requires a mutable reference".into());
    }
    let Value::BTreeMap(map) = super::import_receiver(receiver)? else {
        return Err("expected BTreeMap receiver".into());
    };
    match method {
        "len" => Ok(crate::numeric::native_usize(map.entries.borrow().len())),
        "is_empty" => Ok(Value::Bool(map.entries.borrow().is_empty())),
        "clear" => {
            reject_referenced(&map)?;
            map.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        "contains_key" => {
            let key = query(arguments, &map.key_type.borrow())?;
            Ok(Value::Bool(map.entries.borrow().contains_key(&key)))
        }
        "insert" => {
            reject_referenced(&map)?;
            let key = key(arguments, 1)?;
            let value = arguments.get(2).ok_or("missing BTreeMap value")?.clone();
            let key_type = merge_types(&map.key_type.borrow(), &key.ty())
                .ok_or("BTreeMap key type mismatch")?;
            let value_type = merge_types(
                &map.value_type.borrow(),
                &Type::of_value(&value).unwrap_or(Type::Unknown),
            )
            .ok_or("BTreeMap value type mismatch")?;
            let previous = map
                .entries
                .borrow_mut()
                .insert(key, FieldSlot::new(value_type.clone(), value));
            *map.key_type.borrow_mut() = key_type;
            *map.value_type.borrow_mut() = value_type.clone();
            option(previous.and_then(|slot| slot.value), value_type)
        }
        "get_cloned" => {
            let key = query(arguments, &map.key_type.borrow())?;
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
            reject_referenced(&map)?;
            let key = query(arguments, &map.key_type.borrow())?;
            let value = map
                .entries
                .borrow_mut()
                .remove(&key)
                .and_then(|slot| slot.value);
            option(value, map.value_type.borrow().clone())
        }
        "first_key_cloned" | "last_key_cloned" => {
            let entries = map.entries.borrow();
            let key = if method == "first_key_cloned" {
                entries.first_key_value()
            } else {
                entries.last_key_value()
            };
            option(
                key.map(|(key, _)| key.to_value()),
                map.key_type.borrow().clone(),
            )
        }
        _ => Err("unsupported BTreeMap operation".into()),
    }
}

pub(crate) fn into_iter(map: Rc<BTreeMapValue>) -> Result<Value, String> {
    reject_referenced(&map)?;
    let key_type = map.key_type.borrow().clone();
    let value_type = map.value_type.borrow().clone();
    let mut entries = map
        .entries
        .try_borrow_mut()
        .map_err(|_| "cannot consume BTreeMap while its entries are accessed")?;
    if entries.values().any(|slot| slot.value.is_none()) {
        return Err("cannot iterate a partially moved BTreeMap".into());
    }
    let entries = std::mem::take(&mut *entries);
    let values = entries.into_iter().map(|(key, slot)| {
        tuple(vec![
            key.into_value(),
            slot.value.expect("unreferenced BTreeMap entry is present"),
        ])
    });
    let collection_type = Type::Named {
        name: "BTreeMap".into(),
        arguments: vec![key_type.clone(), value_type.clone()],
    };
    Ok(crate::iteration::generated_collection_iterator(
        values,
        Type::Tuple(vec![key_type, value_type]),
        &collection_type,
    ))
}

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let member = rils_builtins::builtin_member("BTreeMap", "new")?;
    if member.native_symbol != Some(symbol) {
        return None;
    }
    Some(if arguments.is_empty() {
        Ok(Value::BTreeMap(Rc::new(BTreeMapValue {
            borrowed: std::cell::Cell::new(0),
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        })))
    } else {
        Err(format!(
            "BTreeMap::new expects 0 arguments, found {}",
            arguments.len()
        ))
    })
}

fn key(arguments: &[Value], index: usize) -> Result<HashKey, String> {
    HashKey::from_ordered_value(arguments.get(index).ok_or("missing BTreeMap key")?)
        .map_err(|_| "BTreeMap key must be bool, integer, char, or string".into())
}

fn reject_referenced(map: &BTreeMapValue) -> Result<(), String> {
    if map.borrowed.get() > 0
        || map
            .entries
            .try_borrow()
            .map_err(|_| "cannot mutate BTreeMap while its entries are accessed")?
            .values()
            .any(|slot| {
                slot.references > 0
                    || slot
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
            })
    {
        Err("cannot mutate BTreeMap while a value is referenced".into())
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

fn query(arguments: &[Value], expected: &Type) -> Result<KeyIdentity, String> {
    KeyIdentity::from_value(
        arguments.get(1).ok_or("missing BTreeMap key")?,
        Some(expected),
        true,
    )
}
