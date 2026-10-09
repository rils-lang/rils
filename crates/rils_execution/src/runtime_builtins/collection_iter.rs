use std::{cell::Cell, rc::Rc};

use crate::{
    types::Type,
    value::{
        BorrowedMapIteratorValue, BorrowedSetIteratorValue, HashKey, KeyIdentity, MapCollection,
        ReferenceValue, SetCollection, Value,
    },
};

pub(super) fn call_symbol(symbol: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let owner = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"]
        .into_iter()
        .find(|owner| {
            rils_builtins::builtin_member(owner, "iter")
                .is_some_and(|member| member.native_symbol == Some(symbol))
        })?;
    call(owner, arguments)
}

fn call(owner: &str, arguments: &[Value]) -> Option<Result<Value, String>> {
    let Some(Value::Reference(source)) = arguments.first() else {
        return Some(Err("collection iter requires a borrowed receiver".into()));
    };
    match source.native_layout() {
        Ok(Some(layout)) if layout.sequence_item().is_some() => return None,
        Err(error) => return Some(Err(error)),
        _ => {}
    }
    let collection = match source.read() {
        Ok(collection) => collection,
        Err(message) => return Some(Err(message)),
    };
    if matches!(collection, Value::Dynamic(_)) {
        return None;
    }
    Some((|| match (owner, collection) {
        ("HashMap", Value::HashMap(map)) => {
            let keys = map
                .entries
                .try_borrow()
                .map_err(|_| "iter map is already mutably accessed")?
                .keys()
                .map(HashKey::identity)
                .collect();
            let key_type = map
                .key_type
                .try_borrow()
                .map_err(|_| "iter map key type is already mutably accessed")?
                .clone();
            let value_type = map
                .value_type
                .try_borrow()
                .map_err(|_| "iter map value type is already mutably accessed")?
                .clone();
            borrowed_map(source, MapCollection::Hash(map), keys, key_type, value_type)
        }
        ("BTreeMap", Value::BTreeMap(map)) => {
            let keys = map
                .entries
                .try_borrow()
                .map_err(|_| "iter map is already mutably accessed")?
                .keys()
                .map(HashKey::identity)
                .collect();
            let key_type = map
                .key_type
                .try_borrow()
                .map_err(|_| "iter map key type is already mutably accessed")?
                .clone();
            let value_type = map
                .value_type
                .try_borrow()
                .map_err(|_| "iter map value type is already mutably accessed")?
                .clone();
            borrowed_map(
                source,
                MapCollection::BTree(map),
                keys,
                key_type,
                value_type,
            )
        }
        ("HashSet", Value::HashSet(set)) => {
            let keys = set
                .entries
                .try_borrow()
                .map_err(|_| "iter set is already mutably accessed")?
                .iter()
                .map(HashKey::identity)
                .collect();
            let element_type = set
                .element_type
                .try_borrow()
                .map_err(|_| "iter set element type is already mutably accessed")?
                .clone();
            borrowed_set(source, SetCollection::Hash(set), keys, element_type)
        }
        ("BTreeSet", Value::BTreeSet(set)) => {
            let keys = set
                .entries
                .try_borrow()
                .map_err(|_| "iter set is already mutably accessed")?
                .iter()
                .map(HashKey::identity)
                .collect();
            let element_type = set
                .element_type
                .try_borrow()
                .map_err(|_| "iter set element type is already mutably accessed")?
                .clone();
            borrowed_set(source, SetCollection::BTree(set), keys, element_type)
        }
        _ => Err("iter receiver has the wrong collection type".into()),
    })())
}

fn borrowed_map(
    source: &Rc<ReferenceValue>,
    map: MapCollection,
    keys: Vec<Rc<KeyIdentity>>,
    key_type: Type,
    value_type: Type,
) -> Result<Value, String> {
    map.borrowed().set(map.borrowed().get() + 1);
    Ok(Value::BorrowedMapIterator(Rc::new(
        BorrowedMapIteratorValue {
            source: source.clone(),
            map,
            keys,
            index: Cell::new(0),
            key_type,
            value_type,
        },
    )))
}

fn borrowed_set(
    source: &Rc<ReferenceValue>,
    set: SetCollection,
    keys: Vec<Rc<KeyIdentity>>,
    element_type: Type,
) -> Result<Value, String> {
    set.borrowed().set(set.borrowed().get() + 1);
    Ok(Value::BorrowedSetIterator(Rc::new(
        BorrowedSetIteratorValue {
            source: source.clone(),
            set,
            keys,
            index: Cell::new(0),
            element_type,
        },
    )))
}
