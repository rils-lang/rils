use super::*;
use crate::environment::StorageSlot;

mod native_enum;
mod native_record;
mod native_sum;

pub(super) fn pattern_matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
) -> bool {
    if matches!(pattern, Pattern::Wildcard { .. }) {
        return true;
    }
    if let Some(matches) = native_sum::matches(pattern, value, bindings, environment, false) {
        return matches;
    }
    if let Some(matches) = native_record::matches(pattern, value, bindings, environment, false) {
        return matches;
    }
    if let Some(matches) = native_enum::matches(pattern, value, bindings, environment, false) {
        return matches;
    }
    if let (Pattern::Binding { name, .. }, Value::Reference(_)) = (pattern, value) {
        bindings.push((name.clone(), value.clone()));
        return true;
    }
    let borrowed_value = match value {
        Value::Reference(reference) => reference.read().ok(),
        _ => None,
    };
    pattern_matches_inner(
        pattern,
        borrowed_value.as_ref().unwrap_or(value),
        bindings,
        environment,
        borrowed_value.is_some(),
    )
}

fn pattern_matches_inner(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
    borrowed: bool,
) -> bool {
    if let Some(matches) = native_sum::matches(pattern, value, bindings, environment, borrowed) {
        return matches;
    }
    if let Some(matches) = native_record::matches(pattern, value, bindings, environment, borrowed) {
        return matches;
    }
    if let Some(matches) = native_enum::matches(pattern, value, bindings, environment, borrowed) {
        return matches;
    }
    match pattern {
        Pattern::Wildcard { .. } => true,
        Pattern::Binding { name, .. } => {
            let bound = if borrowed {
                let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
                slot.borrow_mut().initialize(value.clone());
                Value::Reference(Rc::new(ReferenceValue::new_storage(slot, false)))
            } else {
                value.clone()
            };
            bindings.push((name.clone(), bound));
            true
        }
        Pattern::Literal { value: literal, .. } => literal_value(literal) == *value,
        Pattern::Some { inner, .. } => match value {
            Value::Option {
                value: Some(value), ..
            } => pattern_matches_inner(inner, value, bindings, environment, borrowed),
            _ => false,
        },
        Pattern::None { .. } => matches!(value, Value::Option { value: None, .. }),
        Pattern::Ok { inner, .. } => match value {
            Value::Result {
                value: Ok(value), ..
            } => pattern_matches_inner(inner, value, bindings, environment, borrowed),
            _ => false,
        },
        Pattern::Err { inner, .. } => match value {
            Value::Result {
                value: Err(value), ..
            } => pattern_matches_inner(inner, value, bindings, environment, borrowed),
            _ => false,
        },
        Pattern::TupleVariant { .. } | Pattern::Record { .. } | Pattern::Path { .. } => false,
    }
}

fn struct_type_matches(
    type_path: &[String],
    actual: &Rc<StructType>,
    environment: &EnvironmentRef,
) -> bool {
    matches!(
        execution::resolve_visible_path(type_path, environment, Span::default()),
        Ok(Value::StructType(expected)) if Rc::ptr_eq(&expected, actual)
    )
}

fn nominal_type_matches(
    variant_path: &[String],
    actual: &Rc<EnumType>,
    environment: &EnvironmentRef,
) -> bool {
    let Some((_, type_path)) = variant_path.split_last() else {
        return false;
    };
    matches!(
        execution::resolve_visible_path(type_path, environment, Span::default()),
        Ok(Value::EnumType(expected)) if Rc::ptr_eq(&expected, actual)
    )
}

pub(super) fn literal_value(literal: &Literal) -> Value {
    match literal {
        Literal::Unit => Value::Unit,
        Literal::Bool(value) => Value::Bool(*value),
        Literal::I8(value) => crate::numeric::native_i8(*value),
        Literal::I16(value) => crate::numeric::native_i16(*value),
        Literal::I32(value) => crate::numeric::native_i32(*value),
        Literal::I64(value) => crate::numeric::native_i64(*value),
        Literal::I128(value) => crate::numeric::native_i128(*value),
        Literal::Isize(value) => crate::numeric::native_isize(*value),
        Literal::U8(value) => crate::numeric::native_u8(*value),
        Literal::U16(value) => crate::numeric::native_u16(*value),
        Literal::U32(value) => crate::numeric::native_u32(*value),
        Literal::U64(value) => crate::numeric::native_u64(*value),
        Literal::U128(value) => crate::numeric::native_u128(*value),
        Literal::Usize(value) => crate::numeric::native_usize(*value),
        Literal::F32(value) => Value::from_f32(*value),
        Literal::F64(value) => Value::from_f64(*value),
        Literal::Char(value) => rils_execution::value::native_char(*value),
        Literal::Integer(value) => crate::numeric::native_i32(
            i32::try_from(*value).expect("unresolved integer pattern must fit the i32 default"),
        ),
        Literal::Float(value) => Value::from_f64(*value),
        Literal::String(value) => rils_execution::value::native_string(value.clone()),
    }
}

pub(super) fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Module { span, .. }
        | Stmt::Use { span, .. }
        | Stmt::Let { span, .. }
        | Stmt::Function { span, .. }
        | Stmt::Struct { span, .. }
        | Stmt::Enum { span, .. }
        | Stmt::TypeAlias { span, .. }
        | Stmt::Impl { span, .. }
        | Stmt::Trait { span, .. }
        | Stmt::While { span, .. }
        | Stmt::Loop { span, .. }
        | Stmt::For { span, .. }
        | Stmt::Return { span, .. }
        | Stmt::Break { span, .. }
        | Stmt::Continue { span, .. } => *span,
        Stmt::Expr { expression, .. } => expression.span(),
    }
}
