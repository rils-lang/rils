use super::*;

impl Checker<'_> {
    pub(super) fn expression(&mut self, expression: &Expr) -> ExpressionValue {
        match expression {
            Expr::Literal { .. }
            | Expr::Path { .. }
            | Expr::GenericPath { .. }
            | Expr::QualifiedPath { .. } => self.typed_value(expression),
            Expr::Variable { name, .. } => self.take_variable(name, expression),
            Expr::Member { object, name, span }
                if matches!(
                    self.expression_types.get(expression),
                    Some(Type::Function { .. })
                ) =>
            {
                match self.receiver_mode(self.expression_types.get(object), name) {
                    Some(ReceiverMode::Owned) => {
                        if self
                            .expression_types
                            .get(object)
                            .is_some_and(|ty| !self.is_copy(ty))
                        {
                            self.diagnostic(
                                "bound method values with an owned receiver require Copy",
                                *span,
                            );
                        }
                        self.expression(object)
                    }
                    Some(ReceiverMode::Borrowed { .. }) => {
                        self.diagnostic(
                            "bound method values cannot capture a local reference",
                            *span,
                        );
                        ExpressionValue::default()
                    }
                    None => self.typed_value(expression),
                }
            }
            Expr::Member { .. } => {
                self.read_place(expression);
                if let Some(ty) = self.expression_types.get(expression)
                    && !self.is_copy(ty)
                {
                    self.move_place(expression);
                }
                self.typed_value(expression)
            }
            Expr::Index { object, index, .. } => {
                self.read_place(object);
                let index = self.expression(index);
                self.discard(index);
                if let Some(ty) = self.expression_types.get(expression)
                    && !self.is_copy(ty)
                {
                    self.diagnostic(
                        "cannot move a non-Copy value out through indexing",
                        expression.span(),
                    );
                }
                self.typed_value(expression)
            }
            Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
                let mut values = elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Vec<_>>();
                if let Expr::Array {
                    repeat: Some(repeat),
                    ..
                } = expression
                {
                    let repeat = self.expression(repeat);
                    self.discard(repeat);
                }
                let reference_region = values
                    .iter()
                    .filter_map(|value| value.reference_region)
                    .reduce(|a, b| self.shorter_region(a, b));
                let borrows = values.drain(..).flat_map(|value| value.borrows).collect();
                ExpressionValue {
                    reference_region,
                    borrows,
                }
            }
            Expr::Try { operand, .. } => {
                let value = self.expression(operand);
                if value.contains_reference() {
                    value
                } else {
                    self.discard(value);
                    self.typed_value(expression)
                }
            }
            Expr::RecordLiteral { fields, .. } => {
                let values = fields
                    .iter()
                    .map(|field| self.expression(&field.value))
                    .collect::<Vec<_>>();
                let reference_region = values
                    .iter()
                    .filter_map(|value| value.reference_region)
                    .reduce(|a, b| self.shorter_region(a, b));
                let borrows = values.into_iter().flat_map(|value| value.borrows).collect();
                ExpressionValue {
                    reference_region,
                    borrows,
                }
            }
            Expr::Assign {
                target,
                value,
                span,
            } => {
                let value = self.expression(value);
                self.assign_place(target, value.reference_region, *span);
                if value.contains_reference() {
                    self.retain(value.borrows);
                } else {
                    self.discard(value);
                }
                ExpressionValue::default()
            }
            Expr::Borrow {
                mutable,
                target,
                span,
            } => self.borrow_place(target, *mutable, *span),
            Expr::Unary {
                operator,
                operand,
                span,
            } => {
                let value = self.expression(operand);
                if *operator == UnaryOp::Dereference
                    && let Some(Type::Reference { inner, .. }) = self.expression_types.get(operand)
                    && !self.is_copy(inner)
                {
                    self.diagnostic("cannot move a non-Copy value out of a reference", *span);
                }
                self.discard(value);
                self.typed_value(expression)
            }
            Expr::Cast { operand, .. } => {
                let value = self.expression(operand);
                self.discard(value);
                self.typed_value(expression)
            }
            Expr::Binary { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
                ..
            } => {
                let left = self.expression(left);
                let right = self.expression(right);
                self.discard(left);
                self.discard(right);
                self.typed_value(expression)
            }
            Expr::Call {
                callee,
                arguments,
                span: _,
            } => {
                if matches!(
                    callee_name(callee),
                    Some("#rils_native_print" | "#rils_native_println")
                ) {
                    let callee = self.expression(callee);
                    self.discard(callee);
                    for (index, argument) in arguments.iter().enumerate() {
                        if index > 0 && place_key(argument).is_some() {
                            let value = self.borrow_place(argument, false, argument.span());
                            self.discard(value);
                        } else {
                            let value = self.expression(argument);
                            self.discard(value);
                        }
                    }
                    return self.typed_value(expression);
                }
                let receiver = if let Expr::Member { object, name, .. } = callee.as_ref() {
                    self.receiver_effect(object, name)
                } else {
                    let callee = self.expression(callee);
                    self.discard(callee);
                    None
                };
                let values = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Vec<_>>();
                let result_type = self.expression_types.get(expression);
                let result_region =
                    result_type
                        .filter(|ty| ty.contains_reference())
                        .and_then(|_| {
                            values
                                .iter()
                                .filter_map(|value| value.reference_region)
                                .chain(receiver.as_ref().and_then(|value| value.reference_region))
                                .reduce(|a, b| self.shorter_region(a, b))
                        });
                let result_borrows = values
                    .iter()
                    .flat_map(|value| value.borrows.clone())
                    .chain(
                        receiver
                            .as_ref()
                            .into_iter()
                            .flat_map(|value| value.borrows.clone()),
                    )
                    .collect::<Vec<_>>();
                if result_region.is_none() {
                    for value in values {
                        self.discard(value);
                    }
                }
                if result_region.is_none()
                    && let Some(receiver) = receiver
                {
                    self.discard(receiver);
                }
                let mut result = self.typed_value(expression);
                if result_type.is_some_and(Type::contains_reference) {
                    result.reference_region = result_region;
                    result.borrows = result_borrows;
                }
                result
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition = self.expression(condition);
                self.discard(condition);
                let base = self.snapshot();
                let then_value = self.block(then_branch);
                let then_state = self.snapshot();
                self.restore(base.clone());
                let else_value = else_branch
                    .as_deref()
                    .map(|branch| self.expression(branch))
                    .unwrap_or_default();
                let else_state = self.snapshot();
                self.restore(base);
                self.merge_moved(&[then_state.clone(), else_state.clone()]);
                self.merge_active_borrows(&[then_state, else_state]);
                let mut borrows = then_value.borrows;
                borrows.extend(else_value.borrows);
                borrows.dedup_by(|left, right| {
                    left.root == right.root
                        && left.interior == right.interior
                        && left.region == right.region
                });
                ExpressionValue {
                    reference_region: [then_value.reference_region, else_value.reference_region]
                        .into_iter()
                        .flatten()
                        .reduce(|a, b| self.shorter_region(a, b)),
                    borrows,
                }
            }
            Expr::Match { value, arms, .. } => {
                let value = self.expression(value);
                self.discard(value);
                let base = self.snapshot();
                let mut reference_region = None;
                let mut borrows = Vec::new();
                let mut states = Vec::new();
                for arm in arms {
                    self.restore(base.clone());
                    self.push_scope();
                    self.pattern(&arm.pattern);
                    let value = self.expression(&arm.expression);
                    reference_region = match (reference_region, value.reference_region) {
                        (None, region) | (region, None) => region,
                        (Some(a), Some(b)) => Some(self.shorter_region(a, b)),
                    };
                    if value.contains_reference() {
                        borrows.extend(value.borrows);
                    } else {
                        self.discard(value);
                    }
                    self.pop_scope();
                    states.push(self.snapshot());
                }
                self.restore(base);
                self.merge_moved(&states);
                self.merge_active_borrows(&states);
                borrows.dedup_by(|left, right| {
                    left.root == right.root
                        && left.interior == right.interior
                        && left.region == right.region
                });
                ExpressionValue {
                    reference_region,
                    borrows,
                }
            }
            Expr::Block(block) => self.block(block),
        }
    }
}
