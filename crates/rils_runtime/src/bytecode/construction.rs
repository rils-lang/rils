use std::{cell::RefCell, rc::Rc};

use crate::{
    source::Span,
    types::Type,
    value::{FieldSlot, SequenceValue, Value},
};

use super::BytecodeError;

pub(super) fn sequence_value(
    values: Vec<Value>,
    array: bool,
    span: Span,
) -> Result<Value, BytecodeError> {
    let mut element_type = Type::Unknown;
    if array {
        for value in &values {
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            element_type = merge_sequence_types(&element_type, &actual).ok_or_else(|| {
                BytecodeError::new(
                    format!(
                        "array elements must have one type, found `{element_type}` and `{actual}`"
                    ),
                    span,
                )
            })?;
        }
    }
    let elements = values
        .into_iter()
        .map(|value| FieldSlot {
            type_annotation: if array {
                element_type.clone()
            } else {
                Type::of_value(&value).unwrap_or(Type::Unknown)
            },
            value: Some(value),
            references: 0,
        })
        .collect();
    let sequence = Rc::new(SequenceValue {
        elements: RefCell::new(elements),
        element_type: RefCell::new(array.then_some(element_type)),
    });
    Ok(if array {
        Value::Array(sequence)
    } else {
        Value::Tuple(sequence)
    })
}

fn merge_sequence_types(left: &Type, right: &Type) -> Option<Type> {
    if left == &Type::Unknown {
        return Some(right.clone());
    }
    if right == &Type::Unknown || left == right {
        return Some(left.clone());
    }
    None
}
