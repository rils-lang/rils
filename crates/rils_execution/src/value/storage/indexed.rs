//! Recursive declaration application without cloning indexed payloads.

use super::*;
use crate::value::FieldSlot;

impl TypedStorageContext<'_> {
    pub(super) fn compose_indexed(&self, value: Value, expected: &Type) -> Result<Value, String> {
        if !self.needs_indexed_composition(&value, expected) {
            return Ok(value);
        }
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
            .map(|(slot, annotation)| {
                let value = slot
                    .value
                    .ok_or("cannot compose partially moved indexed storage")?;
                let value = self.apply_declared(value, &annotation)?;
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
                sequence.elements.borrow().iter().any(|slot| {
                    slot.value
                        .as_ref()
                        .is_some_and(|value| self.needs_indexed_composition(value, element))
                })
            }
            (Value::Option { .. }, Type::Option(_))
            | (Value::Result { .. }, Type::Result(_, _)) => {
                RecordLayoutResolver::with_enums(self.structs, self.enums)
                    .resolve(expected)
                    .is_ok()
            }
            _ => false,
        }
    }
}
