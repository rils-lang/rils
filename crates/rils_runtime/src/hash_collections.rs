use std::{cell::RefCell, collections::VecDeque, rc::Rc};

use rils_builtins::BuiltinId;

use crate::{
    types::{Type, merge_types},
    value::{
        FieldSlot, HashKey, HashMapValue, HashSetValue, SequenceIteratorValue, SequenceValue, Value,
    },
};

pub fn call(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    match id {
        BuiltinId::HashMapLen
        | BuiltinId::HashMapIsEmpty
        | BuiltinId::HashMapClear
        | BuiltinId::HashMapContainsKey
        | BuiltinId::HashMapInsert
        | BuiltinId::HashMapGetCloned
        | BuiltinId::HashMapRemove
        | BuiltinId::HashMapKeysCloned
        | BuiltinId::HashMapValuesCloned
        | BuiltinId::HashMapIntoIter => call_map(id, arguments),
        BuiltinId::HashSetLen
        | BuiltinId::HashSetIsEmpty
        | BuiltinId::HashSetClear
        | BuiltinId::HashSetContains
        | BuiltinId::HashSetInsert
        | BuiltinId::HashSetRemove
        | BuiltinId::HashSetIsSubset
        | BuiltinId::HashSetIsSuperset
        | BuiltinId::HashSetIsDisjoint
        | BuiltinId::HashSetUnion
        | BuiltinId::HashSetIntersection
        | BuiltinId::HashSetDifference
        | BuiltinId::HashSetSymmetricDifference
        | BuiltinId::HashSetIntoIter => call_set(id, arguments),
        _ => Err(format!("unknown hash collection built-in `{id:?}`")),
    }
}

