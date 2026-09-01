#![allow(non_upper_case_globals)]

use crate::Value;

pub(super) fn constant(
    target: crate::FloatType,
    constant: rils_builtins::FloatConstantId,
) -> Value {
    use crate::FloatType::*;
    use rils_builtins::FloatConstantId::*;
    match (target, constant) {
        (F32, Min) => Value::F32(f32::MIN),
        (F32, Max) => Value::F32(f32::MAX),
        (F32, Epsilon) => Value::F32(f32::EPSILON),
        (F32, MinPositive) => Value::F32(f32::MIN_POSITIVE),
        (F32, Nan) => Value::F32(f32::NAN),
        (F32, Infinity) => Value::F32(f32::INFINITY),
        (F32, NegInfinity) => Value::F32(f32::NEG_INFINITY),
        (F64, Min) => Value::F64(f64::MIN),
        (F64, Max) => Value::F64(f64::MAX),
        (F64, Epsilon) => Value::F64(f64::EPSILON),
        (F64, MinPositive) => Value::F64(f64::MIN_POSITIVE),
        (F64, Nan) => Value::F64(f64::NAN),
        (F64, Infinity) => Value::F64(f64::INFINITY),
        (F64, NegInfinity) => Value::F64(f64::NEG_INFINITY),
    }
}

pub(super) fn handles(id: rils_builtins::BuiltinId) -> bool {
    rils_builtins::FLOAT_INTRINSICS
        .iter()
        .any(|item| item.id == id)
}

pub(super) fn execute(id: rils_builtins::BuiltinId, values: &[Value]) -> Result<Value, String> {
    match values.first() {
        Some(Value::F32(value)) => float!(id, *value, f32, Value::F32, values),
        Some(Value::F64(value)) => float!(id, *value, f64, Value::F64, values),
        Some(value) => Err(format!(
            "float intrinsic expects a float receiver, found {}",
            value.type_name()
        )),
        None => Err("float intrinsic is missing its receiver".into()),
    }
}

macro_rules! float {
    ($id:expr, $value:expr, $ty:ty, $ctor:path, $values:expr) => {{
        use rils_builtins::builtin_ids::*;
        match $id {
            FloatIsNan => Ok(Value::Bool($value.is_nan())),
            FloatIsInfinite => Ok(Value::Bool($value.is_infinite())),
            FloatIsFinite => Ok(Value::Bool($value.is_finite())),
            FloatIsNormal => Ok(Value::Bool($value.is_normal())),
            FloatIsSignPositive => Ok(Value::Bool($value.is_sign_positive())),
            FloatIsSignNegative => Ok(Value::Bool($value.is_sign_negative())),
            FloatAbs => Ok($ctor($value.abs())),
            FloatSignum => Ok($ctor($value.signum())),
            FloatFloor => Ok($ctor($value.floor())),
            FloatCeil => Ok($ctor($value.ceil())),
            FloatRound => Ok($ctor($value.round())),
            FloatTrunc => Ok($ctor($value.trunc())),
            FloatFract => Ok($ctor($value.fract())),
            FloatSqrt => Ok($ctor($value.sqrt())),
            FloatRecip => Ok($ctor($value.recip())),
            FloatCopysign | FloatMin | FloatMax => {
                let right = operand::<$ty>($values, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                Ok($ctor(match $id {
                    FloatCopysign => $value.copysign(right),
                    FloatMin => $value.min(right),
                    FloatMax => $value.max(right),
                    _ => unreachable!(),
                }))
            }
            FloatClamp => {
                let min = operand_at::<$ty>($values, 1, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                let max = operand_at::<$ty>($values, 2, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                if min.is_nan() || max.is_nan() || min > max {
                    Err("float clamp requires non-NaN bounds with min <= max".into())
                } else {
                    Ok($ctor($value.clamp(min, max)))
                }
            }
            FloatMulAdd => {
                let multiplier = operand_at::<$ty>($values, 1, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                let addend = operand_at::<$ty>($values, 2, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                Ok($ctor($value.mul_add(multiplier, addend)))
            }
            _ => unreachable!(),
        }
    }};
}

use float;

fn operand<T: Copy>(
    values: &[Value],
    extract: impl FnOnce(&Value) -> Option<T>,
) -> Result<T, String> {
    operand_at(values, 1, extract)
}

fn operand_at<T: Copy>(
    values: &[Value],
    index: usize,
    extract: impl FnOnce(&Value) -> Option<T>,
) -> Result<T, String> {
    values
        .get(index)
        .and_then(extract)
        .ok_or_else(|| "float intrinsic operands must have the same type".into())
}
