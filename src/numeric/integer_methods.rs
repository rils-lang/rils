#![allow(non_upper_case_globals)]

use crate::{Type, Value};

pub(super) fn constant(
    target: crate::IntegerType,
    constant: rils_builtins::IntegerConstantId,
) -> Value {
    use crate::IntegerType::*;
    use rils_builtins::IntegerConstantId::*;
    if constant == Bits {
        return Value::U32(target.bits());
    }
    match (target, constant) {
        (I8, Min) => Value::I8(i8::MIN),
        (I8, Max) => Value::I8(i8::MAX),
        (I16, Min) => Value::I16(i16::MIN),
        (I16, Max) => Value::I16(i16::MAX),
        (I32, Min) => Value::I32(i32::MIN),
        (I32, Max) => Value::I32(i32::MAX),
        (I64, Min) => Value::I64(i64::MIN),
        (I64, Max) => Value::I64(i64::MAX),
        (I128, Min) => Value::I128(i128::MIN),
        (I128, Max) => Value::I128(i128::MAX),
        (Isize, Min) => Value::Isize(isize::MIN),
        (Isize, Max) => Value::Isize(isize::MAX),
        (U8, Min) => Value::U8(u8::MIN),
        (U8, Max) => Value::U8(u8::MAX),
        (U16, Min) => Value::U16(u16::MIN),
        (U16, Max) => Value::U16(u16::MAX),
        (U32, Min) => Value::U32(u32::MIN),
        (U32, Max) => Value::U32(u32::MAX),
        (U64, Min) => Value::U64(u64::MIN),
        (U64, Max) => Value::U64(u64::MAX),
        (U128, Min) => Value::U128(u128::MIN),
        (U128, Max) => Value::U128(u128::MAX),
        (Usize, Min) => Value::Usize(usize::MIN),
        (Usize, Max) => Value::Usize(usize::MAX),
        (_, Bits) => unreachable!(),
    }
}

pub(super) fn handles(id: rils_builtins::BuiltinId) -> bool {
    use rils_builtins::builtin_ids::*;
    matches!(
        id,
        IntegerCheckedNeg
            | IntegerCheckedAbs
            | IntegerCheckedPow
            | IntegerCheckedShl
            | IntegerCheckedShr
            | IntegerWrappingNeg
            | IntegerWrappingPow
            | IntegerWrappingShl
            | IntegerWrappingShr
            | IntegerSaturatingNeg
            | IntegerSaturatingAbs
            | IntegerSaturatingPow
            | IntegerOverflowingNeg
            | IntegerOverflowingAbs
            | IntegerOverflowingPow
            | IntegerOverflowingShl
            | IntegerOverflowingShr
            | IntegerCountOnes
            | IntegerCountZeros
            | IntegerLeadingZeros
            | IntegerTrailingZeros
            | IntegerRotateLeft
            | IntegerRotateRight
            | IntegerPow
            | IntegerDivEuclid
            | IntegerRemEuclid
            | IntegerAbs
            | IntegerSwapBytes
            | IntegerReverseBits
    )
}

pub(super) fn execute(id: rils_builtins::BuiltinId, values: &[Value]) -> Result<Value, String> {
    match values.first() {
        Some(Value::I8(value)) => signed!(id, *value, i8, Value::I8, values),
        Some(Value::I16(value)) => signed!(id, *value, i16, Value::I16, values),
        Some(Value::I32(value)) => signed!(id, *value, i32, Value::I32, values),
        Some(Value::I64(value)) => signed!(id, *value, i64, Value::I64, values),
        Some(Value::I128(value)) => signed!(id, *value, i128, Value::I128, values),
        Some(Value::Isize(value)) => signed!(id, *value, isize, Value::Isize, values),
        Some(Value::U8(value)) => unsigned!(id, *value, u8, Value::U8, values),
        Some(Value::U16(value)) => unsigned!(id, *value, u16, Value::U16, values),
        Some(Value::U32(value)) => unsigned!(id, *value, u32, Value::U32, values),
        Some(Value::U64(value)) => unsigned!(id, *value, u64, Value::U64, values),
        Some(Value::U128(value)) => unsigned!(id, *value, u128, Value::U128, values),
        Some(Value::Usize(value)) => unsigned!(id, *value, usize, Value::Usize, values),
        Some(value) => Err(format!(
            "integer intrinsic expects an integer receiver, found {}",
            value.type_name()
        )),
        None => Err("integer intrinsic is missing its receiver".into()),
    }
}

