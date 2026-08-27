use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn builtin_combinator(
        &mut self,
        builtin: Option<rils_builtins::BuiltinId>,
        name: &str,
        object: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HirExpression>, CompileError> {
        let Some(builtin) = builtin else {
            return Ok(None);
        };
        if rils_builtins::is_iterator_default_builtin(builtin) {
            return self
                .iterator_default(name, object, arguments, span)
                .map(Some);
        }
        use rils_builtins::BuiltinId;
        if !matches!(
            builtin,
            BuiltinId::OptionMap
                | BuiltinId::OptionAndThen
                | BuiltinId::OptionOrElse
                | BuiltinId::ResultMap
                | BuiltinId::ResultMapErr
                | BuiltinId::ResultAndThen
                | BuiltinId::ResultOrElse
        ) {
            return Ok(None);
        }
        let [callback] = arguments else {
            return Err(CompileError::unsupported(
                format!("`{name}` expects exactly one callback"),
                span,
            ));
        };
        let receiver_local = self.allocate_combinator_local();
        let callback_local = self.allocate_combinator_local();
        let binding_local = self.allocate_combinator_local();
        let local = |local| HirExpression::Local { local, span };
        let call = |arguments| HirExpression::CallValue {
            callee: Box::new(local(callback_local)),
            arguments,
            span,
        };
        let binding = || local(binding_local);
        let (first_pattern, first_expression, second_pattern, second_expression) = match builtin {
            BuiltinId::OptionMap => (
                HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                HirExpression::OptionSome {
                    value: Box::new(call(vec![binding()])),
                    span,
                },
                HirPattern::None,
                HirExpression::OptionNone { span },
            ),
            BuiltinId::OptionAndThen => (
                HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                call(vec![binding()]),
                HirPattern::None,
                HirExpression::OptionNone { span },
            ),
            BuiltinId::OptionOrElse => (
                HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                HirExpression::OptionSome {
                    value: Box::new(binding()),
                    span,
                },
                HirPattern::None,
                call(Vec::new()),
            ),
            BuiltinId::ResultMap => result_arms(binding_local, call(vec![binding()]), false, span),
            BuiltinId::ResultMapErr => {
                result_arms(binding_local, call(vec![binding()]), true, span)
            }
            BuiltinId::ResultAndThen => {
                result_flatten_arms(binding_local, call(vec![binding()]), false, span)
            }
            BuiltinId::ResultOrElse => {
                result_flatten_arms(binding_local, call(vec![binding()]), true, span)
            }
            _ => unreachable!(),
        };
        Ok(Some(HirExpression::Block {
            statements: vec![
                HirStatement::Let {
                    local: receiver_local,
                    initializer: self.expression(object)?,
                    span,
                },
                HirStatement::Let {
                    local: callback_local,
                    initializer: self.expression(callback)?,
                    span,
                },
                HirStatement::Expression {
                    expression: HirExpression::Match {
                        value: Box::new(local(receiver_local)),
                        arms: vec![
                            HirMatchArm {
                                pattern: first_pattern,
                                expression: first_expression,
                                span,
                            },
                            HirMatchArm {
                                pattern: second_pattern,
                                expression: second_expression,
                                span,
                            },
                        ],
                        span,
                    },
                    terminated: false,
                    span,
                },
            ],
            span,
        }))
    }

    pub(super) fn allocate_combinator_local(&mut self) -> LocalId {
        let local = self.mutable.len();
        self.mutable.push(false);
        local
    }
}

fn result_arms(
    binding: LocalId,
    mapped: HirExpression,
    error_side: bool,
    span: Span,
) -> (HirPattern, HirExpression, HirPattern, HirExpression) {
    let value = || HirExpression::Local {
        local: binding,
        span,
    };
    let (ok_value, err_value) = if error_side {
        (value(), mapped)
    } else {
        (mapped, value())
    };
    let ok = HirExpression::ResultOk {
        value: Box::new(ok_value),
        span,
    };
    let err = HirExpression::ResultErr {
        value: Box::new(err_value),
        span,
    };
    (
        HirPattern::Ok(Box::new(HirPattern::Binding(binding))),
        ok,
        HirPattern::Err(Box::new(HirPattern::Binding(binding))),
        err,
    )
}

fn result_flatten_arms(
    binding: LocalId,
    mapped: HirExpression,
    error_side: bool,
    span: Span,
) -> (HirPattern, HirExpression, HirPattern, HirExpression) {
    let value = || HirExpression::Local {
        local: binding,
        span,
    };
    let (ok, err) = if error_side {
        (
            HirExpression::ResultOk {
                value: Box::new(value()),
                span,
            },
            mapped,
        )
    } else {
        (
            mapped,
            HirExpression::ResultErr {
                value: Box::new(value()),
                span,
            },
        )
    };
    (
        HirPattern::Ok(Box::new(HirPattern::Binding(binding))),
        ok,
        HirPattern::Err(Box::new(HirPattern::Binding(binding))),
        err,
    )
}
