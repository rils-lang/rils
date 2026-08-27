use super::*;

impl FunctionLowerer<'_> {
    fn allocate_combinator_local_with_mutability(&mut self, mutable: bool) -> LocalId {
        let local = self.mutable.len();
        self.mutable.push(mutable);
        local
    }

    pub(super) fn iterator_default(
        &mut self,
        name: &str,
        object: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<HirExpression, CompileError> {
        if matches!(
            name,
            "count" | "last" | "collect_vec" | "take" | "skip" | "rev"
        ) {
            return self.iterator_basic_default(name, object, arguments, span);
        }
        let callback_index = usize::from(name == "fold");
        let expected_arguments = if name == "enumerate" {
            0
        } else if name == "fold" {
            2
        } else {
            1
        };
        if arguments.len() != expected_arguments {
            return Err(CompileError::unsupported(
                format!("`{name}` expects {expected_arguments} arguments"),
                span,
            ));
        }

        let mut callback = arguments
            .get(callback_index)
            .map(|callback| self.expression(callback))
            .transpose()?;
        let callback_local = callback.as_ref().map(|_| self.allocate_combinator_local());
        let binding = self.allocate_combinator_local();
        let iterable = self.expression(object)?;
        let local = |local| HirExpression::Local { local, span };
        let callback_call = |arguments| HirExpression::CallValue {
            callee: Box::new(local(callback_local.expect("callback method"))),
            arguments,
            span,
        };
        let callback_statement = callback_local.map(|local| HirStatement::Let {
            local,
            initializer: callback.take().expect("callback initializer"),
            span,
        });

        let expression = match name {
            "map" | "filter" | "filter_map" | "enumerate" => {
                let output = self.allocate_combinator_local_with_mutability(true);
                let mapped = (name == "filter_map").then(|| self.allocate_combinator_local());
                let index = (name == "enumerate")
                    .then(|| self.allocate_combinator_local_with_mutability(true));
                let push = |value| vec_push(output, value, span);
                let body = match name {
                    "map" => vec![expression_statement(
                        push(callback_call(vec![local(binding)])),
                        span,
                    )],
                    "filter" => vec![expression_statement(
                        HirExpression::If {
                            condition: Box::new(callback_call(vec![HirExpression::BorrowLocal {
                                local: binding,
                                mutable: false,
                                span,
                            }])),
                            then_branch: vec![expression_statement(push(local(binding)), span)],
                            else_branch: None,
                            span,
                        },
                        span,
                    )],
                    "filter_map" => vec![expression_statement(
                        HirExpression::Match {
                            value: Box::new(callback_call(vec![local(binding)])),
                            arms: vec![
                                HirMatchArm {
                                    pattern: HirPattern::Some(Box::new(HirPattern::Binding(
                                        mapped.expect("filter_map binding"),
                                    ))),
                                    expression: push(local(mapped.expect("filter_map binding"))),
                                    span,
                                },
                                HirMatchArm {
                                    pattern: HirPattern::None,
                                    expression: unit(span),
                                    span,
                                },
                            ],
                            span,
                        },
                        span,
                    )],
                    "enumerate" => {
                        let index = index.expect("enumerate index");
                        vec![
                            expression_statement(
                                push(HirExpression::Tuple {
                                    elements: vec![local(index), local(binding)],
                                    span,
                                }),
                                span,
                            ),
                            expression_statement(
                                HirExpression::Assign {
                                    local: index,
                                    value: Box::new(HirExpression::Binary {
                                        left: Box::new(local(index)),
                                        operator: crate::ast::BinaryOp::Add,
                                        right: Box::new(usize_literal(1, span)),
                                        integer: None,
                                        span,
                                    }),
                                    span,
                                },
                                span,
                            ),
                        ]
                    }
                    _ => unreachable!(),
                };
                let mut statements = Vec::new();
                if let Some(statement) = callback_statement {
                    statements.push(statement);
                }
                statements.push(HirStatement::Let {
                    local: output,
                    initializer: vec_new(span),
                    span,
                });
                if let Some(index) = index {
                    statements.push(HirStatement::Let {
                        local: index,
                        initializer: usize_literal(0, span),
                        span,
                    });
                }
                statements.push(HirStatement::For {
                    binding,
                    iterable,
                    body,
                    span,
                });
                statements.push(HirStatement::Expression {
                    expression: HirExpression::IntoIterator {
                        value: Box::new(local(output)),
                        span,
                    },
                    terminated: false,
                    span,
                });
                HirExpression::Block { statements, span }
            }
            "fold" => {
                let accumulator = self.allocate_combinator_local_with_mutability(true);
                HirExpression::Block {
                    statements: vec![
                        callback_statement.expect("fold callback"),
                        HirStatement::Let {
                            local: accumulator,
                            initializer: self.expression(&arguments[0])?,
                            span,
                        },
                        HirStatement::For {
                            binding,
                            iterable,
                            body: vec![expression_statement(
                                HirExpression::Assign {
                                    local: accumulator,
                                    value: Box::new(callback_call(vec![
                                        local(accumulator),
                                        local(binding),
                                    ])),
                                    span,
                                },
                                span,
                            )],
                            span,
                        },
                        HirStatement::Expression {
                            expression: local(accumulator),
                            terminated: false,
                            span,
                        },
                    ],
                    span,
                }
            }
            "for_each" => HirExpression::Block {
                statements: vec![
                    callback_statement.expect("for_each callback"),
                    HirStatement::For {
                        binding,
                        iterable,
                        body: vec![expression_statement(
                            callback_call(vec![local(binding)]),
                            span,
                        )],
                        span,
                    },
                    HirStatement::Expression {
                        expression: unit(span),
                        terminated: false,
                        span,
                    },
                ],
                span,
            },
            "any" | "all" | "find" | "position" => {
                let result = self.allocate_combinator_local_with_mutability(true);
                let option_result = matches!(name, "find" | "position");
                let result_iterator =
                    option_result.then(|| self.allocate_combinator_local_with_mutability(true));
                let index = (name == "position")
                    .then(|| self.allocate_combinator_local_with_mutability(true));
                let initial = match name {
                    "any" => bool_literal(false, span),
                    "all" => bool_literal(true, span),
                    "find" | "position" => vec_new(span),
                    _ => unreachable!(),
                };
                let callback_argument = if name == "find" {
                    HirExpression::BorrowLocal {
                        local: binding,
                        mutable: false,
                        span,
                    }
                } else {
                    local(binding)
                };
                let condition = callback_call(vec![callback_argument]);
                let condition = if name == "all" {
                    HirExpression::Unary {
                        operator: crate::ast::UnaryOp::Not,
                        operand: Box::new(condition),
                        span,
                    }
                } else {
                    condition
                };
                let matched = match name {
                    "any" => bool_literal(true, span),
                    "all" => bool_literal(false, span),
                    "find" => local(binding),
                    "position" => local(index.expect("position index")),
                    _ => unreachable!(),
                };
                let store_match = if option_result {
                    vec_push(result, matched, span)
                } else {
                    HirExpression::Assign {
                        local: result,
                        value: Box::new(matched),
                        span,
                    }
                };
                let mut body = vec![expression_statement(
                    HirExpression::If {
                        condition: Box::new(condition),
                        then_branch: vec![
                            expression_statement(store_match, span),
                            HirStatement::Break { value: None, span },
                        ],
                        else_branch: None,
                        span,
                    },
                    span,
                )];
                if let Some(index) = index {
                    body.push(expression_statement(
                        HirExpression::Assign {
                            local: index,
                            value: Box::new(HirExpression::Binary {
                                left: Box::new(local(index)),
                                operator: crate::ast::BinaryOp::Add,
                                right: Box::new(usize_literal(1, span)),
                                integer: None,
                                span,
                            }),
                            span,
                        },
                        span,
                    ));
                }
                let mut statements = vec![
                    callback_statement.expect("predicate callback"),
                    HirStatement::Let {
                        local: result,
                        initializer: initial,
                        span,
                    },
                ];
                if let Some(index) = index {
                    statements.push(HirStatement::Let {
                        local: index,
                        initializer: usize_literal(0, span),
                        span,
                    });
                }
                statements.push(HirStatement::For {
                    binding,
                    iterable,
                    body,
                    span,
                });
                if let Some(result_iterator) = result_iterator {
                    statements.push(HirStatement::Let {
                        local: result_iterator,
                        initializer: HirExpression::IntoIterator {
                            value: Box::new(local(result)),
                            span,
                        },
                        span,
                    });
                    statements.push(HirStatement::Expression {
                        expression: iterator_next(result_iterator, span),
                        terminated: false,
                        span,
                    });
                } else {
                    statements.push(HirStatement::Expression {
                        expression: local(result),
                        terminated: false,
                        span,
                    });
                }
                HirExpression::Block { statements, span }
            }
            _ => unreachable!(),
        };
        Ok(expression)
    }

    fn iterator_basic_default(
        &mut self,
        name: &str,
        object: &Expr,
        arguments: &[Expr],
        span: Span,
    ) -> Result<HirExpression, CompileError> {
        let expected = usize::from(matches!(name, "take" | "skip"));
        if arguments.len() != expected {
            return Err(CompileError::unsupported(
                format!("`{name}` expects {expected} arguments"),
                span,
            ));
        }
        let binding = self.allocate_combinator_local();
        let iterable = self.expression(object)?;
        let local = |local| HirExpression::Local { local, span };

        if name == "count" {
            let count = self.allocate_combinator_local_with_mutability(true);
            return Ok(HirExpression::Block {
                statements: vec![
                    HirStatement::Let {
                        local: count,
                        initializer: usize_literal(0, span),
                        span,
                    },
                    HirStatement::For {
                        binding,
                        iterable,
                        body: vec![increment(count, span)],
                        span,
                    },
                    HirStatement::Expression {
                        expression: local(count),
                        terminated: false,
                        span,
                    },
                ],
                span,
            });
        }

        let output = self.allocate_combinator_local_with_mutability(true);
        let index = matches!(name, "take" | "skip")
            .then(|| self.allocate_combinator_local_with_mutability(true));
        let limit = arguments
            .first()
            .map(|argument| {
                let local = self.allocate_combinator_local();
                self.expression(argument)
                    .map(|initializer| (local, initializer))
            })
            .transpose()?;
        let body = match name {
            "take" | "skip" => {
                let index = index.expect("indexed iterator method");
                let limit_local = limit.as_ref().expect("iterator limit").0;
                let operator = match name {
                    "take" => crate::ast::BinaryOp::Less,
                    "skip" => crate::ast::BinaryOp::GreaterEqual,
                    _ => unreachable!(),
                };
                let matched = vec![expression_statement(
                    vec_push(output, local(binding), span),
                    span,
                )];
                vec![
                    expression_statement(
                        HirExpression::If {
                            condition: Box::new(HirExpression::Binary {
                                left: Box::new(local(index)),
                                operator,
                                right: Box::new(local(limit_local)),
                                integer: None,
                                span,
                            }),
                            then_branch: matched,
                            else_branch: None,
                            span,
                        },
                        span,
                    ),
                    increment(index, span),
                ]
            }
            "last" | "collect_vec" | "rev" => vec![expression_statement(
                vec_push(output, local(binding), span),
                span,
            )],
            _ => unreachable!(),
        };

        let mut statements = vec![HirStatement::Let {
            local: output,
            initializer: vec_new(span),
            span,
        }];
        if let Some((limit, initializer)) = limit {
            statements.push(HirStatement::Let {
                local: limit,
                initializer,
                span,
            });
        }
        if let Some(index) = index {
            statements.push(HirStatement::Let {
                local: index,
                initializer: usize_literal(0, span),
                span,
            });
        }
        statements.push(HirStatement::For {
            binding,
            iterable,
            body,
            span,
        });
        if name == "collect_vec" {
            statements.push(HirStatement::Expression {
                expression: local(output),
                terminated: false,
                span,
            });
        } else {
            let iterator = self.allocate_combinator_local_with_mutability(name == "last");
            let iterator_value = HirExpression::IntoIterator {
                value: Box::new(local(output)),
                span,
            };
            let iterator_value = if name == "rev" || name == "last" {
                iterator_rev(iterator_value, span)
            } else {
                iterator_value
            };
            statements.push(HirStatement::Let {
                local: iterator,
                initializer: iterator_value,
                span,
            });
            statements.push(HirStatement::Expression {
                expression: if name == "last" {
                    iterator_next(iterator, span)
                } else {
                    local(iterator)
                },
                terminated: false,
                span,
            });
        }
        Ok(HirExpression::Block { statements, span })
    }
}

