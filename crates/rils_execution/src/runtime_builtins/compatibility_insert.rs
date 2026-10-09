//! Owned fallback while compatibility Map/Set storage is being retired.
use crate::value::borrowed::with_legacy;
use crate::{
    Type,
    types::merge_types,
    value::{FieldSlot, HashKey, Value},
};
use std::rc::Rc;

pub(super) fn map(arguments: Vec<Value>, ordered: bool) -> Result<Value, String> {
    let mut arguments = arguments.into_iter();
    let receiver = arguments.next().ok_or("missing map receiver")?;
    if !matches!(&receiver, Value::Reference(reference) if reference.mutable) {
        return Err("map insertion requires &mut self".into());
    }
    let key = arguments.next().ok_or("missing map key")?;
    let value = arguments.next().ok_or("missing map value")?;
    if arguments.next().is_some() {
        return Err("too many map arguments".into());
    }
    macro_rules! insert {
        ($map:expr) => {{
            let map = $map;
            if map.borrowed.get() != 0 {
                return Err("cannot mutate a borrowed map".into());
            }
            let mut key_type = map
                .key_type
                .try_borrow_mut()
                .map_err(|_| "map key type is accessed")?;
            let mut value_type = map
                .value_type
                .try_borrow_mut()
                .map_err(|_| "map value type is accessed")?;
            let next_value_type = merge_types(
                &value_type,
                &Type::of_value(&value).ok_or("map value has no type")?,
            )
            .filter(Type::is_concrete_type)
            .ok_or("map value type mismatch or incomplete type")?;
            let key = HashKey::from_owned_value(key, Some(&key_type), ordered)?;
            let next_key_type = key.ty();
            let mut entries = map
                .entries
                .try_borrow_mut()
                .map_err(|_| "map entries are accessed")?;
            if entries.values().any(|slot| {
                slot.references != 0
                    || slot
                        .value
                        .as_ref()
                        .is_some_and(Value::has_active_references)
            }) {
                return Err("cannot mutate a map while a value is referenced".into());
            }
            let previous = entries.insert(key, FieldSlot::new(next_value_type.clone(), value));
            *key_type = next_key_type;
            *value_type = next_value_type.clone();
            Ok(Value::Option {
                value: previous.and_then(|slot| slot.value).map(Rc::new),
                element_type: Some(next_value_type),
            })
        }};
    }
    with_legacy(&receiver, |value| match value {
        Value::HashMap(map) if !ordered => insert!(map),
        Value::BTreeMap(map) if ordered => insert!(map),
        _ => Err("wrong map receiver".into()),
    })
}

pub(super) fn set(arguments: Vec<Value>, ordered: bool) -> Result<Value, String> {
    let mut arguments = arguments.into_iter();
    let receiver = arguments.next().ok_or("missing set receiver")?;
    if !matches!(&receiver, Value::Reference(reference) if reference.mutable) {
        return Err("set insertion requires &mut self".into());
    }
    let value = arguments.next().ok_or("missing set element")?;
    if arguments.next().is_some() {
        return Err("too many set arguments".into());
    }
    macro_rules! insert {
        ($set:expr) => {{
            let set = $set;
            if set.borrowed.get() != 0 {
                return Err("cannot mutate a borrowed set".into());
            }
            let mut element_type = set
                .element_type
                .try_borrow_mut()
                .map_err(|_| "set element type is accessed")?;
            let key = HashKey::from_owned_value(value, Some(&element_type), ordered)?;
            let next_type = key.ty();
            let mut entries = set
                .entries
                .try_borrow_mut()
                .map_err(|_| "set entries are accessed")?;
            let inserted = entries.insert(key);
            *element_type = next_type;
            Ok(Value::Bool(inserted))
        }};
    }
    with_legacy(&receiver, |value| match value {
        Value::HashSet(set) if !ordered => insert!(set),
        Value::BTreeSet(set) if ordered => insert!(set),
        _ => Err("wrong set receiver".into()),
    })
}
