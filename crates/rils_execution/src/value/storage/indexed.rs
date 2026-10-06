//! Recursive declaration application without cloning indexed payloads.

use super::*;
use crate::value::FieldSlot;

impl TypedStorageContext<'_> {
    pub(super) fn compose_indexed(&self, value: Value, expected: &Type) -> Result<Value, String> {
        if !self.needs_indexed_composition(&value, expected) {
            return Ok(value);
        }
        map_owned(value, expected, |value, ty, _| {
            self.apply_declared(value, ty)
        })
    }

    fn needs_indexed_composition(&self, value: &Value, expected: &Type) -> bool {
        match (value, expected) {
            (Value::Tuple(sequence), Type::Tuple(types)) => sequence
                .elements
                .borrow()
                .iter()
                .zip(types)
                .any(|(slot, ty)| {
                    slot.value
                        .as_ref()
                        .is_some_and(|value| self.needs_indexed_composition(value, ty))
                }),
            (Value::Array(sequence), Type::Array { element, .. }) => {
                if sequence.elements.borrow().is_empty()
                    && sequence.element_type.borrow().as_ref() != Some(element.as_ref())
                {
                    return true;
                }
                sequence.elements.borrow().iter().any(|slot| {
                    slot.value
                        .as_ref()
                        .is_some_and(|value| self.needs_indexed_composition(value, element))
                })
            }
            (Value::Option { .. }, Type::Option(_))
            | (Value::Result { .. }, Type::Result(_, _)) => self.layout(expected).is_ok(),
            _ => false,
        }
    }
}

/// Rebuild indexed storage by moving children; shared or borrowed owners are
/// rejected before any element is changed.
pub(super) fn map_owned(
    value: Value,
    expected: &Type,
    mut apply: impl FnMut(Value, &Type, usize) -> Result<Value, String>,
) -> Result<Value, String> {
    let (sequence, annotations, array) = match (value, expected) {
        (Value::Tuple(sequence), Type::Tuple(types)) => (sequence, types.clone(), false),
        (Value::Array(sequence), Type::Array { element, length }) => {
            let count = sequence.elements.borrow().len();
            if *length != count {
                return Err("array length does not match declaration".into());
            }
            (sequence, vec![(**element).clone(); count], true)
        }
        (value, _) => return Ok(value),
    };
    let element_type = match expected {
        Type::Array { element, .. } => Some((**element).clone()),
        _ => None,
    };
    let sequence = Rc::try_unwrap(sequence)
        .map_err(|_| "cannot compose native elements in shared indexed storage")?;
    if sequence.active_iterators.get() != 0 {
        return Err("cannot compose native elements during indexed iteration".into());
    }
    let elements = sequence.elements.into_inner();
    if elements.len() != annotations.len() {
        return Err("tuple arity does not match declaration".into());
    }
    if elements.iter().any(|slot| slot.references != 0) {
        return Err("cannot compose native elements with active element references".into());
    }
    let elements = elements
        .into_iter()
        .zip(annotations)
        .enumerate()
        .map(|(index, (slot, annotation))| {
            let value = slot
                .value
                .ok_or("cannot compose partially moved indexed storage")?;
            let value = apply(value, &annotation, index)?;
            Ok(FieldSlot::new(annotation, value))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let sequence = Rc::new(super::super::IndexedStorage {
        elements: std::cell::RefCell::new(elements),
        element_type: std::cell::RefCell::new(element_type),
        active_iterators: sequence.active_iterators,
    });
    Ok(if array {
        Value::Array(sequence)
    } else {
        Value::Tuple(sequence)
    })
}
