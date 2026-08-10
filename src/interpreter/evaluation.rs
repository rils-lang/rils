use super::*;

impl Interpreter {
    pub(super) fn execute_block(
        &mut self,
        block: &Block,
        parent: EnvironmentRef,
    ) -> Result<Flow, RuntimeError> {
        let flow = self.execute_statements(&block.statements, Environment::child(parent))?;
        let value = match &flow {
            Flow::Value(value) | Flow::Return(value) | Flow::Break(value) => Some(value),
            Flow::Continue => None,
        };
        if value.is_some_and(Value::contains_reference) {
            return Err(RuntimeError::new(
                "reference cannot escape its local block",
                block.span,
            ));
        }
        Ok(flow)
    }

    pub(super) fn evaluate(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
        self.tick(expression.span())?;
        match expression {
            Expr::Literal { value, .. } => Ok(match value {
                Literal::Unit => Value::Unit,
                Literal::Bool(value) => Value::Bool(*value),
                Literal::I8(value) => Value::I8(*value),
                Literal::I16(value) => Value::I16(*value),
                Literal::I32(value) => Value::I32(*value),
                Literal::I64(value) => Value::I64(*value),
                Literal::I128(value) => Value::I128(*value),
                Literal::Isize(value) => Value::Isize(*value),
                Literal::U8(value) => Value::U8(*value),
                Literal::U16(value) => Value::U16(*value),
                Literal::U32(value) => Value::U32(*value),
                Literal::U64(value) => Value::U64(*value),
                Literal::U128(value) => Value::U128(*value),
                Literal::Usize(value) => Value::Usize(*value),
                Literal::F32(value) => Value::F32(*value),
                Literal::F64(value) => Value::F64(*value),
                Literal::Char(value) => Value::Char(*value),
                Literal::Integer(value) => Value::I32(i32::try_from(*value).map_err(|_| {
                    RuntimeError::new(
                        "integer literal is outside the inferred i32 range",
                        expression.span(),
                    )
                })?),
                Literal::Float(value) => Value::F64(*value),
                Literal::String(value) => Value::String(Rc::from(value.as_str())),
            }),
            Expr::Tuple { elements, span } => {
                let mut slots = Vec::with_capacity(elements.len());
                for element in elements {
                    let value = self.evaluate(element, environment.clone())?;
                    if value.contains_reference() {
                        return Err(RuntimeError::new(
                            "tuple values cannot own local references",
                            *span,
                        ));
                    }
                    let ty = Type::of_value(&value).unwrap_or(Type::Unknown);
                    slots.push(FieldSlot {
                        value: Some(value),
                        type_annotation: ty,
                        references: 0,
                    });
                }
                Ok(Value::Tuple(Rc::new(SequenceValue {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(None),
                })))
            }
            Expr::Array {
                elements,
                repeat,
                span,
            } => {
                let mut values = Vec::new();
                if let Some(count) = repeat {
                    let value = self.evaluate(&elements[0], environment.clone())?;
                    if value.contains_reference() {
                        return Err(RuntimeError::new(
                            "arrays cannot own local references",
                            *span,
                        ));
                    }
                    if !value.is_copy() {
                        return Err(RuntimeError::new(
                            "array repeat syntax requires a Copy value",
                            *span,
                        ));
                    }
                    let count = self.evaluate(count, environment.clone())?;
                    let Value::Usize(count) = count else {
                        return Err(RuntimeError::new("array repeat count must be usize", *span));
                    };
                    for _ in 0..count {
                        values.push(
                            value
                                .clone_owned()
                                .map_err(|message| RuntimeError::new(message, *span))?,
                        );
                    }
                } else {
                    for element in elements {
                        let value = self.evaluate(element, environment.clone())?;
                        if value.contains_reference() {
                            return Err(RuntimeError::new(
                                "arrays cannot own local references",
                                *span,
                            ));
                        }
                        values.push(value);
                    }
                }
                let mut element_type = Type::Unknown;
                for value in &values {
                    let actual = Type::of_value(value).unwrap_or(Type::Unknown);
                    element_type = merge_types(&element_type, &actual).ok_or_else(|| {
                        RuntimeError::new(
                            format!("array elements must have one type, found `{element_type}` and `{actual}`"),
                            *span,
                        )
                    })?;
                }
                let slots = values
                    .into_iter()
                    .map(|value| FieldSlot {
                        value: Some(value),
                        type_annotation: element_type.clone(),
                        references: 0,
                    })
                    .collect();
                Ok(Value::Array(Rc::new(SequenceValue {
                    elements: RefCell::new(slots),
                    element_type: RefCell::new(Some(element_type)),
                })))
            }
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
            Expr::Variable { name, span } => environment.borrow().take(name).map_err(|error| {
                RuntimeError::new(
                    match error {
                        AccessError::Undefined => format!("undefined variable `{name}`"),
                        AccessError::Moved => format!("use of moved value `{name}`"),
                        AccessError::Borrowed => {
                            format!("cannot move `{name}` while it is referenced")
                        }
                        AccessError::PartiallyMoved => {
                            format!("use of partially moved value `{name}`")
                        }
                    },
                    *span,
                )
            }),
            Expr::Path { segments, span } => self.resolve_path(segments, &environment, *span),
            Expr::QualifiedPath {
                target,
                trait_name,
                member,
                span,
            } => self.resolve_qualified_path(target, trait_name, member, &environment, *span),
            Expr::Member { object, name, span } => {
                if let Expr::Variable {
                    name: variable_name,
                    ..
                } = object.as_ref()
                    && let Some(value) = environment.borrow().get(variable_name)
                {
                    if let Value::Struct(instance) = &value
                        && instance.fields.borrow().contains_key(name)
                    {
                        return self.resolve_member(value, name, *span);
                    }
                    if matches!(&value, Value::Tuple(_)) && name.parse::<usize>().is_ok() {
                        return self.resolve_member(value, name, *span);
                    }
                    if let Value::HostObject(instance) = &value
                        && instance.type_definition.methods.borrow().contains_key(name)
                    {
                        return self.resolve_member(value, name, *span);
                    }
                    let builtin_borrow = match (&value, name.as_str()) {
                        (Value::Array(_) | Value::Vec(_), "len") => Some(false),
                        (Value::Vec(_), "push" | "pop") => Some(true),
                        (Value::SequenceIterator(_), "next") => Some(true),
                        (Value::Result { .. }, "is_ok" | "is_err") => Some(false),
                        _ => None,
                    };
                    if let Some(mutable) = builtin_borrow {
                        let receiver =
                            self.reference_variable(variable_name, mutable, &environment, *span)?;
                        return self.resolve_member(receiver, name, *span);
                    }
                    let method = match &value {
                        Value::Struct(instance) => super::call::select_method(
                            &instance.type_definition.methods,
                            &instance.type_definition.trait_methods,
                            name,
                        )
                        .map_err(|traits| {
                            RuntimeError::new(
                                format!(
                                    "method `{name}` is ambiguous; candidates come from traits {}",
                                    traits.join(", ")
                                ),
                                *span,
                            )
                        })?,
                        Value::Enum(instance) => super::call::select_method(
                            &instance.type_definition.methods,
                            &instance.type_definition.trait_methods,
                            name,
                        )
                        .map_err(|traits| {
                            RuntimeError::new(
                                format!(
                                    "method `{name}` is ambiguous; candidates come from traits {}",
                                    traits.join(", ")
                                ),
                                *span,
                            )
                        })?,
                        _ => None,
                    };
                    if matches!(&value, Value::Range(_)) && name == "next" {
                        let receiver =
                            self.reference_variable(variable_name, true, &environment, *span)?;
                        return self.resolve_member(receiver, name, *span);
                    }
                    if let Some(mutable) = method.as_ref().and_then(|method| {
                        match method.parameters.first()?.type_annotation.as_ref()? {
                            Type::Reference { mutable, .. } => Some(*mutable),
                            _ => None,
                        }
                    }) {
                        let receiver =
                            self.reference_variable(variable_name, mutable, &environment, *span)?;
                        return self.resolve_member(receiver, name, *span);
                    }
                    if method.is_none()
                        && name == "clone"
                        && Type::of_value(&value)
                            .is_some_and(|ty| type_implements_trait(&ty, "Clone", &environment))
                    {
                        let receiver =
                            self.reference_variable(variable_name, false, &environment, *span)?;
                        return self.resolve_member(receiver, name, *span);
                    }
                }
                if matches!(
                    object.as_ref(),
                    Expr::Member { .. }
                        | Expr::Index { .. }
                        | Expr::Unary {
                            operator: UnaryOp::Dereference,
                            ..
                        }
                ) {
                    let place = self.resolve_place(object, &environment, *span)?;
                    let value = place.read(*span)?;
                    let method = match &value {
                        Value::Struct(instance) => super::call::select_method(
                            &instance.type_definition.methods,
                            &instance.type_definition.trait_methods,
                            name,
                        )
                        .map_err(|traits| {
                            RuntimeError::new(
                                format!(
                                    "method `{name}` is ambiguous; candidates come from traits {}",
                                    traits.join(", ")
                                ),
                                *span,
                            )
                        })?,
                        Value::Enum(instance) => super::call::select_method(
                            &instance.type_definition.methods,
                            &instance.type_definition.trait_methods,
                            name,
                        )
                        .map_err(|traits| {
                            RuntimeError::new(
                                format!(
                                    "method `{name}` is ambiguous; candidates come from traits {}",
                                    traits.join(", ")
                                ),
                                *span,
                            )
                        })?,
                        _ => None,
                    };
                    if let Some(mutable) = method.as_ref().and_then(|method| {
                        match method.parameters.first()?.type_annotation.as_ref()? {
                            Type::Reference { mutable, .. } => Some(*mutable),
                            _ => None,
                        }
                    }) {
                        let receiver = place.borrow(mutable, *span)?;
                        return self.resolve_member(receiver, name, *span);
                    }
                }
                let object = self.evaluate(object, environment)?;
                self.resolve_member(object, name, *span)
            }
            Expr::Index { span, .. } => {
                let place = self.resolve_place(expression, &environment, *span)?;
                place.read(*span)
            }
            Expr::RecordLiteral { path, fields, span } => {
                let mut values = HashMap::new();
                for (name, expression) in fields {
                    values.insert(
                        name.clone(),
                        self.evaluate(expression, environment.clone())?,
                    );
                }
                self.construct_record(path, values, *span, &environment)
            }
            Expr::Assign {
                target,
                value,
                span,
            } => {
                let value = self.evaluate(value, environment.clone())?;
                let place = self.resolve_place(target, &environment, *span)?;
                place.assign(value, *span)?;
                Ok(Value::Unit)
            }
            Expr::Borrow {
                mutable,
                target,
                span,
            } => self
                .resolve_place(target, &environment, *span)?
                .borrow(*mutable, *span),
            Expr::Unary {
                operator,
                operand,
                span,
            } => {
                let value = self.evaluate(operand, environment)?;
                if *operator == UnaryOp::Dereference {
                    let Value::Reference(reference) = value else {
                        return Err(RuntimeError::new(
                            "cannot dereference a non-reference value",
                            *span,
                        ));
                    };
                    let value = reference
                        .read()
                        .map_err(|message| RuntimeError::new(message, *span))?;
                    if !value.is_copy() {
                        return Err(RuntimeError::new(
                            "cannot move a non-Copy value out of a reference",
                            *span,
                        ));
                    }
                    return value
                        .clone_owned()
                        .map_err(|message| RuntimeError::new(message, *span));
                }
                self.unary(*operator, value, *span)
            }
            Expr::Binary {
                left,
                operator,
                right,
                span,
            } => {
                let left = self.evaluate(left, environment.clone())?;
                let right = self.evaluate(right, environment)?;
                self.binary(left, *operator, right, *span)
            }
            Expr::Logical {
                left,
                operator,
                right,
                ..
            } => {
                let left_span = left.span();
                let left = self.evaluate(left, environment.clone())?;
                match operator {
                    LogicalOp::And if !self.condition_value(&left, left_span)? => Ok(left),
                    LogicalOp::Or if self.condition_value(&left, left_span)? => Ok(left),
                    _ => self.evaluate(right, environment),
                }
            }
            Expr::Range { start, end, span } => {
                let start = self.evaluate(start, environment.clone())?;
                let end = self.evaluate(end, environment)?;
                RangeValue::new(start, end)
                    .map(Value::Range)
                    .map_err(|message| RuntimeError::new(message, *span))
            }
            Expr::Call {
                callee,
                arguments,
                span,
            } => {
                let callee = self.evaluate(callee, environment.clone())?;
                let arguments = arguments
                    .iter()
                    .map(|argument| self.evaluate(argument, environment.clone()))
                    .collect::<Result<Vec<_>, _>>()?;
                self.call(callee, &arguments, *span)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
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
        }
    }

    pub(super) fn flow_value(&mut self, flow: Flow) -> Value {
        match flow {
            Flow::Value(value) => value,
            Flow::Return(value) => {
                self.pending_return = Some(value.clone());
                value
            }
            Flow::Break(value) => {
                self.pending_loop_flow = Some(Flow::Break(value.clone()));
                value
            }
            Flow::Continue => {
                self.pending_loop_flow = Some(Flow::Continue);
                Value::Unit
            }
        }
    }

    fn reference_variable(
        &self,
        name: &str,
        mutable: bool,
        environment: &EnvironmentRef,
        span: Span,
    ) -> Result<Value, RuntimeError> {
        let slot = environment
            .borrow()
            .slot(name)
            .ok_or_else(|| RuntimeError::new(format!("undefined variable `{name}`"), span))?;
        {
            let storage = slot.borrow();
            let current = storage.read().map_err(|_| {
                RuntimeError::new(format!("cannot reference moved value `{name}`"), span)
            })?;
            if current.is_partially_moved() {
                return Err(RuntimeError::new(
                    format!("cannot reference partially moved value `{name}`"),
                    span,
                ));
            }
            if mutable && !storage.is_mutable() {
                return Err(RuntimeError::new(
                    format!("cannot mutably reference immutable variable `{name}`"),
                    span,
                ));
            }
        }
        Ok(Value::Reference(Rc::new(ReferenceValue::new_storage(
            slot, mutable,
        ))))
    }
}

pub(super) fn assignment_error(error: AssignError, subject: &str, span: Span) -> RuntimeError {
    match error {
        AssignError::Undefined => {
            RuntimeError::new(format!("undefined variable `{subject}`"), span)
        }
        AssignError::Immutable if subject == "reference" => {
            RuntimeError::new("cannot assign through immutable reference", span)
        }
        AssignError::Immutable => RuntimeError::new(
            format!("cannot assign to immutable variable `{subject}`"),
            span,
        ),
        AssignError::TypeMismatch(expected) => RuntimeError::new(
            format!("cannot assign a value incompatible with `{subject}` of type {expected}"),
            span,
        ),
        AssignError::OptionRequiresAnnotation => RuntimeError::new(
            format!(
                "cannot assign Option to untyped variable `{subject}`; declare it as `Option<T>`"
            ),
            span,
        ),
        AssignError::ReferenceEscape => {
            RuntimeError::new("reference cannot escape its local scope", span)
        }
        AssignError::BorrowedTarget => RuntimeError::new(
            format!("cannot replace `{subject}` while one of its fields is referenced"),
            span,
        ),
    }
}
