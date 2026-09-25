use super::*;

impl Interpreter {
    pub(super) fn call_native_symbol(
        &mut self,
        symbol: &str,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
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
