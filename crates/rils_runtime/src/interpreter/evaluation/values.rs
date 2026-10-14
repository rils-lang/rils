use super::super::*;

impl Interpreter {
    pub(super) fn evaluate_sequence(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Tuple { elements, span } => {
                let expected = self
                    .constructor_type(expression, &environment)?
                    .ok_or_else(|| {
                        RuntimeError::new("cannot infer the complete tuple type", *span)
                    })?;
                let mut values = Vec::with_capacity(elements.len());
                for element in elements {
                    values.push(self.evaluate(element, environment.clone())?);
                }
                let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                    &environment.borrow(),
                );
                context
                    .storage()
                    .construct_tuple(&expected, values)
                    .map_err(|message| RuntimeError::new(message, *span))
            }
            Expr::Array {
                elements,
                repeat,
                span,
            } => {
                let expected = self
                    .constructor_type(expression, &environment)?
                    .ok_or_else(|| {
                        RuntimeError::new("cannot infer the complete array type", *span)
                    })?;
                let mut values = Vec::new();
                if let Some(count) = repeat {
                    let value = self.evaluate(&elements[0], environment.clone())?;
                    if !value.is_copy() {
                        return Err(RuntimeError::new(
                            "array repeat syntax requires a Copy value",
                            *span,
                        ));
                    }
                    let count = self.evaluate(count, environment.clone())?;
                    let Ok(count) = count.get_cloned::<usize>() else {
                        return Err(RuntimeError::new("array repeat count must be usize", *span));
                    };
                    let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                        &environment.borrow(),
                    );
                    return context
                        .storage()
                        .construct_repeat_array(&expected, value, count)
                        .map_err(|message| RuntimeError::new(message, *span));
                } else {
                    for element in elements {
                        let value = self.evaluate(element, environment.clone())?;
                        values.push(value);
                    }
                }
                let context = crate::runtime_builtins::NativeOwnedContext::from_environment(
                    &environment.borrow(),
                );
                context
                    .storage()
                    .construct_array(&expected, values)
                    .map_err(|message| RuntimeError::new(message, *span))
            }
            _ => unreachable!("sequence evaluator received a non-sequence expression"),
        }
    }
}
