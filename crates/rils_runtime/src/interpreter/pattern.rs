use super::*;

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
    match pattern {
        Pattern::Wildcard { .. } => true,
        Pattern::Binding { name, .. } => {
            bindings.push((name.clone(), value.clone()));
            true
        }
        Pattern::Literal { value: literal, .. } => {
            crate::value::equality::literal_matches(&literal_value(literal), value).unwrap_or(false)
        }
        Pattern::Some { .. } | Pattern::None { .. } | Pattern::Ok { .. } | Pattern::Err { .. } => {
            false
        }
        Pattern::TupleVariant { .. } | Pattern::Record { .. } | Pattern::Path { .. } => false,
    }
}

fn struct_type_matches(
    type_path: &[String],
    actual: &Rc<StructType>,
    environment: &EnvironmentRef,
) -> bool {
    matches!(
        execution::resolve_visible_declaration(type_path, environment, Span::default()),
        Ok(crate::value::Declaration::StructType(expected)) if Rc::ptr_eq(&expected, actual)
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
        execution::resolve_visible_declaration(type_path, environment, Span::default()),
        Ok(crate::value::Declaration::EnumType(expected)) if Rc::ptr_eq(&expected, actual)
    )
}

pub(super) fn literal_value(literal: &Literal) -> Value {
    match literal {
        Literal::Unit => Value::from(rils_execution::value::native_unit()),
        Literal::Bool(value) => Value::from(rils_execution::value::native_bool(*value)),
        Literal::I8(value) => Value::from(crate::numeric::native_i8(*value)),
        Literal::I16(value) => Value::from(crate::numeric::native_i16(*value)),
        Literal::I32(value) => Value::from(crate::numeric::native_i32(*value)),
        Literal::I64(value) => Value::from(crate::numeric::native_i64(*value)),
        Literal::I128(value) => Value::from(crate::numeric::native_i128(*value)),
        Literal::Isize(value) => Value::from(crate::numeric::native_isize(*value)),
        Literal::U8(value) => Value::from(crate::numeric::native_u8(*value)),
        Literal::U16(value) => Value::from(crate::numeric::native_u16(*value)),
        Literal::U32(value) => Value::from(crate::numeric::native_u32(*value)),
        Literal::U64(value) => Value::from(crate::numeric::native_u64(*value)),
        Literal::U128(value) => Value::from(crate::numeric::native_u128(*value)),
        Literal::Usize(value) => Value::from(crate::numeric::native_usize(*value)),
        Literal::F32(value) => Value::from(rils_execution::value::native_f32(*value)),
        Literal::F64(value) => Value::from(rils_execution::value::native_f64(*value)),
        Literal::Char(value) => Value::from(rils_execution::value::native_char(*value)),
        Literal::Integer(value) => Value::from(crate::numeric::native_i32(
            i32::try_from(*value).expect("unresolved integer pattern must fit the i32 default"),
        )),
        Literal::Float(value) => Value::from(rils_execution::value::native_f64(*value)),
        Literal::String(value) => Value::from(rils_execution::value::native_string(value.clone())),
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
