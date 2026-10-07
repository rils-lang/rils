//! Visit explicit and propagated returns without entering nested functions.

use super::{Block, Expr, Stmt};

pub(super) fn visit_block(block: &Block, visitor: &mut impl FnMut(&Expr)) {
    walk_block(block, visitor, &mut |_| {});
}

pub(super) fn visit_try_operands(block: &Block, visitor: &mut impl FnMut(&Expr)) {
    walk_block(block, &mut |_| {}, visitor);
}

fn walk_block(block: &Block, visitor: &mut impl FnMut(&Expr), propagated: &mut impl FnMut(&Expr)) {
    for statement in &block.statements {
        match statement {
            Stmt::Return {
                value: Some(value), ..
            } => {
                visitor(value);
                walk_expression(value, visitor, propagated);
            }
            Stmt::Let {
                initializer: expression,
                ..
            }
            | Stmt::Expr { expression, .. }
            | Stmt::Break {
                value: Some(expression),
                ..
            } => walk_expression(expression, visitor, propagated),
            Stmt::While {
                condition, body, ..
            } => {
                walk_expression(condition, visitor, propagated);
                walk_block(body, visitor, propagated);
            }
            Stmt::Loop { body, .. } => walk_block(body, visitor, propagated),
            Stmt::For { iterable, body, .. } => {
                walk_expression(iterable, visitor, propagated);
                walk_block(body, visitor, propagated);
            }
            _ => {}
        }
    }
}

fn walk_expression(
    expression: &Expr,
    visitor: &mut impl FnMut(&Expr),
    propagated: &mut impl FnMut(&Expr),
) {
    match expression {
        Expr::Member { object, .. }
        | Expr::Borrow { target: object, .. }
        | Expr::Unary {
            operand: object, ..
        }
        | Expr::Cast {
            operand: object, ..
        } => walk_expression(object, visitor, propagated),
        Expr::Try { operand, .. } => {
            propagated(operand);
            walk_expression(operand, visitor, propagated);
        }
        Expr::Index {
            object: left,
            index: right,
            ..
        }
        | Expr::Assign {
            target: left,
            value: right,
            ..
        }
        | Expr::Binary { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            walk_expression(left, visitor, propagated);
            walk_expression(right, visitor, propagated);
        }
        Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
            for element in elements {
                walk_expression(element, visitor, propagated);
            }
            if let Expr::Array {
                repeat: Some(repeat),
                ..
            } = expression
            {
                walk_expression(repeat, visitor, propagated);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for field in fields {
                walk_expression(&field.value, visitor, propagated);
            }
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            walk_expression(callee, visitor, propagated);
            for argument in arguments {
                walk_expression(argument, visitor, propagated);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            walk_expression(condition, visitor, propagated);
            walk_block(then_branch, visitor, propagated);
            if let Some(branch) = else_branch {
                walk_expression(branch, visitor, propagated);
            }
        }
        Expr::Match { value, arms, .. } => {
            walk_expression(value, visitor, propagated);
            for arm in arms {
                walk_expression(&arm.expression, visitor, propagated);
            }
        }
        Expr::Block(block) => walk_block(block, visitor, propagated),
        Expr::Literal { .. }
        | Expr::Variable { .. }
        | Expr::Path { .. }
        | Expr::GenericPath { .. }
        | Expr::QualifiedPath { .. } => {}
    }
}
