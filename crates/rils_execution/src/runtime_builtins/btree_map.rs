use std::{cell::RefCell, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::{Type, merge_types},
    value::{BTreeMapValue, FieldSlot, HashKey, SequenceIteratorValue, SequenceValue, Value},
};

pub(super) fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    if id == BuiltinId::BtreeMapNew {
        return Ok(Value::BTreeMap(Rc::new(BTreeMapValue {
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
        })));
    }
    let receiver = arguments.first().ok_or("missing BTreeMap receiver")?;
    let mutating = matches!(
        id,
        BuiltinId::BtreeMapClear | BuiltinId::BtreeMapInsert | BuiltinId::BtreeMapRemove
    );
    if mutating && !matches!(receiver, Value::Reference(reference) if reference.mutable) {
        return Err("BTreeMap mutation requires a mutable reference".into());
    }
    let Value::BTreeMap(map) = super::import_receiver(receiver)? else {
        return Err("expected BTreeMap receiver".into());
    };
    match id {
        BuiltinId::BtreeMapLen => Ok(Value::Usize(map.entries.borrow().len())),
        BuiltinId::BtreeMapIsEmpty => Ok(Value::Bool(map.entries.borrow().is_empty())),
        BuiltinId::BtreeMapClear => {
            reject_referenced(&map)?;
            map.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        BuiltinId::BtreeMapContainsKey => {
            let key = key(arguments, 1)?;
            Ok(Value::Bool(map.entries.borrow().contains_key(&key)))
        }
        BuiltinId::BtreeMapInsert => {
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
            let previous = map.entries.borrow_mut().insert(
                key,
                FieldSlot {
                    value: Some(value),
                    type_annotation: value_type.clone(),
                    references: 0,
                },
            );
            *map.key_type.borrow_mut() = key_type;
            *map.value_type.borrow_mut() = value_type.clone();
            option(previous.and_then(|slot| slot.value), value_type)
        }
        BuiltinId::BtreeMapGetCloned => {
            let key = key(arguments, 1)?;
            let value = map
                .entries
                .borrow()
                .get(&key)
                .and_then(|slot| slot.value.as_ref())
                .map(Value::clone_owned)
                .transpose()?;
            option(value, map.value_type.borrow().clone())
        }
        BuiltinId::BtreeMapRemove => {
            reject_referenced(&map)?;
            let key = key(arguments, 1)?;
            let value = map
                .entries
                .borrow_mut()
                .remove(&key)
                .and_then(|slot| slot.value);
            option(value, map.value_type.borrow().clone())
        }
        BuiltinId::BtreeMapFirstKeyCloned | BuiltinId::BtreeMapLastKeyCloned => {
            let entries = map.entries.borrow();
            let key = if id == BuiltinId::BtreeMapFirstKeyCloned {
                entries.first_key_value()
            } else {
                entries.last_key_value()
            };
            option(
                key.map(|(key, _)| key.to_value()),
                map.key_type.borrow().clone(),
            )
        }
        BuiltinId::BtreeMapIntoIter => {
            reject_referenced(&map)?;
            let key_type = map.key_type.borrow().clone();
            let value_type = map.value_type.borrow().clone();
            let entries = std::mem::take(&mut *map.entries.borrow_mut());
            let values = entries
                .into_iter()
                .map(|(key, slot)| {
                    tuple(vec![
                        key.to_value(),
                        slot.value.expect("unreferenced BTreeMap entry is present"),
                    ])
                })
                .collect();
            Ok(Value::SequenceIterator(Rc::new(SequenceIteratorValue {
                items: RefCell::new(values),
                element_type: Type::Tuple(vec![key_type, value_type]),
            })))
        }
        _ => Err("unsupported BTreeMap operation".into()),
    }
}

fn key(arguments: &[Value], index: usize) -> Result<HashKey, String> {
    HashKey::from_value(arguments.get(index).ok_or("missing BTreeMap key")?)
        .map_err(|_| "BTreeMap key must be bool, integer, char, or string".into())
}

fn reject_referenced(map: &BTreeMapValue) -> Result<(), String> {
    if map.entries.borrow().values().any(|slot| {
        slot.references > 0
            || slot
                .value
                .as_ref()
                .is_some_and(Value::has_active_references)
    }) {
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
    Value::Tuple(Rc::new(SequenceValue {
        elements: RefCell::new(
            values
                .into_iter()
                .map(|value| FieldSlot {
                    type_annotation: Type::of_value(&value).unwrap_or(Type::Unknown),
                    value: Some(value),
                    references: 0,
                })
                .collect(),
        ),
        element_type: RefCell::new(None),
    }))
}
