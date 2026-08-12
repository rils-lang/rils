use std::collections::HashMap;

use crate::{
    ast::{Block, Expr, Literal, Program, Stmt},
    source::Span,
    type_inference,
    types::{FloatType, FunctionSignature, IntegerType, Type},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NumericResolutionError {
    pub message: String,
    pub span: Span,
}

pub fn resolve_numeric_literals(program: &mut Program) -> Result<(), NumericResolutionError> {
    resolve_numeric_literals_with_host_functions(program, &HashMap::new())
}

#[doc(hidden)]
pub fn resolve_numeric_literals_with_host_functions(
    program: &mut Program,
    host_functions: &HashMap<String, FunctionSignature>,
) -> Result<(), NumericResolutionError> {
    let types = type_inference::infer_with_host_functions(program, host_functions).expression_types;
    resolve_statements(&mut program.statements, &types)
}

fn resolve_statements(
    statements: &mut [Stmt],
    types: &HashMap<Span, Type>,
) -> Result<(), NumericResolutionError> {
    for statement in statements {
        match statement {
            Stmt::Public { statement, .. } => {
                resolve_statements(std::slice::from_mut(statement.as_mut()), types)?
            }
            Stmt::Module {
                statements: Some(statements),
                ..
            } => resolve_statements(statements, types)?,
            Stmt::Let { initializer, .. } => resolve_expression(initializer, types)?,
            Stmt::Function { body, .. } => resolve_block(body, types)?,
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    resolve_block(&mut method.body, types)?;
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                resolve_expression(condition, types)?;
                resolve_block(body, types)?;
            }
            Stmt::Loop { body, .. } => resolve_block(body, types)?,
            Stmt::For { iterable, body, .. } => {
                resolve_expression(iterable, types)?;
                resolve_block(body, types)?;
            }
            Stmt::Return {
                value: Some(value), ..
            }
            | Stmt::Break {
                value: Some(value), ..
            } => resolve_expression(value, types)?,
            Stmt::Expr { expression, .. } => resolve_expression(expression, types)?,
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Return { value: None, .. }
            | Stmt::Break { value: None, .. }
            | Stmt::Continue { .. } => {}
        }
    }
    Ok(())
}

fn resolve_block(
    block: &mut Block,
    types: &HashMap<Span, Type>,
) -> Result<(), NumericResolutionError> {
    resolve_statements(&mut block.statements, types)
}

fn resolve_expression(
    expression: &mut Expr,
    types: &HashMap<Span, Type>,
) -> Result<(), NumericResolutionError> {
    let span = expression.span();
    match expression {
        Expr::Literal { value, .. } => resolve_literal(value, types.get(&span), span)?,
        Expr::Member { object, .. }
        | Expr::Try {
            operand: object, ..
        }
        | Expr::Cast {
            operand: object, ..
        }
        | Expr::Borrow { target: object, .. }
        | Expr::Unary {
            operand: object, ..
        } => resolve_expression(object, types)?,
        Expr::Index { object, index, .. }
        | Expr::Assign {
            target: object,
            value: index,
            ..
        }
        | Expr::Binary {
            left: object,
            right: index,
            ..
        }
        | Expr::Logical {
            left: object,
            right: index,
            ..
        }
        | Expr::Range {
            start: object,
            end: index,
            ..
        } => {
            resolve_expression(object, types)?;
            resolve_expression(index, types)?;
        }
        Expr::Tuple { elements, .. } => {
            for element in elements {
                resolve_expression(element, types)?;
            }
        }
        Expr::Array {
            elements, repeat, ..
        } => {
            for element in elements {
                resolve_expression(element, types)?;
            }
            if let Some(repeat) = repeat {
                resolve_expression(repeat, types)?;
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for (_, value) in fields {
                resolve_expression(value, types)?;
            }
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            resolve_expression(callee, types)?;
            for argument in arguments {
                resolve_expression(argument, types)?;
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            resolve_expression(condition, types)?;
            resolve_block(then_branch, types)?;
            if let Some(branch) = else_branch {
                resolve_expression(branch, types)?;
            }
        }
        Expr::Match { value, arms, .. } => {
            resolve_expression(value, types)?;
            for arm in arms {
                resolve_expression(&mut arm.expression, types)?;
            }
        }
        Expr::Block(block) => resolve_block(block, types)?,
        Expr::Variable { .. } | Expr::Path { .. } | Expr::QualifiedPath { .. } => {}
    }
    Ok(())
}

fn resolve_literal(
    literal: &mut Literal,
    inferred: Option<&Type>,
    span: Span,
) -> Result<(), NumericResolutionError> {
    match literal {
        Literal::Integer(value) => {
            let value = *value;
            let ty = match inferred {
                Some(Type::Integer(ty)) => *ty,
                _ => IntegerType::I32,
            };
            *literal = integer_literal(value, ty).ok_or_else(|| NumericResolutionError {
                message: format!("integer literal `{value}` is outside the `{ty}` range"),
                span,
            })?;
        }
        Literal::Float(value) => {
            *literal = match inferred {
                Some(Type::Float(FloatType::F32)) => Literal::F32(*value as f32),
                _ => Literal::F64(*value),
            };
        }
        _ => {}
    }
    Ok(())
}

fn integer_literal(value: i128, ty: IntegerType) -> Option<Literal> {
    macro_rules! signed {
        ($variant:ident, $type:ty) => {
            <$type>::try_from(value).ok().map(Literal::$variant)
        };
    }
    macro_rules! unsigned {
        ($variant:ident, $type:ty) => {
            <$type>::try_from(value).ok().map(Literal::$variant)
        };
    }
    match ty {
        IntegerType::I8 => signed!(I8, i8),
        IntegerType::I16 => signed!(I16, i16),
        IntegerType::I32 => signed!(I32, i32),
        IntegerType::I64 => signed!(I64, i64),
        IntegerType::I128 => Some(Literal::I128(value)),
        IntegerType::Isize => signed!(Isize, isize),
        IntegerType::U8 => unsigned!(U8, u8),
        IntegerType::U16 => unsigned!(U16, u16),
        IntegerType::U32 => unsigned!(U32, u32),
        IntegerType::U64 => unsigned!(U64, u64),
        IntegerType::U128 => unsigned!(U128, u128),
        IntegerType::Usize => unsigned!(Usize, usize),
    }
}
