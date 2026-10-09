// HashKey's owned payload does not participate in Eq, Hash, or Ord.
#![allow(clippy::mutable_key_type)]

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    rc::Rc,
};

use super::{FieldSlot, Value};
use crate::types::Type;

#[path = "hash/key.rs"]
mod key;
pub use key::{HashKey, KeyIdentity};
#[path = "hash/inspection.rs"]
mod inspection;

pub struct HashMapValue {
    pub borrowed: std::cell::Cell<usize>,
    pub entries: RefCell<HashMap<HashKey, FieldSlot>>,
    pub key_type: RefCell<Type>,
    pub value_type: RefCell<Type>,
}

pub struct BTreeMapValue {
    pub borrowed: std::cell::Cell<usize>,
    pub entries: RefCell<BTreeMap<HashKey, FieldSlot>>,
    pub key_type: RefCell<Type>,
    pub value_type: RefCell<Type>,
}

pub struct BTreeSetValue {
    pub borrowed: std::cell::Cell<usize>,
    pub entries: RefCell<BTreeSet<HashKey>>,
    pub element_type: RefCell<Type>,
}

pub struct HashSetValue {
    pub borrowed: std::cell::Cell<usize>,
    pub entries: RefCell<HashSet<HashKey>>,
    pub element_type: RefCell<Type>,
}

#[derive(Clone)]
pub enum MapCollection {
    Hash(Rc<HashMapValue>),
    BTree(Rc<BTreeMapValue>),
}

impl MapCollection {
    pub fn borrowed(&self) -> &std::cell::Cell<usize> {
        match self {
            Self::Hash(map) => &map.borrowed,
            Self::BTree(map) => &map.borrowed,
        }
    }

    pub fn contains_key(&self, key: &HashKey) -> bool {
        match self {
            Self::Hash(map) => map.entries.borrow().contains_key(key),
            Self::BTree(map) => map.entries.borrow().contains_key(key),
        }
    }

    pub fn value(&self, key: &HashKey) -> Option<Value> {
        match self {
            Self::Hash(map) => map
                .entries
                .borrow()
                .get(key)
                .and_then(|slot| slot.value.clone()),
            Self::BTree(map) => map
                .entries
                .borrow()
                .get(key)
                .and_then(|slot| slot.value.clone()),
        }
    }
}

#[derive(Clone)]
pub enum SetCollection {
    Hash(Rc<HashSetValue>),
    BTree(Rc<BTreeSetValue>),
}

impl SetCollection {
    pub fn borrowed(&self) -> &std::cell::Cell<usize> {
        match self {
            Self::Hash(set) => &set.borrowed,
            Self::BTree(set) => &set.borrowed,
        }
    }

    pub fn contains(&self, key: &HashKey) -> bool {
        match self {
            Self::Hash(set) => set.entries.borrow().contains(key),
            Self::BTree(set) => set.entries.borrow().contains(key),
        }
    }
}

pub(super) fn clone_hash_map(map: &HashMapValue) -> Result<HashMapValue, String> {
    let entries = map
        .entries
        .borrow()
        .iter()
        .map(|(key, slot)| {
            let value = slot
                .value
                .as_ref()
                .ok_or_else(|| "cannot clone a partially moved HashMap".to_string())?;
            Ok((
                key.clone_owned()?,
                FieldSlot::new(slot.type_annotation.clone(), value.clone_owned()?),
            ))
        })
        .collect::<Result<HashMap<_, _>, String>>()?;
    Ok(HashMapValue {
        borrowed: std::cell::Cell::new(0),
        entries: RefCell::new(entries),
        key_type: RefCell::new(map.key_type.borrow().clone()),
        value_type: RefCell::new(map.value_type.borrow().clone()),
    })
}

pub(super) fn clone_btree_map(map: &BTreeMapValue) -> Result<BTreeMapValue, String> {
    let entries = map
        .entries
        .borrow()
        .iter()
        .map(|(key, slot)| {
            let value = slot
                .value
                .as_ref()
                .ok_or("cannot clone a partially moved BTreeMap")?;
            Ok((
                key.clone_owned()?,
                FieldSlot::new(slot.type_annotation.clone(), value.clone_owned()?),
            ))
        })
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    Ok(BTreeMapValue {
        borrowed: std::cell::Cell::new(0),
        entries: RefCell::new(entries),
        key_type: RefCell::new(map.key_type.borrow().clone()),
        value_type: RefCell::new(map.value_type.borrow().clone()),
    })
}

pub(super) fn btree_maps_equal(left: &BTreeMapValue, right: &BTreeMapValue) -> bool {
    let left = left.entries.borrow();
    let right = right.entries.borrow();
    left.len() == right.len()
        && left.iter().all(|(key, slot)| {
            right
                .get(key)
                .is_some_and(|other| slot.value == other.value)
        })
}

pub(super) fn display_btree_map(f: &mut fmt::Formatter<'_>, map: &BTreeMapValue) -> fmt::Result {
    let entries = map.entries.borrow();
    write!(f, "{{")?;
    for (index, (key, slot)) in entries.iter().enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        write!(
            f,
            "{}: {}",
            key,
            slot.value
                .as_ref()
                .map_or_else(|| "<moved>".into(), ToString::to_string)
        )?;
    }
    write!(f, "}}")
}

pub(super) fn display_btree_set(f: &mut fmt::Formatter<'_>, set: &BTreeSetValue) -> fmt::Result {
    write!(f, "{{")?;
    for (index, key) in set.entries.borrow().iter().enumerate() {
        if index > 0 {
            write!(f, ", ")?;
        }
        write!(f, "{key}")?;
    }
    write!(f, "}}")
}

pub(super) fn hash_maps_equal(left: &HashMapValue, right: &HashMapValue) -> bool {
    let left = left.entries.borrow();
    let right = right.entries.borrow();
    left.len() == right.len()
        && left.iter().all(|(key, slot)| {
            right
                .get(key)
                .is_some_and(|other| slot.value == other.value)
        })
}

pub(super) fn display_hash_map(f: &mut fmt::Formatter<'_>, map: &HashMapValue) -> fmt::Result {
    let entries = map.entries.borrow();
    let mut values = entries
        .iter()
        .map(|(key, slot)| {
            format!(
                "{}: {}",
                key,
                slot.value
                    .as_ref()
                    .map_or_else(|| "<moved>".into(), ToString::to_string)
            )
        })
        .collect::<Vec<_>>();
    values.sort();
    write!(f, "{{{}}}", values.join(", "))
}

pub(super) fn display_hash_set(f: &mut fmt::Formatter<'_>, set: &HashSetValue) -> fmt::Result {
    let entries = set.entries.borrow();
    let mut values = entries.iter().map(ToString::to_string).collect::<Vec<_>>();
    values.sort();
    write!(f, "{{{}}}", values.join(", "))
}
