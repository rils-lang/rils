use crate::{
    ast::{BinaryOp, UnaryOp},
    source::Span,
    types::Type,
    value::Value,
};

use super::BytecodeError;

pub(super) fn condition_value(value: &Value, span: Span) -> Result<bool, BytecodeError> {
    match Type::of_value(value) {
        Some(Type::Unit) => Err(BytecodeError::new(
            "`()` cannot be used as a condition",
            span,
        )),
        Some(Type::Option(_)) => Err(BytecodeError::new(
            "Option cannot be used as a condition",
            span,
        )),
        _ => Ok(value.is_truthy()),
    }
}

pub(super) fn unary(operator: UnaryOp, value: Value, span: Span) -> Result<Value, BytecodeError> {
    match (operator, value) {
        (UnaryOp::Not, value) => Ok(Value::Bool(!condition_value(&value, span)?)),
        (UnaryOp::Negate, value) => {
            crate::numeric::negate(value).map_err(|message| BytecodeError::new(message, span))
        }
        (UnaryOp::Dereference, _) => Err(BytecodeError::new(
            "dereference is not supported by the bytecode MVP",
            span,
        )),
    }
}

pub(super) fn binary(
    left: Value,
    operator: BinaryOp,
    right: Value,
    span: Span,
) -> Result<Value, BytecodeError> {
    use BinaryOp::*;
    if matches!(operator, Equal | NotEqual) {
        let equal = left
            .try_equal(&right)
            .map_err(|message| BytecodeError::new(message, span))?;
        return Ok(Value::Bool(if operator == Equal { equal } else { !equal }));
    }
    if operator == Add
        && let (Some(left), Some(right)) = (left.as_string(), right.as_string())
    {
        return Ok(rils_execution::value::native_string(format!(
            "{left}{right}"
        )));
    }
    crate::numeric::binary(left, operator, right)
        .map_err(|message| BytecodeError::new(message, span))
}
