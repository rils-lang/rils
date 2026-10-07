//! Visit return values in one function, excluding nested function declarations.

use super::{Block, Expr, Stmt};

pub(super) fn visit_block(block: &Block, visitor: &mut impl FnMut(&Expr)) {
    for statement in &block.statements {
        match statement {
            Stmt::Return {
                value: Some(value), ..
            } => {
                visitor(value);
                visit_expression(value, visitor);
            }
            Stmt::Let {
                initializer: expression,
                ..
            }
            | Stmt::Expr { expression, .. }
            | Stmt::Break {
                value: Some(expression),
                ..
            } => visit_expression(expression, visitor),
            Stmt::While {
                condition, body, ..
            } => {
                visit_expression(condition, visitor);
                visit_block(body, visitor);
            }
            Stmt::Loop { body, .. } => visit_block(body, visitor),
            Stmt::For { iterable, body, .. } => {
                visit_expression(iterable, visitor);
                visit_block(body, visitor);
            }
            _ => {}
        }
    }
}

fn visit_expression(expression: &Expr, visitor: &mut impl FnMut(&Expr)) {
    match expression {
        Expr::Member { object, .. }
        | Expr::Borrow { target: object, .. }
        | Expr::Unary {
            operand: object, ..
        }
        | Expr::Cast {
            operand: object, ..
        }
        | Expr::Try {
            operand: object, ..
        } => visit_expression(object, visitor),
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
            visit_expression(left, visitor);
            visit_expression(right, visitor);
        }
        Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
            for element in elements {
                visit_expression(element, visitor);
            }
            if let Expr::Array {
                repeat: Some(repeat),
                ..
            } = expression
            {
                visit_expression(repeat, visitor);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for field in fields {
                visit_expression(&field.value, visitor);
            }
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            visit_expression(callee, visitor);
            for argument in arguments {
                visit_expression(argument, visitor);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expression(condition, visitor);
            visit_block(then_branch, visitor);
            if let Some(branch) = else_branch {
                visit_expression(branch, visitor);
            }
        }
        Expr::Match { value, arms, .. } => {
            visit_expression(value, visitor);
            for arm in arms {
                visit_expression(&arm.expression, visitor);
            }
        }
        Expr::Block(block) => visit_block(block, visitor),
        Expr::Literal { .. }
        | Expr::Variable { .. }
        | Expr::Path { .. }
        | Expr::GenericPath { .. }
        | Expr::QualifiedPath { .. } => {}
    }
}
