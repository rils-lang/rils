//! Generic execution bridge for standard-library arithmetic implementations.
use super::native::{NativeInput, NativeOutput};
use crate::{Value, ast::BinaryOp};
use rils_stdlib::arithmetic::{ArithmeticResult, PrimitiveArithmetic};

pub(super) fn binary<T: PrimitiveArithmetic + NativeInput + NativeOutput>(
    left: &Value,
    operator: BinaryOp,
    right: &Value,
) -> Result<Value, String> {
    match T::from_value(left)?.binary(operator, T::from_value(right)?)? {
        ArithmeticResult::Number(value) => value.into_value(),
        ArithmeticResult::Bool(value) => value.into_value(),
    }
}

pub(super) fn negate<T: PrimitiveArithmetic + NativeInput + NativeOutput>(
    value: &Value,
) -> Result<Value, String> {
    T::from_value(value)?.negate()?.into_value()
}
