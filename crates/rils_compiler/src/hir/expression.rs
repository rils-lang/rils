use super::*;

impl<'a> FunctionLowerer<'a> {
    pub(super) fn expression(&mut self, expression: &Expr) -> Result<HirExpression, CompileError> {
        let expression_id = self.expression_id(expression)?;
        match expression {
            Expr::Literal { value, span } => Ok(HirExpression::Literal {
                value: lower_expression_literal(
                    value,
                    self.typeck_results
                        .expression_type(expression_id)
                        .unwrap_or(&Type::Unknown),
                    *span,
                )?,
                span: *span,
            }),
            Expr::Variable { name, span } if name == "None" => {
                Ok(HirExpression::OptionNone { span: *span })
            }
            Expr::Variable { name, span } => {
                if let Some(local) = self.lookup(name) {
                    Ok(HirExpression::Local { local, span: *span })
                } else if let Some(callable) = self.resolved_value(expression_id) {
                    Ok(HirExpression::Function {
                        function: callable.function,
                        span: *span,
                    })
                } else {
                    Err(CompileError::unsupported(
                        format!("bytecode backend cannot resolve non-local value `{name}`"),
                        *span,
                    ))
                }
            }
            Expr::Path { segments, span } => {
                let segments = self.resolve_self_path(segments);
                if let [type_name, member] = segments.as_slice()
                    && let Some(target) = crate::types::IntegerType::from_name(type_name)
                    && let Some(constant) = rils_builtins::integer_constant(member)
                {
                    return Ok(HirExpression::Literal {
                        value: integer_constant_literal(target, constant.id),
                        span: *span,
                    });
                }
                if let [type_name, member] = segments.as_slice()
                    && let Some(target) = crate::types::FloatType::from_name(type_name)
                    && let Some(constant) = rils_builtins::float_constant(member)
                {
                    return Ok(HirExpression::Literal {
                        value: float_constant_literal(target, constant.id),
                        span: *span,
                    });
                }
                if let Some(callable) = self.resolved_value(expression_id) {
                    return Ok(HirExpression::Function {
                        function: callable.function,
                        span: *span,
                    });
                }
                let (type_id, variant) = self.enum_variant_path(&segments, *span)?;
                Ok(HirExpression::ConstructUnitVariant {
                    type_id,
                    variant,
                    span: *span,
                })
            }
            Expr::QualifiedPath { span, .. } => {
                let method = self.resolved_value(expression_id).ok_or_else(|| {
                    CompileError::unsupported(
                        "semantic analysis did not resolve UFCS function value",
                        *span,
                    )
                })?;
                Ok(HirExpression::Function {
                    function: method.function,
                    span: *span,
                })
            }
            Expr::Member { object, name, span } if self.resolved_value(expression_id).is_some() => {
                let method = self
                    .resolved_value(expression_id)
                    .expect("guarded method value");
                let receiver = method.receiver.ok_or_else(|| {
                    CompileError::unsupported(
                        format!("associated function `{name}` cannot be bound to a receiver"),
                        *span,
                    )
                })?;
                Ok(HirExpression::BindMethod {
                    function: method.function,
                    receiver: Box::new(self.method_receiver(object, receiver)?),
                    span: *span,
                })
            }
            Expr::Index { span, .. } | Expr::Member { span, .. } => Ok(HirExpression::Place {
                place: self.place(expression)?,
                span: *span,
            }),
            Expr::Assign {
                target,
                value,
                span,
            } => match target.as_ref() {
                Expr::Variable { name, .. } => {
                    let local = self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), target.span())
                    })?;
                    if !self.mutable[local] {
                        return Err(CompileError::unsupported(
                            format!("cannot assign to immutable local `{name}`"),
                            *span,
                        ));
                    }
                    Ok(HirExpression::Assign {
                        local,
                        value: Box::new(self.expression(value)?),
                        span: *span,
                    })
                }
                Expr::Index { .. } | Expr::Member { .. } => Ok(HirExpression::AssignPlace {
                    place: self.place(target)?,
                    value: Box::new(self.expression(value)?),
                    span: *span,
                }),
                Expr::Unary {
                    operator: UnaryOp::Dereference,
                    operand,
                    ..
                } => Ok(HirExpression::AssignDereference {
                    reference: Box::new(self.expression(operand)?),
                    value: Box::new(self.expression(value)?),
                    span: *span,
                }),
                _ => Err(CompileError::unsupported(
                    "assignment place is not supported by the bytecode backend yet",
                    *span,
                )),
            },
            Expr::Unary {
                operator,
                operand,
                span,
            } => Ok(HirExpression::Unary {
                operator: *operator,
                operand: Box::new(self.expression(operand)?),
                span: *span,
            }),
            Expr::Cast {
                operand,
                target,
                span,
            } => {
                let crate::types::Type::Integer(target) = target else {
                    return Err(CompileError::unsupported(
                        "`as` currently supports concrete integer target types only",
                        *span,
                    ));
                };
                Ok(HirExpression::Cast {
                    operand: Box::new(self.expression(operand)?),
                    target: *target,
                    span: *span,
                })
            }
            Expr::Borrow {
                mutable,
                target,
                span,
            } => match target.as_ref() {
                Expr::Variable { name, .. } => {
                    let local = self.lookup(name).ok_or_else(|| {
                        CompileError::unsupported(format!("unknown local `{name}`"), target.span())
                    })?;
                    Ok(HirExpression::BorrowLocal {
                        local,
                        mutable: *mutable,
                        span: *span,
                    })
                }
                Expr::Index { .. } | Expr::Member { .. } => Ok(HirExpression::BorrowPlace {
                    place: self.place(target)?,
                    mutable: *mutable,
                    span: *span,
                }),
                Expr::Unary {
                    operator: UnaryOp::Dereference,
                    operand,
                    ..
                } => Ok(HirExpression::Reborrow {
                    reference: Box::new(self.expression(operand)?),
                    mutable: *mutable,
                    span: *span,
                }),
                _ => Err(CompileError::unsupported(
                    "borrow place is not supported by the bytecode backend yet",
                    *span,
                )),
            },
            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => Ok(HirExpression::Binary {
                left: Box::new(self.expression(left)?),
                operator: *operator,
                right: Box::new(self.expression(right)?),
                integer: self.expression_type(left).and_then(|value| match value {
                    Type::Integer(integer) => Some(integer),
                    _ => None,
                }),
                span: *span,
            }),
            Expr::Logical {
                left,
                operator,
                right,
                span,
            } => Ok(HirExpression::Logical {
                left: Box::new(self.expression(left)?),
                operator: *operator,
                right: Box::new(self.expression(right)?),
                span: *span,
            }),
            Expr::Call {
                callee,
                arguments,
                span,
            } => {
                if let Some((name, signature, capability)) = self.resolved_import(expression_id) {
                    return Ok(HirExpression::CallImport {
                        name: name.to_owned(),
                        signature: signature.clone(),
                        capability: capability.to_owned(),
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::QualifiedPath {
                    target,
                    trait_name,
                    member,
                    ..
                } = callee.as_ref()
                {
                    if trait_name == "Default" && member == "default" {
                        if !arguments.is_empty() {
                            return Err(CompileError::unsupported(
                                "Default::default takes no arguments",
                                *span,
                            ));
                        }
                        if let Some(value) = builtin_default_hir(target, *span)? {
                            return Ok(value);
                        }
                    }
                    if let Some(callable) = self.resolved_definition(expression_id) {
                        return Ok(HirExpression::Call {
                            function: callable.function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    return Err(CompileError::unsupported(
                        format!(
                            "semantic analysis did not resolve UFCS call `<{target} as {trait_name}>::{member}`"
                        ),
                        *span,
                    ));
                }
                if let Expr::Path { segments, .. } = callee.as_ref() {
                    let segments = self.resolve_self_path(segments);
                    if let Some(callable) = self.resolved_definition(expression_id) {
                        return Ok(HirExpression::Call {
                            function: callable.function,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let [type_name, _] = segments.as_slice()
                        && let Some(target) = crate::types::IntegerType::from_name(type_name)
                        && let Some(intrinsic) =
                            self.resolved_builtin(expression_id)
                                .and_then(|(id, kind, receiver)| {
                                    (kind == rils_frontend::semantic::BuiltinCallKind::Intrinsic
                                        && receiver.is_none())
                                    .then_some(id)
                                })
                    {
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic,
                            target: Some(target),
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    if let Some(host_name) = self.resolved_host(expression_id)
                        && let Some(function) = self.host_function(host_name, arguments, *span)?
                    {
                        return Ok(HirExpression::CallImport {
                            name: function.name,
                            signature: function.signature,
                            capability: function.capability,
                            arguments: arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<_, _>>()?,
                            span: *span,
                        });
                    }
                    let (type_id, variant) = self.enum_variant_path(&segments, *span)?;
                    return Ok(HirExpression::ConstructTupleVariant {
                        type_id,
                        variant,
                        fields: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Member { object, name, .. } = callee.as_ref() {
                    if let Some(method) = self.resolved_definition(expression_id) {
                        let mut lowered = Vec::with_capacity(
                            arguments.len() + usize::from(method.receiver.is_some()),
                        );
                        if let Some(receiver) = method.receiver {
                            lowered.push(self.method_receiver(object, receiver)?);
                        }
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::Call {
                            function: method.function,
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    let semantic_builtin = self
                        .typeck_results
                        .resolved_call(expression_id)
                        .and_then(|call| match call {
                            rils_frontend::semantic::ResolvedCall::Builtin {
                                id,
                                kind,
                                receiver,
                            } => Some((*id, *kind, *receiver)),
                            _ => None,
                        });
                    let intrinsic = semantic_builtin
                        .filter(|(_, kind, _)| {
                            *kind == rils_frontend::semantic::BuiltinCallKind::Intrinsic
                        })
                        .map(|(id, _, _)| id);
                    if let Some(intrinsic) = intrinsic {
                        let mut lowered = Vec::with_capacity(arguments.len() + 1);
                        lowered.push(self.expression(object)?);
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::CallIntrinsic {
                            intrinsic,
                            target: None,
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    if let Some((builtin, _, receiver)) = semantic_builtin.filter(|(_, kind, _)| {
                        *kind == rils_frontend::semantic::BuiltinCallKind::Runtime
                    }) {
                        if name == "into_iter"
                            && arguments.is_empty()
                            && matches!(
                                builtin,
                                rils_builtins::BuiltinId::SequenceIntoIter
                                    | rils_builtins::BuiltinId::RangeIntoIter
                                    | rils_builtins::BuiltinId::IteratorIntoIter
                            )
                        {
                            return Ok(HirExpression::IntoIterator {
                                value: Box::new(self.expression(object)?),
                                span: *span,
                            });
                        }
                        if let Some(expression) =
                            self.builtin_combinator(Some(builtin), name, object, arguments, *span)?
                        {
                            return Ok(expression);
                        }
                        if builtin.has_direct_runtime_call()
                            && let Some(receiver) = receiver.map(|receiver| match receiver {
                                rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
                                rils_builtins::ReceiverMode::Shared => {
                                    ReceiverMode::Reference { mutable: false }
                                }
                                rils_builtins::ReceiverMode::Mutable => {
                                    ReceiverMode::Reference { mutable: true }
                                }
                            })
                        {
                            let receiver = self.method_receiver(object, receiver)?;
                            let mut lowered = Vec::with_capacity(arguments.len() + 1);
                            lowered.push(receiver);
                            lowered.extend(
                                arguments
                                    .iter()
                                    .map(|argument| self.expression(argument))
                                    .collect::<Result<Vec<_>, _>>()?,
                            );
                            return Ok(HirExpression::CallRuntime {
                                builtin,
                                arguments: lowered,
                                span: *span,
                            });
                        }
                    }
                    if self.resolved_host(expression_id).is_some()
                        && let Some(host) = self.host_method(object, name, arguments, *span)?
                    {
                        let receiver = match host.receiver {
                            Some(HostReceiver::Value) => ReceiverMode::Owned,
                            Some(HostReceiver::Ref) => ReceiverMode::Reference { mutable: false },
                            Some(HostReceiver::RefMut) => ReceiverMode::Reference { mutable: true },
                            None => unreachable!("host_method only returns receiver methods"),
                        };
                        let mut lowered = Vec::with_capacity(arguments.len() + 1);
                        // Host ABI methods always receive the opaque handle by value.  The
                        // receiver mode still controls borrowing/ownership at the source
                        // level, but a `&self`/`&mut self` receiver must be dereferenced
                        // before crossing the import boundary (otherwise the VM passes a
                        // reference value and the host reports `expected HostHandle`).
                        let receiver_value = self.method_receiver(object, receiver)?;
                        lowered.push(match receiver {
                            ReceiverMode::Owned => receiver_value,
                            ReceiverMode::Reference { .. } => HirExpression::Unary {
                                operator: UnaryOp::Dereference,
                                operand: Box::new(receiver_value),
                                span: *span,
                            },
                        });
                        lowered.extend(
                            arguments
                                .iter()
                                .map(|argument| self.expression(argument))
                                .collect::<Result<Vec<_>, _>>()?,
                        );
                        return Ok(HirExpression::CallImport {
                            name: host.name.clone(),
                            signature: host.signature.clone(),
                            capability: host.capability.clone(),
                            arguments: lowered,
                            span: *span,
                        });
                    }
                    return Err(CompileError::unsupported(
                        format!(
                            "semantic analysis did not resolve method call `{name}` on `{}`",
                            self.expression_type(object).unwrap_or(Type::Unknown)
                        ),
                        *span,
                    ));
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && matches!(name.as_str(), "Some" | "Ok" | "Err")
                {
                    let [argument] = arguments.as_slice() else {
                        return Err(CompileError::unsupported(
                            format!("`{name}` expects exactly one argument"),
                            *span,
                        ));
                    };
                    let value = Box::new(self.expression(argument)?);
                    return Ok(match name.as_str() {
                        "Some" => HirExpression::OptionSome { value, span: *span },
                        "Ok" => HirExpression::ResultOk { value, span: *span },
                        "Err" => HirExpression::ResultErr { value, span: *span },
                        _ => unreachable!(),
                    });
                }
                if matches!(callee.as_ref(), Expr::Variable { .. })
                    && let Some(callable) = self.resolved_definition(expression_id)
                {
                    return Ok(HirExpression::Call {
                        function: callable.function,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                if let Expr::Variable { name, .. } = callee.as_ref()
                    && self.lookup(name).is_none()
                    && let Some(host_name) = self.resolved_host(expression_id)
                    && let Some(function) = self.host_function(host_name, arguments, *span)?
                {
                    return Ok(HirExpression::CallImport {
                        name: function.name,
                        signature: function.signature,
                        capability: function.capability,
                        arguments: arguments
                            .iter()
                            .map(|argument| self.expression(argument))
                            .collect::<Result<_, _>>()?,
                        span: *span,
                    });
                }
                Ok(HirExpression::CallValue {
                    callee: Box::new(self.expression(callee)?),
                    arguments: arguments
                        .iter()
                        .map(|argument| self.expression(argument))
                        .collect::<Result<_, _>>()?,
                    span: *span,
                })
            }
            Expr::RecordLiteral { path, fields, span } => {
                let path = self.resolve_self_path(path);
                let (type_id, variant) = if path.len() >= 2 {
                    let enum_name = path[..path.len() - 1].join("::");
                    if let Some(type_id) = self.types.get(&enum_name) {
                        (*type_id, Some(path.last().unwrap().clone()))
                    } else {
                        (self.type_id(&path.join("::"), *span)?, None)
                    }
                } else {
                    (self.type_id(path.last().unwrap(), *span)?, None)
                };
                Ok(HirExpression::ConstructRecord {
                    type_id,
                    variant,
                    fields: fields
                        .iter()
                        .map(|field| Ok((field.name.clone(), self.expression(&field.value)?)))
                        .collect::<Result<_, CompileError>>()?,
                    span: *span,
                })
            }
            Expr::Try { operand, span } if self.in_function => Ok(HirExpression::Try {
                operand: Box::new(self.expression(operand)?),
                span: *span,
            }),
            Expr::Match { value, arms, span } => {
                let value = Box::new(self.expression(value)?);
                let mut lowered_arms = Vec::with_capacity(arms.len());
                for arm in arms {
                    self.scopes.push(HashMap::new());
                    let pattern = self.pattern(&arm.pattern)?;
                    let expression = self.expression(&arm.expression)?;
                    self.scopes.pop();
                    lowered_arms.push(HirMatchArm {
                        pattern,
                        expression,
                        span: arm.pattern.span(),
                    });
                }
                Ok(HirExpression::Match {
                    value,
                    arms: lowered_arms,
                    span: *span,
                })
            }
            Expr::Tuple { elements, span } => Ok(HirExpression::Tuple {
                elements: elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<_, _>>()?,
                span: *span,
            }),
            Expr::Array {
                elements,
                repeat,
                span,
            } => Ok(HirExpression::Array {
                elements: elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Result<_, _>>()?,
                repeat: repeat
                    .as_ref()
                    .map(|value| self.expression(value).map(Box::new))
                    .transpose()?,
                span: *span,
            }),
            Expr::Range { start, end, span } => Ok(HirExpression::Range {
                start: Box::new(self.expression(start)?),
                end: Box::new(self.expression(end)?),
                span: *span,
            }),
            Expr::If {
                condition,
                then_branch,
                else_branch,
                span,
            } => Ok(HirExpression::If {
                condition: Box::new(self.expression(condition)?),
                then_branch: self.block_statements(then_branch)?,
                else_branch: else_branch
                    .as_ref()
                    .map(|branch| self.expression(branch).map(Box::new))
                    .transpose()?,
                span: *span,
            }),
            Expr::Block(block) => Ok(HirExpression::Block {
                statements: self.block_statements(block)?,
                span: block.span,
            }),
            _ => Err(CompileError::unsupported(
                "expression is not supported by the bytecode backend yet",
                expression.span(),
            )),
        }
    }
}
