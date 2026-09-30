use super::*;

impl Interpreter {
    pub(super) fn call_builtin_method(
        &mut self,
        method: &BuiltinBoundMethod,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let arity = match method.method {
            BuiltinMethod::IteratorIdentity => 0,
            BuiltinMethod::Runtime(id) => rils_builtins::runtime_member(id)
                .and_then(|(_, member)| member.signature)
                .map_or(0, |signature| signature.parameters.len()),
            BuiltinMethod::Native(symbol) => rils_builtins::native_member(symbol)
                .and_then(|member| member.signature)
                .map(|signature| signature.parameters.len())
                .or_else(|| {
                    rils_builtins::intrinsic_by_symbol(symbol)
                        .map(|(item, _)| item.signature.parameters.len())
                })
                .unwrap_or(0),
        };
        check_arity("builtin method", arity, arity, arguments.len(), span)?;
        if let BuiltinMethod::Runtime(id) = method.method
            && let Some(symbol) =
                rils_builtins::runtime_member(id).and_then(|(_, member)| member.native_symbol)
        {
            let mut values = Vec::with_capacity(arguments.len() + 1);
            values.push((*method.receiver).clone());
            values.extend_from_slice(arguments);
            return self.call_native_symbol(symbol, &values, span);
        }
        if let BuiltinMethod::Runtime(id) = method.method
            && rils_builtins::is_iterator_default_builtin(id)
            && let Some((_, member)) = rils_builtins::runtime_member(id)
            && let Some(bound) =
                self.bind_exported_iterator_default(method.receiver.as_ref(), member.name)
        {
            return self.call(bound, arguments, span);
        }
        match method.method {
            BuiltinMethod::Native(symbol) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                self.call_native_symbol(symbol, &values, span)
            }
            BuiltinMethod::IteratorIdentity => Ok((*method.receiver).clone()),
            BuiltinMethod::Runtime(id) => Err(RuntimeError::new(
                format!("unknown runtime member ID {:#x}", id.as_raw()),
                span,
            )),
        }
    }
}

pub(super) fn format_ok() -> Value {
    Value::Result {
        value: Ok(Rc::new(Value::Unit)),
        ok_type: Some(Type::Unit),
        error_type: Some(Type::named("FormatError")),
    }
}
