// StructuralKey's mutable Value snapshot does not participate in Eq, Hash, or Ord.
#![allow(clippy::mutable_key_type)]

use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fmt,
    hash::{Hash, Hasher},
    rc::Rc,
};

use super::{FieldSlot, Value};
use crate::types::Type;

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum HashKey {
    Unit,
    Bool(bool),
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),
    Char(char),
    String(Rc<str>),
    Composite(Box<StructuralKey>),
}

#[derive(Clone, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
enum StructuralIdentity {
    Tuple(Vec<HashKey>),
    Array(Vec<HashKey>),
    Option(Option<Box<HashKey>>),
    ResultOk(Box<HashKey>),
    ResultErr(Box<HashKey>),
    Struct {
        name: String,
        arguments: Vec<String>,
        fields: Vec<(String, HashKey)>,
    },
    Enum {
        name: String,
        arguments: Vec<String>,
        variant: String,
        payload: Vec<(String, HashKey)>,
    },
}

#[derive(Clone)]
pub struct StructuralKey {
    identity: StructuralIdentity,
    value: Value,
}

impl fmt::Debug for StructuralKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.identity.fmt(formatter)
    }
}

impl PartialEq for StructuralKey {
    fn eq(&self, other: &Self) -> bool {
        self.identity == other.identity
    }
}

impl Eq for StructuralKey {}

impl PartialOrd for StructuralKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for StructuralKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identity.cmp(&other.identity)
    }
}

impl Hash for StructuralKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.identity.hash(state);
    }
}

impl HashKey {
    pub fn from_ordered_value(value: &Value) -> Result<Self, String> {
        let key = Self::from_value(value)?;
        if matches!(&key, Self::Unit | Self::Composite(_)) {
            return Err(format!(
                "{} cannot be used as an ordered collection key",
                value.type_name()
            ));
        }
        Ok(key)
    }

    pub fn from_value(value: &Value) -> Result<Self, String> {
        let value = match value {
            Value::Reference(reference) => reference.read()?,
            value => value.clone(),
        };
        Ok(match value {
            Value::Unit => Self::Unit,
            Value::Bool(value) => Self::Bool(value),
            Value::I8(value) => Self::I8(value),
            Value::I16(value) => Self::I16(value),
            Value::I32(value) => Self::I32(value),
            Value::I64(value) => Self::I64(value),
            Value::I128(value) => Self::I128(value),
            Value::Isize(value) => Self::Isize(value),
            Value::U8(value) => Self::U8(value),
            Value::U16(value) => Self::U16(value),
            Value::U32(value) => Self::U32(value),
            Value::U64(value) => Self::U64(value),
            Value::U128(value) => Self::U128(value),
            Value::Usize(value) => Self::Usize(value),
            Value::Char(value) => Self::Char(value),
            Value::String(value) => Self::String(value),
            Value::Tuple(_)
            | Value::Array(_)
            | Value::Option { .. }
            | Value::Result { .. }
            | Value::Struct(_)
            | Value::Enum(_) => {
                let value = value.clone_owned()?;
                let identity = StructuralIdentity::from_value(&value)?;
                Self::Composite(Box::new(StructuralKey { identity, value }))
            }
            value => {
                return Err(format!(
                    "{} cannot be used as a hash collection key",
                    value.type_name()
                ));
            }
        })
    }

    pub fn to_value(&self) -> Value {
        match self {
            Self::Unit => Value::Unit,
            Self::Bool(value) => Value::Bool(*value),
            Self::I8(value) => Value::I8(*value),
            Self::I16(value) => Value::I16(*value),
            Self::I32(value) => Value::I32(*value),
            Self::I64(value) => Value::I64(*value),
            Self::I128(value) => Value::I128(*value),
            Self::Isize(value) => Value::Isize(*value),
            Self::U8(value) => Value::U8(*value),
            Self::U16(value) => Value::U16(*value),
            Self::U32(value) => Value::U32(*value),
            Self::U64(value) => Value::U64(*value),
            Self::U128(value) => Value::U128(*value),
            Self::Usize(value) => Value::Usize(*value),
            Self::Char(value) => Value::Char(*value),
            Self::String(value) => Value::String(value.clone()),
            Self::Composite(key) => key
                .value
                .clone_owned()
                .expect("stored key remains complete"),
        }
    }

