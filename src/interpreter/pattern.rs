use super::*;

pub(super) fn pattern_matches(
    pattern: &Pattern,
    value: &Value,
    bindings: &mut Vec<(String, Value)>,
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
            } => pattern_matches(inner, value, bindings),
            _ => false,
        },
        Pattern::None { .. } => matches!(value, Value::Option { value: None, .. }),
        Pattern::Ok { inner, .. } => match value {
            Value::Result {
                value: Ok(value), ..
            } => pattern_matches(inner, value, bindings),
            _ => false,
        },
        Pattern::Err { inner, .. } => match value {
            Value::Result {
                value: Err(value), ..
            } => pattern_matches(inner, value, bindings),
            _ => false,
        },
        Pattern::TupleVariant { path, fields, .. } => {
            if path.len() < 2 {
                return false;
            }
            let enum_name = &path[path.len() - 2];
            let variant_name = &path[path.len() - 1];
            let Value::Enum(instance) = value else {
                return false;
            };
            let EnumPayload::Tuple(values) = &instance.payload else {
                return false;
            };
            type_name_matches(&instance.type_definition.name, enum_name)
                && instance.variant == *variant_name
                && fields.len() == values.len()
                && fields
                    .iter()
                    .zip(values)
                    .all(|(pattern, value)| pattern_matches(pattern, value, bindings))
        }
        Pattern::Record { path, fields, .. } => {
            if let Value::Struct(instance) = value
                && path
                    .last()
                    .is_some_and(|name| type_name_matches(&instance.type_definition.name, name))
            {
                let values = instance.fields.borrow();
                return fields.len() == values.len()
                    && fields.iter().all(|(name, pattern)| {
                        values
                            .get(name)
                            .and_then(|field| field.value.as_ref())
                            .is_some_and(|value| pattern_matches(pattern, value, bindings))
                    });
            }
            let values = match value {
                Value::Enum(instance)
                    if path.len() >= 2
                        && type_name_matches(
                            &instance.type_definition.name,
                            &path[path.len() - 2],
                        )
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
                        .is_some_and(|value| pattern_matches(pattern, value, bindings))
                })
        }
        Pattern::Path { path, .. } => {
            if path.len() < 2 {
                return false;
            }
            let enum_name = &path[path.len() - 2];
            let variant_name = &path[path.len() - 1];
            matches!(
                value,
                Value::Enum(instance)
                    if type_name_matches(&instance.type_definition.name, enum_name)
                        && instance.variant == *variant_name
                        && matches!(instance.payload, EnumPayload::Unit)
            )
        }
    }
}

fn type_name_matches(canonical: &str, pattern: &str) -> bool {
    canonical == pattern || canonical.rsplit("::").next() == Some(pattern)
}

pub(super) fn literal_value(literal: &Literal) -> Value {
    match literal {
        Literal::Unit => Value::Unit,
        Literal::Bool(value) => Value::Bool(*value),
        Literal::Integer(value) => Value::Integer(*value),
        Literal::Float(value) => Value::Float(*value),
        Literal::String(value) => Value::String(Rc::from(value.as_str())),
    }
}

pub(super) fn statement_span(statement: &Stmt) -> Span {
    match statement {
        Stmt::Public { span, .. }
        | Stmt::Module { span, .. }
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
