use std::rc::Rc;

use crate::{
    hir::{HirLiteral, HirPattern},
    value::{EnumPayload, Value},
};

pub(super) fn pattern_locals_valid(pattern: &HirPattern, local_count: usize) -> bool {
    match pattern {
        HirPattern::Binding(local) => *local < local_count,
        HirPattern::Some(inner) | HirPattern::Ok(inner) | HirPattern::Err(inner) => {
            pattern_locals_valid(inner, local_count)
        }
        HirPattern::TupleVariant { fields, .. } => fields
            .iter()
            .all(|pattern| pattern_locals_valid(pattern, local_count)),
        HirPattern::Record { fields, .. } => fields
            .iter()
            .all(|(_, pattern)| pattern_locals_valid(pattern, local_count)),
        HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::None | HirPattern::Path(_) => {
            true
        }
    }
}

pub(super) fn pattern_matches(pattern: &HirPattern, value: &Value) -> bool {
    let borrowed = match value {
        Value::Reference(reference) => reference.read().ok(),
        _ => None,
    };
    let value = borrowed.as_ref().unwrap_or(value);
    let materialized = match value.materialize_native_sum() {
        Some(Ok(value)) => Some(value),
        Some(Err(_)) => return false,
        None => None,
    };
    let value = materialized.as_ref().unwrap_or(value);
    match pattern {
        HirPattern::Wildcard | HirPattern::Binding(_) => true,
        HirPattern::Literal(literal) => hir_literal_value(literal) == *value,
        HirPattern::Some(inner) => {
            matches!(value, Value::Option { value: Some(value), .. } if pattern_matches(inner, value))
        }
        HirPattern::None => matches!(value, Value::Option { value: None, .. }),
        HirPattern::Ok(inner) => {
            matches!(value, Value::Result { value: Ok(value), .. } if pattern_matches(inner, value))
        }
        HirPattern::Err(inner) => {
            matches!(value, Value::Result { value: Err(value), .. } if pattern_matches(inner, value))
        }
        HirPattern::TupleVariant { path, fields } => {
            let Some((enum_path, variant_name)) = pattern_variant(path) else {
                return false;
            };
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Tuple(values) = &instance.payload else {
                return false;
            };
            type_path_matches(&instance.type_definition.name, enum_path)
                && instance.variant == variant_name
                && fields.len() == values.len()
                && fields
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value))
        }
        HirPattern::Record { path, fields } => {
            if let Value::Struct(instance) = value
                && type_path_matches(&instance.type_definition.name, path)
            {
                let values = instance.fields.borrow();
                return fields.len() == values.len()
                    && fields.iter().all(|(name, pattern)| {
                        values
                            .get(name)
                            .and_then(|field| field.value.as_ref())
                            .is_some_and(|value| pattern_matches(pattern, value))
                    });
            }
            let Some((enum_path, variant_name)) = pattern_variant(path) else {
                return false;
            };
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Record(values) = &instance.payload else {
                return false;
            };
            type_path_matches(&instance.type_definition.name, enum_path)
                && instance.variant == variant_name
                && fields.len() == values.len()
                && fields.iter().all(|(name, pattern)| {
                    values
                        .get(name)
                        .is_some_and(|value| pattern_matches(pattern, value))
                })
        }
        HirPattern::Path(path) => {
            let Some((enum_path, variant_name)) = pattern_variant(path) else {
                return false;
            };
            matches!(value, Value::Enum(instance) if type_path_matches(&instance.type_definition.name, enum_path)
                && instance.variant == variant_name && matches!(instance.payload, EnumPayload::Unit))
        }
    }
}

pub(super) fn collect_pattern_bindings(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
) {
    let borrowed_value = match value {
        Value::Reference(reference) => reference.read().ok(),
        _ => None,
    };
    collect_pattern_bindings_inner(
        pattern,
        borrowed_value.as_ref().unwrap_or(value),
        bindings,
        borrowed_value.is_some(),
    );
}

