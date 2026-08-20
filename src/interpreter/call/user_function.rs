use super::*;

const INTERPRETER_STACK_RED_ZONE: usize = 128 * 1024;
const INTERPRETER_STACK_SEGMENT: usize = 2 * 1024 * 1024;

impl Interpreter {
    pub(super) fn call_user_function(
        &mut self,
        function: Rc<UserFunction>,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        if self.function_depth >= self.limits.max_call_depth {
            return Err(RuntimeError::new(
                format!(
                    "call stack exceeded the {} frame limit",
                    self.limits.max_call_depth
                ),
                span,
            ));
        }
        stacker::maybe_grow(
            INTERPRETER_STACK_RED_ZONE,
            INTERPRETER_STACK_SEGMENT,
            || self.call_user_function_inner(function, arguments, span),
        )
    }

    fn call_user_function_inner(
        &mut self,
        function: Rc<UserFunction>,
        arguments: &[Value],
        span: Span,
    ) -> Result<Value, RuntimeError> {
        check_arity(
            &function.name,
            function.parameters.len(),
            function.parameters.len(),
            arguments.len(),
            span,
        )?;
        let environment = Environment::child(function.closure.clone());
        let mut substitutions: HashMap<String, Type> = function
            .generic_parameters
            .iter()
            .map(|parameter| (parameter.name.clone(), Type::Unknown))
            .collect();
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            if let Some(parameter_type) = &parameter.type_annotation {
                infer_type_from_value(parameter_type, argument, &mut substitutions)
                    .map_err(|message| RuntimeError::new(message, span))?;
            }
        }
        validate_generic_bounds(
            &function.generic_parameters,
            &substitutions,
            &function.closure,
            span,
        )?;
        for (parameter, argument) in function.parameters.iter().zip(arguments) {
            let expected = parameter
                .type_annotation
                .as_ref()
                .map(|value| {
                    expand_type_aliases(&value.substitute(&substitutions), &function.closure, span)
                })
                .transpose()?;
            let argument = apply_type(expected.as_ref(), argument, span, &parameter.name)?;
            environment.borrow_mut().define(
                parameter.name.clone(),
                argument,
                parameter.mutable,
                parameter.type_annotation.clone(),
            );
        }
        self.function_depth += 1;
        let result = self.execute_statements(&function.body.statements, environment);
        self.function_depth -= 1;
        let result = match result {
            Err(error) if error.message == TRY_RETURN_SIGNAL => {
                let value = self.pending_return.take().ok_or_else(|| {
                    RuntimeError::new("missing Result value for `?` return", span)
                })?;
                Ok(Flow::Return(value))
            }
            result => result,
        };
        match result {
            Ok(Flow::Value(value) | Flow::Return(value)) => {
                if value.contains_reference() {
                    return Err(RuntimeError::new(
                        "references cannot be returned from functions",
                        span,
                    ));
                }
                if let Some(return_type) = &function.return_type {
                    let return_type = expand_type_aliases(
                        &return_type.substitute(&substitutions),
                        &function.closure,
                        span,
                    )?;
                    infer_type_from_value(&return_type, &value, &mut substitutions)
                        .map_err(|message| RuntimeError::new(message, span))?;
                }
                validate_generic_bounds(
                    &function.generic_parameters,
                    &substitutions,
                    &function.closure,
                    span,
                )?;
                let expected = function
                    .return_type
                    .as_ref()
                    .map(|value| {
                        expand_type_aliases(
                            &value.substitute(&substitutions),
                            &function.closure,
                            span,
                        )
                    })
                    .transpose()?;
                let value = apply_type(
                    expected.as_ref(),
                    &value,
                    span,
                    &format!("return value of `{}`", function.name),
                )?;
                Ok(value)
            }
            Ok(Flow::Break(_) | Flow::Continue) => Err(RuntimeError::new(
                "loop control cannot escape a function",
                span,
            )),
            Err(mut error) => {
                error.stack.push(function.name.clone());
                Err(error)
            }
        }
    }
}