fn expression_statement(expression: HirExpression, span: Span) -> HirStatement {
    HirStatement::Expression {
        expression,
        terminated: true,
        span,
    }
}

fn unit(span: Span) -> HirExpression {
    HirExpression::Literal {
        value: HirLiteral::Unit,
        span,
    }
}

fn bool_literal(value: bool, span: Span) -> HirExpression {
    HirExpression::Literal {
        value: HirLiteral::Bool(value),
        span,
    }
}

fn usize_literal(value: usize, span: Span) -> HirExpression {
    HirExpression::Literal {
        value: HirLiteral::Usize(value),
        span,
    }
}

fn increment(local: LocalId, span: Span) -> HirStatement {
    expression_statement(
        HirExpression::Assign {
            local,
            value: Box::new(HirExpression::Binary {
                left: Box::new(HirExpression::Local { local, span }),
                operator: crate::ast::BinaryOp::Add,
                right: Box::new(usize_literal(1, span)),
                integer: None,
                span,
            }),
            span,
        },
        span,
    )
}

fn vec_new(span: Span) -> HirExpression {
    HirExpression::CallImport {
        name: "core::vec::new".into(),
        signature: crate::types::FunctionSignature::fixed(
            Vec::new(),
            Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::Unknown],
            },
        ),
        capability: "core".into(),
        arguments: Vec::new(),
        span,
    }
}

fn vec_push(output: LocalId, value: HirExpression, span: Span) -> HirExpression {
    HirExpression::CallRuntime {
        builtin: rils_builtins::BuiltinId::VecPush,
        arguments: vec![
            HirExpression::BorrowLocal {
                local: output,
                mutable: true,
                span,
            },
            value,
        ],
        span,
    }
}

fn iterator_next(iterator: LocalId, span: Span) -> HirExpression {
    HirExpression::CallRuntime {
        builtin: rils_builtins::BuiltinId::IteratorNext,
        arguments: vec![HirExpression::BorrowLocal {
            local: iterator,
            mutable: true,
            span,
        }],
        span,
    }
}

fn iterator_rev(iterator: HirExpression, span: Span) -> HirExpression {
    HirExpression::CallRuntime {
        builtin: rils_builtins::BuiltinId::IteratorRev,
        arguments: vec![iterator],
        span,
    }
}