macro_rules! common {
    ($id:expr, $value:expr, $ty:ty, $ctor:path, $values:expr, $neg:expr, $sat_neg:expr, $abs:expr) => {{
        use rils_builtins::builtin_ids::*;
        match $id {
            IntegerCountOnes => Ok(Value::U32($value.count_ones())),
            IntegerCountZeros => Ok(Value::U32($value.count_zeros())),
            IntegerLeadingZeros => Ok(Value::U32($value.leading_zeros())),
            IntegerTrailingZeros => Ok(Value::U32($value.trailing_zeros())),
            IntegerRotateLeft => exponent($values).map(|amount| $ctor($value.rotate_left(amount))),
            IntegerRotateRight => {
                exponent($values).map(|amount| $ctor($value.rotate_right(amount)))
            }
            IntegerSwapBytes => Ok($ctor($value.swap_bytes())),
            IntegerReverseBits => Ok($ctor($value.reverse_bits())),
            IntegerCheckedShl => exponent($values).map(|amount| {
                option(
                    $value.checked_shl(amount).map($ctor),
                    Type::of_value(&$ctor($value)),
                )
            }),
            IntegerCheckedShr => exponent($values).map(|amount| {
                option(
                    $value.checked_shr(amount).map($ctor),
                    Type::of_value(&$ctor($value)),
                )
            }),
            IntegerWrappingShl => {
                exponent($values).map(|amount| $ctor($value.wrapping_shl(amount)))
            }
            IntegerWrappingShr => {
                exponent($values).map(|amount| $ctor($value.wrapping_shr(amount)))
            }
            IntegerOverflowingShl => exponent($values).map(|amount| {
                let (value, overflowed) = $value.overflowing_shl(amount);
                overflowing($ctor(value), overflowed)
            }),
            IntegerOverflowingShr => exponent($values).map(|amount| {
                let (value, overflowed) = $value.overflowing_shr(amount);
                overflowing($ctor(value), overflowed)
            }),
            IntegerPow => exponent($values).and_then(|power| {
                $value
                    .checked_pow(power)
                    .map($ctor)
                    .ok_or_else(|| "integer overflow".into())
            }),
            IntegerCheckedPow => exponent($values).map(|power| {
                option(
                    $value.checked_pow(power).map($ctor),
                    Type::of_value(&$ctor($value)),
                )
            }),
            IntegerWrappingPow => exponent($values).map(|power| $ctor($value.wrapping_pow(power))),
            IntegerSaturatingPow => {
                exponent($values).map(|power| $ctor(saturating_pow($value, power)))
            }
            IntegerOverflowingPow => exponent($values).map(|power| {
                let (value, overflowed) = $value.overflowing_pow(power);
                overflowing($ctor(value), overflowed)
            }),
            IntegerDivEuclid | IntegerRemEuclid => {
                let right = same_type::<$ty>($values, |value| match value {
                    $ctor(inner) => Some(*inner),
                    _ => None,
                })?;
                if right == 0 {
                    Err("division by zero".into())
                } else if $id == IntegerDivEuclid {
                    $value
                        .checked_div_euclid(right)
                        .map($ctor)
                        .ok_or_else(|| "integer overflow".into())
                } else {
                    $value
                        .checked_rem_euclid(right)
                        .map($ctor)
                        .ok_or_else(|| "integer overflow".into())
                }
            }
            IntegerCheckedNeg => Ok(option(
                ($neg)($value).map($ctor),
                Type::of_value(&$ctor($value)),
            )),
            IntegerWrappingNeg => Ok($ctor($value.wrapping_neg())),
            IntegerSaturatingNeg => Ok($ctor(($sat_neg)($value))),
            IntegerOverflowingNeg => {
                let (value, overflowed) = $value.overflowing_neg();
                Ok(overflowing($ctor(value), overflowed))
            }
            IntegerAbs => ($abs)($value)
                .map($ctor)
                .ok_or_else(|| "integer overflow".into()),
            IntegerCheckedAbs => Ok(option(
                ($abs)($value).map($ctor),
                Type::of_value(&$ctor($value)),
            )),
            IntegerSaturatingAbs => Ok($ctor(if $value == <$ty>::MIN {
                <$ty>::MAX
            } else {
                ($abs)($value).expect("non-min absolute value")
            })),
            IntegerOverflowingAbs => match ($abs)($value) {
                Some(value) => Ok(overflowing($ctor(value), false)),
                None => Ok(overflowing($ctor($value), true)),
            },
            _ => unreachable!(),
        }
    }};
}