fn collect_pattern_bindings_inner(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
    borrowed: bool,
) {
    let materialized = value.materialize_native_sum().and_then(Result::ok);
    let value = materialized.as_ref().unwrap_or(value);
    match pattern {
        HirPattern::Binding(local) => {
            let bound = if borrowed {
                let slot = Rc::new(std::cell::RefCell::new(
                    crate::environment::StorageSlot::uninitialized(false),
                ));
                slot.borrow_mut().initialize(value.clone());
                Value::Reference(Rc::new(crate::value::ReferenceValue::new_storage(
                    slot, false,
                )))
            } else {
                value.clone()
            };
            bindings.push((*local, bound));
        }
        HirPattern::Some(inner) => {
            if let Value::Option {
                value: Some(value), ..
            } = value
            {
                collect_pattern_bindings_inner(inner, value, bindings, borrowed);
            }
        }
        HirPattern::Ok(inner) => {
            if let Value::Result {
                value: Ok(value), ..
            } = value
            {
                collect_pattern_bindings_inner(inner, value, bindings, borrowed);
            }
        }
        HirPattern::Err(inner) => {
            if let Value::Result {
                value: Err(value), ..
            } = value
            {
                collect_pattern_bindings_inner(inner, value, bindings, borrowed);
            }
        }
        HirPattern::TupleVariant { fields, .. } => {
            if let Value::Enum(instance) = value
                && let EnumPayload::Tuple(values) = &instance.payload
            {
                for (pattern, value) in fields.iter().zip(values) {
                    collect_pattern_bindings_inner(pattern, value, bindings, borrowed);
                }
            }
        }
        HirPattern::Record { fields, .. } => match value {
            Value::Struct(instance) => {
                let values = instance.fields.borrow();
                for (name, pattern) in fields {
                    if let Some(value) = values.get(name).and_then(|field| field.value.as_ref()) {
                        collect_pattern_bindings_inner(pattern, value, bindings, borrowed);
                    }
                }
            }
            Value::Enum(instance) => {
                if let EnumPayload::Record(values) = &instance.payload {
                    for (name, pattern) in fields {
                        if let Some(value) = values.get(name) {
                            collect_pattern_bindings_inner(pattern, value, bindings, borrowed);
                        }
                    }
                }
            }
            _ => {}
        },
        HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::None | HirPattern::Path(_) => {}
    }
}

fn hir_literal_value(literal: &HirLiteral) -> Value {
    match literal {
        HirLiteral::Unit => Value::Unit,
        HirLiteral::Bool(value) => Value::Bool(*value),
        HirLiteral::I8(value) => crate::numeric::native_i8(*value),
        HirLiteral::I16(value) => crate::numeric::native_i16(*value),
        HirLiteral::I32(value) => crate::numeric::native_i32(*value),
        HirLiteral::I64(value) => crate::numeric::native_i64(*value),
        HirLiteral::I128(value) => crate::numeric::native_i128(*value),
        HirLiteral::Isize(value) => crate::numeric::native_isize(*value),
        HirLiteral::U8(value) => crate::numeric::native_u8(*value),
        HirLiteral::U16(value) => crate::numeric::native_u16(*value),
        HirLiteral::U32(value) => crate::numeric::native_u32(*value),
        HirLiteral::U64(value) => crate::numeric::native_u64(*value),
        HirLiteral::U128(value) => crate::numeric::native_u128(*value),
        HirLiteral::Usize(value) => crate::numeric::native_usize(*value),
        HirLiteral::F32(value) => Value::from_f32(*value),
        HirLiteral::F64(value) => Value::from_f64(*value),
        HirLiteral::Char(value) => rils_execution::value::native_char(*value),
        HirLiteral::String(value) => rils_execution::value::native_string(value.clone()),
    }
}

fn pattern_variant(path: &[String]) -> Option<(&[String], &str)> {
    let (variant, type_path) = path.split_last()?;
    (!type_path.is_empty()).then_some((type_path, variant.as_str()))
}

fn type_path_matches(canonical: &str, pattern: &[String]) -> bool {
    canonical.split("::").eq(pattern.iter().map(String::as_str))
}
