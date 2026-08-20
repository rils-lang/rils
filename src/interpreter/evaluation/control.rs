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
            } => self.evaluate_call(callee, arguments, *span, environment),
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
    ) -> Result<Value, RuntimeError> {
        let callee = self.evaluate(callee, environment.clone())?;
        let arguments = arguments
            .iter()
            .map(|argument| self.evaluate(argument, environment.clone()))
            .collect::<Result<Vec<_>, _>>()?;
        self.call(callee, &arguments, span)
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
                let value = self.evaluate(operand, environment)?;
                let Value::Result {
                    value, error_type, ..
                } = value
                else {
                    return Err(RuntimeError::new(
                        format!(
                            "the `?` operator requires Result, found {}",
                            value.type_name()
                        ),
                        *span,
                    ));
                };
                match value {
                    Ok(value) => Rc::try_unwrap(value)
                        .or_else(|value| value.clone_owned())
                        .map_err(|message| RuntimeError::new(message, *span)),
                    Err(error) => {
                        self.pending_return = Some(Value::Result {
                            value: Err(error),
                            ok_type: None,
                            error_type,
                        });
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
                    if pattern_matches(&arm.pattern, &value, &mut bindings) {
                        let branch_environment = Environment::child(environment);
                        for (name, value) in bindings {
                            branch_environment
                                .borrow_mut()
                                .define(name, value, false, None);
                        }
                        let result = self.evaluate(&arm.expression, branch_environment)?;
                        if result.contains_reference() {
                            return Err(RuntimeError::new(
                                "reference cannot escape its match arm",
                                arm.expression.span(),
                            ));
                        }
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
