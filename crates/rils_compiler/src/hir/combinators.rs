use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn builtin_combinator(
        &mut self,
        owner: Option<&str>,
        name: &str,
        object: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<Option<HirExpression>, CompileError> {
        let family = match (owner, name) {
            (Some("Option"), "map" | "and_then" | "or_else") => "option",
            (Some("Result"), "map" | "map_err" | "and_then" | "or_else") => "result",
            _ => return Ok(None),
        };
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
        let (first_pattern, first_expression, second_pattern, second_expression) =
            match (family, name) {
                ("option", "map") => (
                    HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                    HirExpression::OptionSome {
                        value: Box::new(call(vec![binding()])),
                        span,
                    },
                    HirPattern::None,
                    HirExpression::OptionNone { span },
                ),
                ("option", "and_then") => (
                    HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                    call(vec![binding()]),
                    HirPattern::None,
                    HirExpression::OptionNone { span },
                ),
                ("option", "or_else") => (
                    HirPattern::Some(Box::new(HirPattern::Binding(binding_local))),
                    HirExpression::OptionSome {
                        value: Box::new(binding()),
                        span,
                    },
                    HirPattern::None,
                    call(Vec::new()),
                ),
                ("result", "map") => result_arms(binding_local, call(vec![binding()]), false, span),
                ("result", "map_err") => {
                    result_arms(binding_local, call(vec![binding()]), true, span)
                }
                ("result", "and_then") => {
                    result_flatten_arms(binding_local, call(vec![binding()]), false, span)
                }
                ("result", "or_else") => {
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

    fn allocate_combinator_local(&mut self) -> LocalId {
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
