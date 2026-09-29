use super::*;

impl Interpreter {
    pub(super) fn call_native_symbol(
        &mut self,
        symbol: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        match rils_builtins::native_member(symbol).and_then(|member| member.builtin_id) {
            Some(rils_builtins::BuiltinId::FormatterWriteStr) => {
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
            Some(rils_builtins::BuiltinId::FormatterWriteDerivedDebug) => {
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
        let result = crate::runtime_builtins::call_native_symbol_with_callback(
            symbol,
            arguments,
            &mut |function, values| self.call(function.clone(), values, span),
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