    pub fn ty(&self) -> Type {
        Type::of_value(&self.to_value()).expect("hash keys always have a runtime type")
    }
}

impl StructuralIdentity {
    fn from_value(value: &Value) -> Result<Self, String> {
        let unsupported = || {
            format!(
                "{} cannot be used as a hash collection key",
                value.type_name()
            )
        };
        Ok(match value {
            Value::Tuple(sequence) | Value::Array(sequence) => {
                let elements = sequence
                    .elements
                    .borrow()
                    .iter()
                    .map(|slot| {
                        HashKey::from_value(slot.value.as_ref().ok_or("moved key element")?)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if matches!(value, Value::Tuple(_)) {
                    Self::Tuple(elements)
                } else {
                    Self::Array(elements)
                }
            }
            Value::Option { value, .. } => Self::Option(
                value
                    .as_ref()
                    .map(|value| HashKey::from_value(value).map(Box::new))
                    .transpose()?,
            ),
            Value::Result {
                value: Ok(value), ..
            } => Self::ResultOk(Box::new(HashKey::from_value(value)?)),
            Value::Result {
                value: Err(value), ..
            } => Self::ResultErr(Box::new(HashKey::from_value(value)?)),
            Value::Struct(instance) => {
                let traits = instance.type_definition.implemented_traits.borrow();
                if !traits.contains("Eq") || !traits.contains("Hash") {
                    return Err(unsupported());
                }
                let mut fields = instance
                    .fields
                    .borrow()
                    .iter()
                    .map(|(name, slot)| {
                        Ok((
                            name.clone(),
                            HashKey::from_value(slot.value.as_ref().ok_or("moved key field")?)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, String>>()?;
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                Self::Struct {
                    name: instance.type_definition.name.clone(),
                    arguments: instance
                        .type_arguments
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    fields,
                }
            }
            Value::Enum(instance) => {
                let traits = instance.type_definition.implemented_traits.borrow();
                if !traits.contains("Eq") || !traits.contains("Hash") {
                    return Err(unsupported());
                }
                let mut payload = match &instance.payload {
                    super::EnumPayload::Unit => Vec::new(),
                    super::EnumPayload::Tuple(values) => values
                        .iter()
                        .enumerate()
                        .map(|(index, value)| Ok((index.to_string(), HashKey::from_value(value)?)))
                        .collect::<Result<Vec<_>, String>>()?,
                    super::EnumPayload::Record(values) => values
                        .iter()
                        .map(|(name, value)| Ok((name.clone(), HashKey::from_value(value)?)))
                        .collect::<Result<Vec<_>, String>>()?,
                };
                payload.sort_by(|left, right| left.0.cmp(&right.0));
                Self::Enum {
                    name: instance.type_definition.name.clone(),
                    arguments: instance
                        .type_arguments
                        .iter()
                        .map(ToString::to_string)
                        .collect(),
                    variant: instance.variant.clone(),
                    payload,
                }
            }
            _ => return Err(unsupported()),
        })
    }
}

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
                key.clone(),
                FieldSlot {
                    value: Some(value.clone_owned()?),
                    type_annotation: slot.type_annotation.clone(),
                    references: 0,
                },
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
                key.clone(),
                FieldSlot {
                    value: Some(value.clone_owned()?),
                    type_annotation: slot.type_annotation.clone(),
                    references: 0,
                },
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
            key.to_value(),
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
        write!(f, "{}", key.to_value())?;
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
                key.to_value(),
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
    let mut values = entries
        .iter()
        .map(|key| key.to_value().to_string())
        .collect::<Vec<_>>();
    values.sort();
    write!(f, "{{{}}}", values.join(", "))
}
