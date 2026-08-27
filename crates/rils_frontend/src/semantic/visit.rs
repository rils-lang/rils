use crate::{
    Type,
    ast::{Block, Expr, Stmt},
};

use super::qualified_name;

pub(super) fn visit_statements(
    statements: &[Stmt],
    namespace: &mut Vec<String>,
    self_type: Option<&str>,
    visitor: &mut impl FnMut(&Expr, &[String], Option<&str>),
) {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
            Stmt::Module {
                name,
                statements: Some(statements),
                ..
            } => {
                namespace.push(name.clone());
                visit_statements(statements, namespace, self_type, visitor);
                namespace.pop();
            }
            Stmt::Let { initializer, .. } => {
                visit_expression(initializer, namespace, self_type, visitor)
            }
            Stmt::Function { body, .. } => visit_block(body, namespace, self_type, visitor),
            Stmt::Impl {
                target, methods, ..
            } => {
                let impl_type = match target {
                    Type::Named { name, .. } => Some(qualified_name(namespace, name)),
                    _ => None,
                };
                for method in methods {
                    visit_block(&method.body, namespace, impl_type.as_deref(), visitor);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                visit_expression(condition, namespace, self_type, visitor);
                visit_block(body, namespace, self_type, visitor);
            }
            Stmt::Loop { body, .. } => visit_block(body, namespace, self_type, visitor),
            Stmt::For { iterable, body, .. } => {
                visit_expression(iterable, namespace, self_type, visitor);
                visit_block(body, namespace, self_type, visitor);
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    visit_expression(value, namespace, self_type, visitor);
                }
            }
            Stmt::Expr { expression, .. } => {
                visit_expression(expression, namespace, self_type, visitor)
            }
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Continue { .. } => {}
            Stmt::Public { .. } => unreachable!("public statements were unwrapped"),
        }
    }
}

fn visit_block(
    block: &Block,
    namespace: &mut Vec<String>,
    self_type: Option<&str>,
    visitor: &mut impl FnMut(&Expr, &[String], Option<&str>),
) {
    visit_statements(&block.statements, namespace, self_type, visitor);
}

fn visit_expression(
    expression: &Expr,
    namespace: &mut Vec<String>,
    self_type: Option<&str>,
    visitor: &mut impl FnMut(&Expr, &[String], Option<&str>),
) {
    visitor(expression, namespace, self_type);
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
        } => visit_expression(object, namespace, self_type, visitor),
        Expr::Index { object, index, .. } => {
            visit_expression(object, namespace, self_type, visitor);
            visit_expression(index, namespace, self_type, visitor);
        }
        Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
            for element in elements {
                visit_expression(element, namespace, self_type, visitor);
            }
            if let Expr::Array {
                repeat: Some(repeat),
                ..
            } = expression
            {
                visit_expression(repeat, namespace, self_type, visitor);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for field in fields {
                visit_expression(&field.value, namespace, self_type, visitor);
            }
        }
        Expr::Assign { target, value, .. } => {
            visit_expression(target, namespace, self_type, visitor);
            visit_expression(value, namespace, self_type, visitor);
        }
        Expr::Binary { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            visit_expression(left, namespace, self_type, visitor);
            visit_expression(right, namespace, self_type, visitor);
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            visit_expression(callee, namespace, self_type, visitor);
            for argument in arguments {
                visit_expression(argument, namespace, self_type, visitor);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expression(condition, namespace, self_type, visitor);
            visit_block(then_branch, namespace, self_type, visitor);
            if let Some(else_branch) = else_branch {
                visit_expression(else_branch, namespace, self_type, visitor);
            }
        }
        Expr::Match { value, arms, .. } => {
            visit_expression(value, namespace, self_type, visitor);
            for arm in arms {
                visit_expression(&arm.expression, namespace, self_type, visitor);
            }
        }
        Expr::Block(block) => visit_block(block, namespace, self_type, visitor),
        Expr::Literal { .. }
        | Expr::Variable { .. }
        | Expr::Path { .. }
        | Expr::QualifiedPath { .. } => {}
    }
}
