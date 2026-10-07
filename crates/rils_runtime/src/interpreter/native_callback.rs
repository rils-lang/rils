use super::*;

impl Interpreter {
    pub(super) fn call_native_symbol(
        &mut self,
        symbol: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let formatter_member =
            rils_builtins::native_member_owner(symbol).and_then(|(owner, member)| {
                rils_builtins::builtin("Formatter")
                    .filter(|formatter| std::ptr::eq(owner, *formatter))
                    .map(|_| member.name)
            });
        match formatter_member {
            Some("write_str") => {
                let [receiver, value] = arguments else {
                    return Err(RuntimeError::new(
                        "Formatter::write_str expects a receiver and string",
                        span,
                    ));
                };
                let buffer = super::formatting::formatter_buffer(receiver, span)?;
                let value = value.as_string().ok_or_else(|| {
                    RuntimeError::new("Formatter::write_str expects string", span)
                })?;
                buffer.write_str(&value);
                return Ok(super::builtin_methods::format_ok());
            }
            Some("write_derived_debug") => {
                let [receiver, value] = arguments else {
                    return Err(RuntimeError::new(
                        "Formatter::write_derived_debug expects two arguments",
                        span,
                    ));
                };
                self.write_derived_debug(receiver, value, span)?;
                return Ok(super::builtin_methods::format_ok());
            }
            _ => {}
        }
        crate::runtime_builtins::call_native_symbol(symbol, arguments)
            .ok_or_else(|| {
                RuntimeError::new(format!("native method `{symbol}` is unavailable"), span)
            })?
            .map_err(|message| RuntimeError::new(message, span))
    }

    pub(super) fn call_native_owned_symbol(
        &mut self,
        symbol: &str,
        arguments: Vec<Value>,
        expected: Option<&Type>,
        span: Span,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        let context =
            crate::runtime_builtins::NativeOwnedContext::from_environment(&environment.borrow());
        let result = crate::runtime_builtins::call_native_symbol_with_callback(
            symbol,
            arguments,
            &context,
            expected,
            &mut |function, values| {
                self.call_owned(function.clone(), values, span, environment.clone())
            },
        )
        .ok_or_else(|| {
            RuntimeError::new(format!("native method `{symbol}` is unavailable"), span)
        })?;
        result.map_err(|error| match error {
            crate::runtime_builtins::NativeCallError::Bridge(message) => {
                RuntimeError::new(message, span)
            }
            crate::runtime_builtins::NativeCallError::Callback(error) => error,
        })
    }
}
