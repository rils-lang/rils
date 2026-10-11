// HashKey orders its immutable identity; stored values only reconstruct keys.
#![allow(clippy::mutable_key_type)]

use std::{cell::RefCell, rc::Rc};

use crate::value::borrowed::with_legacy;

use crate::{
    types::Type,
    value::{BTreeMapValue, KeyIdentity, Value},
};

pub(super) fn call(method: &str, arguments: &[Value]) -> Result<Value, String> {
    let receiver = arguments.first().ok_or("missing BTreeMap receiver")?;
    let mutating = matches!(method, "clear" | "insert" | "remove");
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeMap mutation requires a mutable reference".into());
    }
    with_legacy(receiver, |value| {
        let Value::BTreeMap(map) = value else {
            return Err("expected BTreeMap receiver".into());
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
                reject_referenced(map)?;
                map.entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?
                    .clear();
                Ok(Value::Unit)
            }
            "contains_key" => {
                let key = query(
                    arguments,
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
                let key = query(
                    arguments,
                    &*map
                        .key_type
                        .try_borrow()
                        .map_err(|_| "collection key type is accessed")?,
                )?;
                let entries = map
                    .entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?;
                let value_type = map
                    .value_type
                    .try_borrow()
                    .map_err(|_| "collection value type is accessed")?;
                let output = map
                    .option_storage
                    .try_borrow_mut()
                    .map_err(|_| "map Option storage is accessed")?
                    .prepare(
                        &value_type,
                        entries
                            .get(&key)
                            .and_then(|slot| slot.value.as_ref())
                            .or_else(|| entries.values().find_map(|slot| slot.value.as_ref())),
                        &crate::value::storage::TypedStorageContext::new(&[], &[]),
                    )?;
                let value = entries
                    .get(&key)
                    .and_then(|slot| slot.value.as_ref())
                    .map(Value::clone_owned)
                    .transpose()?;
                output.construct(value)
            }

            "remove" => {
                reject_referenced(map)?;
                let key = query(
                    arguments,
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
                let mut entries = map
                    .entries
                    .try_borrow_mut()
                    .map_err(|_| "collection entries are accessed")?;
                let output = map
                    .option_storage
                    .try_borrow_mut()
                    .map_err(|_| "map Option storage is accessed")?
                    .prepare(
                        &value_type,
                        entries
                            .get(&key)
                            .and_then(|slot| slot.value.as_ref())
                            .or_else(|| entries.values().find_map(|slot| slot.value.as_ref())),
                        &crate::value::storage::TypedStorageContext::new(&[], &[]),
                    )?;
                if let Some(value) = entries.get(&key).and_then(|slot| slot.value.as_ref()) {
                    crate::value::storage::PreparedOption::check_move(value)?;
                }
                let value = entries.remove(&key).and_then(|slot| slot.value);
                output.construct(value)
            }
            "first_key_cloned" | "last_key_cloned" => {
                let entries = map
                    .entries
                    .try_borrow()
                    .map_err(|_| "collection entries are accessed")?;
                let key = if method == "first_key_cloned" {
                    entries.first_key_value()
                } else {
                    entries.last_key_value()
                };
                let key_type = map
                    .key_type
                    .try_borrow()
                    .map_err(|_| "collection key type is accessed")?;
                let output = map
                    .option_storage
                    .try_borrow_mut()
                    .map_err(|_| "map Option storage is accessed")?
                    .prepare_key(
                        &key_type,
                        key.map(|(key, _)| key.stored_value()),
                        &crate::value::storage::TypedStorageContext::new(&[], &[]),
                    )?;
                output.construct(key.map(|(key, _)| key.to_value()).transpose()?)
            }
            _ => Err("unsupported BTreeMap operation".into()),
        }
    })
}

pub(crate) fn into_iter(
    map: Rc<BTreeMapValue>,
    context: &crate::runtime_builtins::NativeOwnedContext,
) -> Result<Value, String> {
    reject_referenced(&map)?;
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
        .map_err(|_| "cannot consume BTreeMap while its entries are accessed")?;
    if entries.values().any(|slot| slot.value.is_none()) {
        return Err("cannot iterate a partially moved BTreeMap".into());
    }
    for key in entries.keys() {
        key.check_move()?;
    }
    let item_type = Type::Tuple(vec![key_type.clone(), value_type.clone()]);
    if !item_type.is_concrete_type() {
        return Err(format!("cannot infer complete map entry type: {item_type}"));
    }
    let prepared = entries
        .iter()
        .map(|(key, slot)| {
            context.storage().prepare_tuple(
                &item_type,
                &[
                    key.stored_value(),
                    slot.value.as_ref().expect("entries checked"),
                ],
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let entries = std::mem::take(&mut *entries);
    let values = entries
        .into_iter()
        .zip(prepared)
        .map(|((key, slot), tuple)| {
            tuple.construct(vec![
                key.into_value()?,
                slot.value.expect("entries checked"),
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
            option_storage: Default::default(),
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

fn query(arguments: &[Value], expected: &Type) -> Result<KeyIdentity, String> {
    KeyIdentity::from_value(
        arguments.get(1).ok_or("missing BTreeMap key")?,
        Some(expected),
        true,
    )
}
