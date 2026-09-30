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
        match method.method {
            BuiltinMethod::Native(symbol) => {
                let mut values = Vec::with_capacity(arguments.len() + 1);
                values.push((*method.receiver).clone());
                values.extend_from_slice(arguments);
                self.call_native_symbol(symbol, &values, span)
            }
            BuiltinMethod::IteratorIdentity => Ok((*method.receiver).clone()),
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
