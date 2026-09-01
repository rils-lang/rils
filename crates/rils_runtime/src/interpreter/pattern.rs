use super::*;

pub(super) fn pattern_matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
    environment: &EnvironmentRef,
) -> bool {
    match pattern {
        Pattern::Wildcard { .. } => true,
        Pattern::Binding { name, .. } => {
            bindings.push((name.clone(), value.clone()));
            true
        }
        Pattern::Literal { value: literal, .. } => literal_value(literal) == *value,
        Pattern::Some { inner, .. } => match value {
            Value::Option {
                value: Some(value), ..
            } => pattern_matches(inner, value, bindings, environment),
            _ => false,
        },
        Pattern::None { .. } => matches!(value, Value::Option { value: None, .. }),
        Pattern::Ok { inner, .. } => match value {
            Value::Result {
                value: Ok(value), ..
            } => pattern_matches(inner, value, bindings, environment),
            _ => false,
        },
        Pattern::Err { inner, .. } => match value {
            Value::Result {
                value: Err(value), ..
            } => pattern_matches(inner, value, bindings, environment),
            _ => false,
        },
        Pattern::TupleVariant { path, fields, .. } => {
            if path.len() < 2 {
                return false;
            }
            let variant_name = &path[path.len() - 1];
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Tuple(values) = &instance.payload else {
                return false;
            };
            nominal_type_matches(path, &instance.type_definition, environment)
                && instance.variant == *variant_name
                && fields.len() == values.len()
                && fields
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value, bindings, environment))
        }
        Pattern::Record { path, fields, .. } => {
            if let Value::Struct(instance) = value
                && struct_type_matches(path, &instance.type_definition, environment)
            {
                let values = instance.fields.borrow();
                return fields.len() == values.len()
                    && fields.iter().all(|(name, pattern)| {
                        values
                            .get(name)
                            .and_then(|field| field.value.as_ref())
                            .is_some_and(|value| {
                                pattern_matches(pattern, value, bindings, environment)
                            })
                    });
            }
            let values = match value {
                Value::Enum(instance)
                    if path.len() >= 2
                        && nominal_type_matches(path, &instance.type_definition, environment)
                        && instance.variant == path[path.len() - 1] =>
                {
                    let EnumPayload::Record(values) = &instance.payload else {
                        return false;
                    };
                    values
                }
                _ => return false,
            };
            fields.len() == values.len()
                && fields.iter().all(|(name, pattern)| {
                    values
                        .get(name)
                        .is_some_and(|value| pattern_matches(pattern, value, bindings, environment))
                })
        }
        Pattern::Path { path, .. } => {
            if path.len() < 2 {
                return false;
            }
            let variant_name = &path[path.len() - 1];
            matches!(
                value,
                Value::Enum(instance)
                    if nominal_type_matches(path, &instance.type_definition, environment)
                        && instance.variant == *variant_name
                        && matches!(instance.payload, EnumPayload::Unit)
            )
        }
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
        Literal::I8(value) => Value::I8(*value),
        Literal::I16(value) => Value::I16(*value),
        Literal::I32(value) => Value::I32(*value),
        Literal::I64(value) => Value::I64(*value),
        Literal::I128(value) => Value::I128(*value),
        Literal::Isize(value) => Value::Isize(*value),
        Literal::U8(value) => Value::U8(*value),
        Literal::U16(value) => Value::U16(*value),
        Literal::U32(value) => Value::U32(*value),
        Literal::U64(value) => Value::U64(*value),
        Literal::U128(value) => Value::U128(*value),
        Literal::Usize(value) => Value::Usize(*value),
        Literal::F32(value) => Value::F32(*value),
        Literal::F64(value) => Value::F64(*value),
        Literal::Char(value) => Value::Char(*value),
        Literal::Integer(value) => Value::I32(
            i32::try_from(*value).expect("unresolved integer pattern must fit the i32 default"),
        ),
        Literal::Float(value) => Value::F64(*value),
        Literal::String(value) => Value::String(Rc::from(value.as_str())),
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
