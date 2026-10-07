//! Propagate a checked context type to constructors that cannot infer it alone.

use super::*;

mod returns;

impl Inferencer<'_> {
    pub(super) fn apply_expected_type(&mut self, expression: &Expr, expected: &Type) {
        let expected = self.resolve_type(expected);
        match (expression, &expected) {
            (Expr::Try { operand, .. }, _) => {
                let id = self.expression_ids.id(operand);
                if let Some(Type::Result(_, error)) =
                    self.result.expression_types_by_id.get(&id).cloned()
                {
                    self.apply_expected_type(
                        operand,
                        &Type::Result(Box::new(expected.clone()), error),
                    );
                    self.record_compatible_type(expression, &expected);
                }
            }
            (Expr::RecordLiteral { path, fields, .. }, Type::Named { name, arguments })
                if is_known(&expected) =>
            {
                let id = self.expression_ids.id(expression);
                if self
                    .result
                    .expression_types_by_id
                    .get(&id)
                    .is_none_or(|actual| merge_types(actual, &expected).is_none())
                {
                    return;
                }
                let Some(definition) = self.types.get(name).cloned() else {
                    return;
                };
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect();
                let declared = path
                    .last()
                    .and_then(|variant| definition.variants.get(variant))
                    .and_then(|variant| match variant {
                        VariantDefinition::Record(fields) => Some(fields),
                        _ => None,
                    })
                    .unwrap_or(&definition.fields);
                for field in fields {
                    if let Some(ty) = declared.get(&field.name) {
                        self.apply_expected_type(&field.value, &ty.substitute(&substitutions));
                    }
                }
                self.result.expression_types_by_id.insert(id, expected);
            }
            (
                Expr::Call {
                    callee,
                    arguments: values,
                    ..
                },
                Type::Named { name, arguments },
            ) if is_known(&expected)
                && self
                    .nominal_variant_type(callee)
                    .is_some_and(|actual| merge_types(&actual, &expected).is_some()) =>
            {
                let Some(definition) = self.types.get(name).cloned() else {
                    return;
                };
                let path = match callee.as_ref() {
                    Expr::Path { segments, .. } | Expr::GenericPath { segments, .. } => segments,
                    _ => return,
                };
                let Some(VariantDefinition::Tuple(fields)) = path
                    .last()
                    .and_then(|variant| definition.variants.get(variant))
                else {
                    return;
                };
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect();
                for (value, ty) in values.iter().zip(fields) {
                    self.apply_expected_type(value, &ty.substitute(&substitutions));
                }
                let id = self.expression_ids.id(expression);
                self.result.expression_types_by_id.insert(id, expected);
            }
            (Expr::Path { .. } | Expr::GenericPath { .. }, Type::Named { .. })
                if is_known(&expected)
                    && self
                        .nominal_variant_type(expression)
                        .is_some_and(|actual| merge_types(&actual, &expected).is_some()) =>
            {
                let id = self.expression_ids.id(expression);
                self.result.expression_types_by_id.insert(id, expected);
            }
            (Expr::Variable { name, .. }, Type::Option(_)) if name == "None" => {
                if is_known(&expected) {
                    let id = self.expression_ids.id(expression);
                    self.result.expression_types_by_id.insert(id, expected);
                }
            }
            (
                Expr::Call {
                    callee, arguments, ..
                },
                Type::Named { name, .. },
            ) if arguments.is_empty()
                && is_known(&expected)
                && matches!(callee.as_ref(),
                    Expr::Path { segments, .. } | Expr::GenericPath { segments, .. }
                        if segments.len() >= 2
                            && segments[segments.len() - 2] == *name
                            && segments.last().is_some_and(|member| member == "new")
                            && crate::standard_library::empty_native_constructor_symbol(name)
                                .is_some()) =>
            {
                let id = self.expression_ids.id(expression);
                self.result.expression_types_by_id.insert(id, expected);
            }
            (Expr::Call { arguments, .. }, Type::Option(inner))
                if self
                    .result
                    .sum_constructors
                    .get(&self.expression_ids.id(expression))
                    == Some(&"Some") =>
            {
                if let Some(argument) = arguments.first() {
                    self.apply_expected_type(argument, inner);
                }
                self.record_compatible_type(expression, &expected);
            }
            (Expr::Call { arguments, .. }, Type::Result(ok, error)) => {
                let ty = match self
                    .result
                    .sum_constructors
                    .get(&self.expression_ids.id(expression))
                {
                    Some(&"Ok") => ok,
                    Some(&"Err") => error,
                    _ => return,
                };
                if let Some(argument) = arguments.first() {
                    self.apply_expected_type(argument, ty);
                }
                self.record_compatible_type(expression, &expected);
            }
            (Expr::Tuple { elements, .. }, Type::Tuple(types)) => {
                for (element, expected) in elements.iter().zip(types) {
                    self.apply_expected_type(element, expected);
                }
            }
            (
                Expr::Array { elements, .. },
                Type::Array { element, .. } | Type::ArrayParameter { element, .. },
            ) => {
                for value in elements {
                    self.apply_expected_type(value, element);
                }
            }
            (Expr::Block(block), _) => self.apply_expected_block_tail(block, &expected),
            (
                Expr::If {
                    then_branch,
                    else_branch,
                    ..
                },
                _,
            ) => {
                self.apply_expected_block_tail(then_branch, &expected);
                if let Some(else_branch) = else_branch {
                    self.apply_expected_type(else_branch, &expected);
                }
            }
            (Expr::Match { arms, .. }, _) => {
                for arm in arms {
                    self.apply_expected_type(&arm.expression, &expected);
                }
            }
            _ => {}
        }
    }

    fn record_compatible_type(&mut self, expression: &Expr, expected: &Type) {
        if !is_known(expected) {
            return;
        }
        let id = self.expression_ids.id(expression);
        let actual = self
            .result
            .expression_types_by_id
            .get(&id)
            .cloned()
            .map(|actual| self.resolve_type(&actual));
        if actual.is_some_and(|actual| merge_types(&actual, expected).is_some()) {
            self.result
                .expression_types_by_id
                .insert(id, expected.clone());
        }
    }

    pub(super) fn apply_expected_block_tail(&mut self, block: &Block, expected: &Type) {
        if let Some(Stmt::Expr {
            expression,
            terminated: false,
        }) = block.statements.last()
        {
            self.apply_expected_type(expression, expected);
        }
    }

    pub(super) fn apply_expected_returns(&mut self, block: &Block, expected: &Type) {
        returns::visit_block(block, &mut |value| {
            self.apply_expected_type(value, expected)
        });
    }

    pub(super) fn apply_function_return_type(&mut self, block: &Block, expected: &Type) {
        self.apply_expected_block_tail(block, expected);
        self.apply_expected_returns(block, expected);
        if let Type::Result(_, error_type) = expected {
            returns::visit_try_operands(block, &mut |operand| {
                let id = self.expression_ids.id(operand);
                if let Some(Type::Result(ok, _)) =
                    self.result.expression_types_by_id.get(&id).cloned()
                {
                    self.apply_expected_type(operand, &Type::Result(ok, error_type.clone()));
                }
            });
        }
    }
}
