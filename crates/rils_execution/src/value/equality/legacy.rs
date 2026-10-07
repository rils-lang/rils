//! Compatibility comparison for data not yet migrated to native views.

use super::super::{IndexedStorage, Value, char_payload, hash, native_ops};
use crate::Type;
use std::rc::Rc;

pub(super) fn equal(left: &Value, right: &Value) -> bool {
    match (left, right) {
        (Value::Unit, Value::Unit) => true,
        (Value::Bool(left), Value::Bool(right)) => left == right,
        (Value::I16(left), Value::I16(right)) => left == right,
        (Value::I64(left), Value::I64(right)) => left == right,
        (Value::I128(left), Value::I128(right)) => left == right,
        (Value::Isize(left), Value::Isize(right)) => left == right,
        (Value::U8(left), Value::U8(right)) => left == right,
        (Value::U16(left), Value::U16(right)) => left == right,
        (Value::U32(left), Value::U32(right)) => left == right,
        (Value::U64(left), Value::U64(right)) => left == right,
        (Value::U128(left), Value::U128(right)) => left == right,
        (Value::Usize(left), Value::Usize(right)) => left == right,
        (Value::Native(_), Value::Usize(right)) => {
            left.as_usize().is_some_and(|left| left == *right)
        }
        (Value::Usize(left), Value::Native(_)) => {
            right.as_usize().is_some_and(|right| *left == right)
        }
        (Value::F32(left), Value::F32(right)) => left == right,
        (Value::F64(left), Value::F64(right)) => left == right,
        (Value::Native(_), Value::F32(right)) => left.as_f32().is_some_and(|left| left == *right),
        (Value::F32(left), Value::Native(_)) => right.as_f32().is_some_and(|right| *left == right),
        (Value::Native(_), Value::F64(right)) => left.as_f64().is_some_and(|left| left == *right),
        (Value::F64(left), Value::Native(_)) => right.as_f64().is_some_and(|right| *left == right),
        (Value::Char(left), Value::Char(right)) => left == right,
        (Value::Native(_), Value::Char(right)) => {
            char_payload(left).is_some_and(|left| left == *right)
        }
        (Value::Char(left), Value::Native(_)) => {
            char_payload(right).is_some_and(|right| *left == right)
        }
        (Value::Native(left), Value::Native(right)) => native_ops::equal(left, right),
        (Value::Native(_), legacy) if matches!(Type::of_value(legacy), Some(Type::Integer(_))) => {
            let lowered = crate::numeric::lower_migrated_integer(left.clone());
            !matches!(&lowered, Value::Native(_)) && &lowered == legacy
        }
        (legacy, Value::Native(_)) if matches!(Type::of_value(legacy), Some(Type::Integer(_))) => {
            let lowered = crate::numeric::lower_migrated_integer(right.clone());
            !matches!(&lowered, Value::Native(_)) && legacy == &lowered
        }
        (Value::Tuple(left), Value::Tuple(right))
        | (Value::Array(left), Value::Array(right))
        | (Value::Vec(left), Value::Vec(right)) => sequence_equal(left, right),
        (Value::Reference(left), Value::Reference(right)) => Rc::ptr_eq(left, right),
        (Value::Option { value: None, .. }, Value::Option { value: None, .. }) => true,
        (
            Value::Option {
                value: Some(left), ..
            },
            Value::Option {
                value: Some(right), ..
            },
        ) => left == right,
        (Value::Result { value: left, .. }, Value::Result { value: right, .. }) => {
            match (left, right) {
                (Ok(left), Ok(right)) | (Err(left), Err(right)) => left == right,
                _ => false,
            }
        }
        (Value::HashMap(left), Value::HashMap(right)) => hash::hash_maps_equal(left, right),
        (Value::BTreeMap(left), Value::BTreeMap(right)) => hash::btree_maps_equal(left, right),
        (Value::BTreeSet(left), Value::BTreeSet(right)) => {
            left.entries.borrow().eq(&right.entries.borrow())
        }
        (Value::HashSet(left), Value::HashSet(right)) => {
            *left.entries.borrow() == *right.entries.borrow()
        }

        _ => false,
    }
}

fn sequence_equal(left: &IndexedStorage, right: &IndexedStorage) -> bool {
    let left = left.elements.borrow();
    let right = right.elements.borrow();
    left.len() == right.len()
        && left
            .iter()
            .zip(right.iter())
            .all(|(left, right)| left.value == right.value)
}
