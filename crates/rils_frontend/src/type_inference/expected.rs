//! Propagate a checked context type to constructors that cannot infer it alone.

use super::*;

impl Inferencer<'_> {
    pub(super) fn apply_expected_type(&mut self, expression: &Expr, expected: &Type) {
        let expected = self.resolve_type(expected);
        match (expression, &expected) {
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
                Type::Option(inner),
            ) if matches!(callee.as_ref(), Expr::Variable { name, .. } if name == "Some") => {
                if let Some(argument) = arguments.first() {
                    self.apply_expected_type(argument, inner);
                }
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
        for statement in &block.statements {
            if let Stmt::Return {
                value: Some(value), ..
            } = statement
            {
                self.apply_expected_type(value, expected);
            }
        }
    }
}
