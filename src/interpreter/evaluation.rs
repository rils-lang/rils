use super::*;

mod control;
mod member;
mod values;

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
            Expr::Call { .. }
            | Expr::If { .. }
            | Expr::Match { .. }
            | Expr::Block(_)
            | Expr::Try { .. } => self.evaluate_control(expression, environment),
            _ => self.evaluate_non_control(expression, environment),
        }
    }

    fn evaluate_non_control(
        &mut self,
        expression: &Expr,
        environment: EnvironmentRef,
    ) -> Result<Value, RuntimeError> {
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
            Expr::Tuple { .. } | Expr::Array { .. } => {
                self.evaluate_sequence(expression, environment)
            }
            Expr::Try { .. } => self.evaluate_control(expression, environment),
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
                self.evaluate_member(object, name, *span, environment)
            }
            Expr::Index { span, .. } => {
                let place = self.resolve_place(expression, &environment, *span)?;
                place.read(*span)
            }
            Expr::RecordLiteral { path, fields, span } => {
                let mut values = HashMap::new();
                for field in fields {
                    values.insert(
                        field.name.clone(),
                        self.evaluate(&field.value, environment.clone())?,
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
            Expr::Cast {
                operand,
                target,
                span,
            } => {
                let value = self.evaluate(operand, environment)?;
                let Type::Integer(target) = target else {
                    return Err(RuntimeError::new(
                        "`as` currently supports concrete integer target types only",
                        *span,
                    ));
                };
                crate::numeric::cast_integer(value, *target)
                    .map_err(|message| RuntimeError::new(message, *span))
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
            Expr::Call { .. } | Expr::If { .. } | Expr::Match { .. } | Expr::Block(_) => {
                self.evaluate_control(expression, environment)
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
