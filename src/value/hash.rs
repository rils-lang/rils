use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    fmt,
    rc::Rc,
};

use super::{FieldSlot, Value};
use crate::types::Type;

#[derive(Clone, Debug, Hash, PartialEq, Eq)]
pub enum HashKey {
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
}

impl HashKey {
    pub fn from_value(value: &Value) -> Result<Self, String> {
        let value = match value {
            Value::Reference(reference) => reference.read()?,
            value => value.clone(),
        };
        Ok(match value {
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
        }
    }

    pub fn ty(&self) -> Type {
        Type::of_value(&self.to_value()).expect("hash keys always have a runtime type")
    }
}

pub struct HashMapValue {
    pub entries: RefCell<HashMap<HashKey, FieldSlot>>,
    pub key_type: RefCell<Type>,
    pub value_type: RefCell<Type>,
}

pub struct HashSetValue {
    pub entries: RefCell<HashSet<HashKey>>,
    pub element_type: RefCell<Type>,
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
        entries: RefCell::new(entries),
        key_type: RefCell::new(map.key_type.borrow().clone()),
        value_type: RefCell::new(map.value_type.borrow().clone()),
    })
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
