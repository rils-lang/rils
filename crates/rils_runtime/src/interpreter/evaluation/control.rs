use super::super::*;

impl Interpreter {
    pub(super) fn evaluate_control(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Call {
                callee,
                arguments,
                span,
            } => self.evaluate_call(callee, arguments, *span, environment, expression),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => self.evaluate_if(condition, then_branch, else_branch.as_deref(), environment),
            _ => self.evaluate_other_control(expression, environment),
        }
    }

    fn evaluate_call(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
        span: Span,
        environment: EnvironmentRef,
        expression: &Expr,
    ) -> Result<Value, RuntimeError> {
        let callee_value = self.evaluate(callee, environment.clone())?;
        let arguments = arguments
            .iter()
            .map(|argument| self.evaluate(argument, environment.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        if let Expr::GenericPath {
            segments,
            arguments: type_arguments,
            ..
        } = callee
            && let Some((member, owner)) = segments.split_last()
            && let Some(owner) =
                rils_frontend::standard_library::builtin_type_name(&owner.join("::"))
            && let Some(declaration) = rils_builtins::builtin(owner)
            && let Some(signature) =
                rils_frontend::standard_library::builtin_associated_function_signature(
                    owner, member,
                )
        {
            if declaration.type_parameters.len() != type_arguments.len() {
                return Err(RuntimeError::new(
                    format!(
                        "`{owner}` expects {} type arguments, found {}",
                        declaration.type_parameters.len(),
                        type_arguments.len()
                    ),
                    span,
                ));
            }
            let substitutions = declaration
                .type_parameters
                .iter()
                .zip(type_arguments)
                .map(|(name, ty)| ((*name).to_owned(), ty.clone()))
                .collect();
            if let Some(parameters) = signature.parameters {
                for (expected, actual) in parameters.iter().zip(&arguments) {
                    let expected = expected.substitute(&substitutions);
                    let actual = Type::of_value(actual).unwrap_or(Type::Unknown);
                    if merge_types(&expected, &actual).is_none() {
                        return Err(RuntimeError::new(
                            format!("argument expects `{expected}`, found `{actual}`"),
                            span,
                        ));
                    }
                }
            }
        }
        if let Value::VariantConstructor(constructor) = callee_value {
            let expected = self.constructor_type(expression, &environment)?;
            return self.construct_tuple_variant(constructor, arguments, span, expected.as_ref());
        }
        let expected = self.constructor_type(expression, &environment)?;
        self.call_owned_with_type(
            callee_value,
            arguments,
            span,
            environment,
            expected.as_ref(),
        )
    }

    fn evaluate_if(
        &mut self,
        condition: &Expr,
        then_branch: &Block,
        else_branch: Option<&Expr>,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        let condition_value = self.evaluate(condition, environment.clone())?;
        if self.condition_value(&condition_value, condition.span())? {
            let flow = self.execute_block(then_branch, environment)?;
            Ok(self.flow_value(flow))
        } else if let Some(else_branch) = else_branch {
            self.evaluate(else_branch, environment)
        } else {
            Ok(Value::Unit)
        }
    }

    fn evaluate_other_control(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        match expression {
            Expr::Try { operand, span } => {
                if self.function_depth == 0 {
                    return Err(RuntimeError::new(
                        "the `?` operator can only be used inside a function",
                        *span,
                    ));
                }
                let value = self.evaluate(operand, environment.clone())?;
                let (structs, enums) = environment.borrow().visible_type_definitions();
                let value = crate::value::sum::try_result(value, &structs, &enums)
                    .map_err(|message| RuntimeError::new(message, *span))?;
                match value {
                    Ok(value) => Ok(value),
                    Err(error) => {
                        self.pending_return = Some(error);
                        Err(RuntimeError::new(TRY_RETURN_SIGNAL, *span))
                    }
                }
            }
            Expr::Call { .. } | Expr::If { .. } => {
                unreachable!("call and if expressions use dedicated evaluators")
            }
            Expr::Match {
                value, arms, span, ..
            } => {
                let value = self.evaluate(value, environment.clone())?;
                for arm in arms {
                    self.tick(arm.pattern.span())?;
                    let mut bindings = Vec::new();
                    if pattern_matches(&arm.pattern, &value, &mut bindings, &environment) {
                        let branch_environment = Environment::child(environment.clone());
                        for (name, value) in bindings {
                            branch_environment
                                .borrow_mut()
                                .define(name, value, false, None);
                        }
                        let result = self.evaluate(&arm.expression, branch_environment)?;
                        return Ok(result);
                    }
                }
                Err(RuntimeError::new(
                    format!("non-exhaustive match for value `{value}`"),
                    *span,
                ))
            }
            Expr::Block(block) => {
                let flow = self.execute_block(block, environment)?;
                Ok(self.flow_value(flow))
            }
            _ => unreachable!("control evaluator received a non-control expression"),
        }
    }
}
