use super::*;

pub(super) fn resolve_numeric_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    let method = match value {
        Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::Isize(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_) => rils_builtins::integer_method(name)
            .map(|method| BuiltinMethod::IntegerIntrinsic(method.id)),
        Value::F32(_) | Value::F64(_) => {
            rils_builtins::float_method(name).map(|method| BuiltinMethod::FloatIntrinsic(method.id))
        }
        _ => return Ok(None),
    }
    .ok_or_else(|| {
        RuntimeError::new(
            format!("{} has no method `{name}`", value.type_name()),
            span,
        )
    })?;
    Ok(Some(Value::BuiltinBoundMethod(Rc::new(
        BuiltinBoundMethod {
            receiver: Rc::new(value.clone()),
            method,
        },
    ))))
}

pub(super) fn resolve_host_or_builtin_member(
    value: &Value,
    name: &str,
    span: Span,
) -> Result<Option<Value>, RuntimeError> {
    if let Some((id, _)) = builtin_runtime_member(value, name) {
        return Ok(Some(Value::BuiltinBoundMethod(Rc::new(
            BuiltinBoundMethod {
                receiver: Rc::new(value.clone()),
                method: BuiltinMethod::Runtime(id),
            },
        ))));
    }
    let Value::HostObject(instance) = value else {
        return Ok(None);
    };
    let function = instance
        .type_definition
        .methods
        .borrow()
        .get(name)
        .cloned()
        .ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type `{}` has no method `{name}`",
                    instance.type_definition.name
                ),
                span,
            )
        })?;
    Ok(Some(Value::HostBoundMethod(Rc::new(HostBoundMethod {
        receiver: Rc::new(value.clone()),
        function,
    }))))
}
