use std::rc::Rc;

use crate::{
    hir::{HirLiteral, HirPattern},
    value::Value,
};

mod native_enum;
mod native_record;
mod native_sum;

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
    if matches!(pattern, HirPattern::Wildcard | HirPattern::Binding(_)) {
        return true;
    }
    if let Some(matches) = native_sum::matches(pattern, value) {
        return matches;
    }
    if let Some(matches) = native_record::matches(pattern, value) {
        return matches;
    }
    if let Some(matches) = native_enum::matches(pattern, value) {
        return matches;
    }
    let borrowed = match value {
        Value::Reference(reference) => reference.read().ok(),
        _ => None,
    };
    let value = borrowed.as_ref().unwrap_or(value);
    if let Some(matches) = native_sum::matches(pattern, value) {
        return matches;
    }
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
        HirPattern::TupleVariant { .. } | HirPattern::Record { .. } | HirPattern::Path(_) => false,
    }
}

pub(super) fn collect_pattern_bindings(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
) -> Result<(), String> {
    if let Some(result) = native_sum::collect(pattern, value, bindings, false) {
        return result;
    }
    if let Some(result) = native_record::collect(pattern, value, bindings, false) {
        return result;
    }
    if let Some(result) = native_enum::collect(pattern, value, bindings, false) {
        return result;
    }
    if let (HirPattern::Binding(local), Value::Reference(_)) = (pattern, value) {
        bindings.push((*local, value.clone()));
        return Ok(());
    }
    let borrowed_value = match value {
        Value::Reference(reference) => reference.read().ok(),
        _ => None,
    };
    collect_pattern_bindings_inner(
        pattern,
        borrowed_value.as_ref().unwrap_or(value),
        bindings,
        borrowed_value.is_some(),
    )
}

fn collect_pattern_bindings_inner(
    pattern: &HirPattern,
    value: &Value,
    bindings: &mut Vec<(usize, Value)>,
    borrowed: bool,
) -> Result<(), String> {
    if let Some(result) = native_sum::collect(pattern, value, bindings, borrowed) {
        return result;
    }
    if let Some(result) = native_record::collect(pattern, value, bindings, borrowed) {
        return result;
    }
    if let Some(result) = native_enum::collect(pattern, value, bindings, borrowed) {
        return result;
    }
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
                collect_pattern_bindings_inner(inner, value, bindings, borrowed)?;
            }
        }
        HirPattern::Ok(inner) => {
            if let Value::Result {
                value: Ok(value), ..
            } = value
            {
                collect_pattern_bindings_inner(inner, value, bindings, borrowed)?;
            }
        }
        HirPattern::Err(inner) => {
            if let Value::Result {
                value: Err(value), ..
            } = value
            {
                collect_pattern_bindings_inner(inner, value, bindings, borrowed)?;
            }
        }
        HirPattern::TupleVariant { .. } | HirPattern::Record { .. } => {}
        HirPattern::Wildcard | HirPattern::Literal(_) | HirPattern::None | HirPattern::Path(_) => {}
    }
    Ok(())
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