fn call_map(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    let map = hash_map(
        arguments
            .first()
            .ok_or_else(|| "missing HashMap receiver".to_string())?,
    )?;
    match id {
        BuiltinId::HashMapLen => Ok(Value::Usize(map.entries.borrow().len())),
        BuiltinId::HashMapIsEmpty => Ok(Value::Bool(map.entries.borrow().is_empty())),
        BuiltinId::HashMapClear => {
            reject_referenced_map(&map)?;
            map.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        BuiltinId::HashMapContainsKey => {
            let key = hash_argument(arguments, 1)?;
            Ok(Value::Bool(map.entries.borrow().contains_key(&key)))
        }
        BuiltinId::HashMapInsert => {
            reject_referenced_map(&map)?;
            let key_value = arguments
                .get(1)
                .ok_or_else(|| "missing HashMap key".to_string())?;
            let key = HashKey::from_value(key_value)?;
            let value = arguments
                .get(2)
                .ok_or_else(|| "missing HashMap value".to_string())?
                .clone();
            if value.contains_reference() {
                return Err("HashMap values cannot contain references".into());
            }
            let key_type = merge_collection_type(&map.key_type, key.ty(), "HashMap key")?;
            let value_type = merge_collection_type(
                &map.value_type,
                Type::of_value(&value).unwrap_or(Type::Unknown),
                "HashMap value",
            )?;
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
        BuiltinId::HashMapGetCloned => {
            let key = hash_argument(arguments, 1)?;
            let value = map
                .entries
                .borrow()
                .get(&key)
                .and_then(|slot| slot.value.as_ref())
                .map(Value::clone_owned)
                .transpose()?;
            option(value, map.value_type.borrow().clone())
        }
        BuiltinId::HashMapRemove => {
            reject_referenced_map(&map)?;
            let key = hash_argument(arguments, 1)?;
            let value = map
                .entries
                .borrow_mut()
                .remove(&key)
                .and_then(|slot| slot.value);
            option(value, map.value_type.borrow().clone())
        }
        BuiltinId::HashMapKeysCloned => Ok(iterator(
            map.entries.borrow().keys().map(HashKey::to_value).collect(),
            map.key_type.borrow().clone(),
        )),
        BuiltinId::HashMapValuesCloned => Ok(iterator(
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
        BuiltinId::HashMapIntoIter => {
            reject_referenced_map(&map)?;
            let key_type = map.key_type.borrow().clone();
            let value_type = map.value_type.borrow().clone();
            let values = map
                .entries
                .borrow_mut()
                .drain()
                .map(|(key, slot)| {
                    tuple(vec![
                        key.to_value(),
                        slot.value.expect("unreferenced HashMap value is present"),
                    ])
                })
                .collect();
            Ok(iterator(values, Type::Tuple(vec![key_type, value_type])))
        }
        _ => Err(format!("unknown HashMap built-in `{id:?}`")),
    }
}

fn call_set(id: BuiltinId, arguments: &[Value]) -> Result<Value, String> {
    let set = hash_set(
        arguments
            .first()
            .ok_or_else(|| "missing HashSet receiver".to_string())?,
    )?;
    match id {
        BuiltinId::HashSetLen => Ok(Value::Usize(set.entries.borrow().len())),
        BuiltinId::HashSetIsEmpty => Ok(Value::Bool(set.entries.borrow().is_empty())),
        BuiltinId::HashSetClear => {
            set.entries.borrow_mut().clear();
            Ok(Value::Unit)
        }
        BuiltinId::HashSetContains => {
            let key = hash_argument(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow().contains(&key)))
        }
        BuiltinId::HashSetInsert => {
            let key = hash_argument(arguments, 1)?;
            let element_type =
                merge_collection_type(&set.element_type, key.ty(), "HashSet element")?;
            let inserted = set.entries.borrow_mut().insert(key);
            *set.element_type.borrow_mut() = element_type;
            Ok(Value::Bool(inserted))
        }
        BuiltinId::HashSetRemove => {
            let key = hash_argument(arguments, 1)?;
            Ok(Value::Bool(set.entries.borrow_mut().remove(&key)))
        }
        BuiltinId::HashSetIsSubset
        | BuiltinId::HashSetIsSuperset
        | BuiltinId::HashSetIsDisjoint => {
            let other = hash_set(
                arguments
                    .get(1)
                    .ok_or_else(|| "missing other HashSet".to_string())?,
            )?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            Ok(Value::Bool(match id {
                BuiltinId::HashSetIsSubset => left.is_subset(&right),
                BuiltinId::HashSetIsSuperset => left.is_superset(&right),
                BuiltinId::HashSetIsDisjoint => left.is_disjoint(&right),
                _ => unreachable!(),
            }))
        }
        BuiltinId::HashSetUnion
        | BuiltinId::HashSetIntersection
        | BuiltinId::HashSetDifference
        | BuiltinId::HashSetSymmetricDifference => {
            let other = hash_set(
                arguments
                    .get(1)
                    .ok_or_else(|| "missing other HashSet".to_string())?,
            )?;
            let left = set.entries.borrow();
            let right = other.entries.borrow();
            let entries = match id {
                BuiltinId::HashSetUnion => left.union(&right).cloned().collect(),
                BuiltinId::HashSetIntersection => left.intersection(&right).cloned().collect(),
                BuiltinId::HashSetDifference => left.difference(&right).cloned().collect(),
                BuiltinId::HashSetSymmetricDifference => {
                    left.symmetric_difference(&right).cloned().collect()
                }
                _ => unreachable!(),
            };
            let element_type =
                merge_types(&set.element_type.borrow(), &other.element_type.borrow())
                    .ok_or_else(|| "HashSet element types do not match".to_string())?;
            Ok(Value::HashSet(Rc::new(HashSetValue {
                entries: RefCell::new(entries),
                element_type: RefCell::new(element_type),
            })))
        }
        BuiltinId::HashSetIntoIter => {
            let element_type = set.element_type.borrow().clone();
            let values = set
                .entries
                .borrow_mut()
                .drain()
                .map(|key| key.to_value())
                .collect();
            Ok(iterator(values, element_type))
        }
        _ => Err(format!("unknown HashSet built-in `{id:?}`")),
    }
}

fn hash_argument(arguments: &[Value], index: usize) -> Result<HashKey, String> {
    HashKey::from_value(
        arguments
            .get(index)
            .ok_or_else(|| "missing hash collection key".to_string())?,
    )
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
    if map.entries.borrow().values().any(|slot| {
        slot.references > 0
            || slot
                .value
                .as_ref()
                .is_some_and(Value::has_active_references)
    }) {
        Err("cannot mutate a HashMap while a value is referenced".into())
    } else {
        Ok(())
    }
}

fn merge_collection_type(
    target: &RefCell<Type>,
    actual: Type,
    subject: &str,
) -> Result<Type, String> {
    merge_types(&target.borrow(), &actual).ok_or_else(|| {
        format!(
            "{subject} type mismatch: expected {}, found {actual}",
            target.borrow()
        )
    })
}

fn option(value: Option<Value>, element_type: Type) -> Result<Value, String> {
    Ok(Value::Option {
        value: value.map(Rc::new),
        element_type: Some(element_type),
    })
}

fn iterator(items: VecDeque<Value>, element_type: Type) -> Value {
    Value::SequenceIterator(Rc::new(SequenceIteratorValue {
        items: RefCell::new(items),
        element_type,
    }))
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