macro_rules! signed {
    ($id:expr, $value:expr, $ty:ty, $ctor:path, $values:expr) => {
        common!(
            $id,
            $value,
            $ty,
            $ctor,
            $values,
            |value: $ty| value.checked_neg(),
            |value: $ty| value.saturating_neg(),
            |value: $ty| value.checked_abs()
        )
    };
}

macro_rules! unsigned {
    ($id:expr, $value:expr, $ty:ty, $ctor:path, $values:expr) => {{
        use rils_builtins::builtin_ids::*;
        match $id {
            IntegerCheckedNeg => Ok(option(
                ($value == 0).then_some($ctor(0)),
                Type::of_value(&$ctor($value)),
            )),
            IntegerWrappingNeg => Ok($ctor($value.wrapping_neg())),
            IntegerSaturatingNeg => Ok($ctor(0)),
            IntegerOverflowingNeg => {
                let (value, overflowed) = $value.overflowing_neg();
                Ok(overflowing($ctor(value), overflowed))
            }
            IntegerAbs | IntegerSaturatingAbs => Ok($ctor($value)),
            IntegerCheckedAbs => Ok(option(Some($ctor($value)), Type::of_value(&$ctor($value)))),
            IntegerOverflowingAbs => Ok(overflowing($ctor($value), false)),
            _ => common!(
                $id,
                $value,
                $ty,
                $ctor,
                $values,
                |value: $ty| value.checked_neg(),
                |_value: $ty| 0,
                |value: $ty| Some(value)
            ),
        }
    }};
}

use common;
use signed;
use unsigned;

fn exponent(values: &[Value]) -> Result<u32, String> {
    match values.get(1) {
        Some(Value::U32(value)) => Ok(*value),
        Some(value) => Err(format!(
            "integer exponent must be u32, found {}",
            value.type_name()
        )),
        None => Err("integer exponent is missing".into()),
    }
}

fn same_type<T: Copy>(
    values: &[Value],
    extract: impl FnOnce(&Value) -> Option<T>,
) -> Result<T, String> {
    values
        .get(1)
        .and_then(extract)
        .ok_or_else(|| "integer intrinsic operands must have the same type".into())
}

fn option(value: Option<Value>, element_type: Option<Type>) -> Value {
    Value::Option {
        value: value.map(std::rc::Rc::new),
        element_type,
    }
}

fn overflowing(value: Value, overflowed: bool) -> Value {
    let value_type = Type::of_value(&value).unwrap_or(Type::Unknown);
    Value::Tuple(std::rc::Rc::new(crate::value::SequenceValue {
        elements: std::cell::RefCell::new(vec![
            crate::value::FieldSlot {
                value: Some(value),
                type_annotation: value_type,
                references: 0,
            },
            crate::value::FieldSlot {
                value: Some(Value::Bool(overflowed)),
                type_annotation: Type::Bool,
                references: 0,
            },
        ]),
        element_type: std::cell::RefCell::new(None),
    }))
}

fn saturating_pow<T>(mut base: T, mut power: u32) -> T
where
    T: Copy + IntegerOne + SaturatingMultiply,
{
    let mut result = T::one();
    while power > 0 {
        if power & 1 == 1 {
            result = result.saturating_multiply(base);
        }
        power >>= 1;
        if power > 0 {
            base = base.saturating_multiply(base);
        }
    }
    result
}

trait SaturatingMultiply {
    fn saturating_multiply(self, right: Self) -> Self;
}

trait IntegerOne {
    fn one() -> Self;
}

macro_rules! impl_saturating_multiply {
    ($($ty:ty),* $(,)?) => {$(
        impl SaturatingMultiply for $ty {
            fn saturating_multiply(self, right: Self) -> Self {
                self.saturating_mul(right)
            }
        }
        impl IntegerOne for $ty {
            fn one() -> Self { 1 }
        }
    )*};
}

impl_saturating_multiply!(
    i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize
);
