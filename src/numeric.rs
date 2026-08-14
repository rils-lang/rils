use crate::{IntegerType, Type, ast::BinaryOp, value::Value};

mod integer_methods;

pub(crate) fn integer_constant(
    target: IntegerType,
    constant: rils_builtins::IntegerConstantId,
) -> Value {
    integer_methods::constant(target, constant)
}

pub(crate) fn cast_integer(value: Value, target: IntegerType) -> Result<Value, String> {
    enum IntegerValue {
        Signed(i128),
        Unsigned(u128),
    }

    let source = match value {
        Value::I8(value) => (IntegerType::I8, IntegerValue::Signed(value.into())),
        Value::I16(value) => (IntegerType::I16, IntegerValue::Signed(value.into())),
        Value::I32(value) => (IntegerType::I32, IntegerValue::Signed(value.into())),
        Value::I64(value) => (IntegerType::I64, IntegerValue::Signed(value.into())),
        Value::I128(value) => (IntegerType::I128, IntegerValue::Signed(value)),
        Value::Isize(value) => (IntegerType::Isize, IntegerValue::Signed(value as i128)),
        Value::U8(value) => (IntegerType::U8, IntegerValue::Unsigned(value.into())),
        Value::U16(value) => (IntegerType::U16, IntegerValue::Unsigned(value.into())),
        Value::U32(value) => (IntegerType::U32, IntegerValue::Unsigned(value.into())),
        Value::U64(value) => (IntegerType::U64, IntegerValue::Unsigned(value.into())),
        Value::U128(value) => (IntegerType::U128, IntegerValue::Unsigned(value)),
        Value::Usize(value) => (IntegerType::Usize, IntegerValue::Unsigned(value as u128)),
        value => {
            return Err(format!(
                "`as` expects an integer, found {}",
                value.type_name()
            ));
        }
    };
    if !source.0.can_cast_losslessly_to(target) {
        return Err(format!(
            "cannot cast `{}` to `{target}` because the target type cannot represent the source type's full range",
            source.0
        ));
    }

    macro_rules! signed_target {
        ($kind:ty, $constructor:path) => {{
            let converted = match source.1 {
                IntegerValue::Signed(value) => <$kind>::try_from(value).map_err(|_| ()),
                IntegerValue::Unsigned(value) => <$kind>::try_from(value).map_err(|_| ()),
            };
            converted.map($constructor)
        }};
    }
    macro_rules! unsigned_target {
        ($kind:ty, $constructor:path) => {{
            let converted = match source.1 {
                IntegerValue::Signed(value) => <$kind>::try_from(value).map_err(|_| ()),
                IntegerValue::Unsigned(value) => <$kind>::try_from(value).map_err(|_| ()),
            };
            converted.map($constructor)
        }};
    }

    let converted = match target {
        IntegerType::I8 => signed_target!(i8, Value::I8),
        IntegerType::I16 => signed_target!(i16, Value::I16),
        IntegerType::I32 => signed_target!(i32, Value::I32),
        IntegerType::I64 => signed_target!(i64, Value::I64),
        IntegerType::I128 => signed_target!(i128, Value::I128),
        IntegerType::Isize => signed_target!(isize, Value::Isize),
        IntegerType::U8 => unsigned_target!(u8, Value::U8),
        IntegerType::U16 => unsigned_target!(u16, Value::U16),
        IntegerType::U32 => unsigned_target!(u32, Value::U32),
        IntegerType::U64 => unsigned_target!(u64, Value::U64),
        IntegerType::U128 => unsigned_target!(u128, Value::U128),
        IntegerType::Usize => unsigned_target!(usize, Value::Usize),
    };
    converted.map_err(|_| {
        format!(
            "cannot cast value from `{}` to `{target}` without losing information",
            source.0
        )
    })
}

pub(crate) fn execute_integer_intrinsic(
    id: rils_builtins::IntrinsicId,
    target: Option<IntegerType>,
    values: &[Value],
) -> Result<Value, String> {
    use rils_builtins::IntrinsicId::*;
    if integer_methods::handles(id) {
        return integer_methods::execute(id, values);
    }
    if id == IntegerTryFrom {
        let target =
            target.ok_or_else(|| "integer try_from is missing its target type".to_string())?;
        return try_cast_integer(values[0].clone(), target).map(|value| Value::Result {
            value: value
                .map(std::rc::Rc::new)
                .map_err(|message| std::rc::Rc::new(Value::String(message.into()))),
            ok_type: Some(Type::Integer(target)),
            error_type: Some(Type::String),
        });
    }
    match id {
        IntegerToF32 => integer_to_float(values[0].clone(), true),
        IntegerToF64 => integer_to_float(values[0].clone(), false),
        IntegerCheckedAdd
        | IntegerCheckedSub
        | IntegerCheckedMul
        | IntegerCheckedDiv
        | IntegerCheckedRem
        | IntegerWrappingAdd
        | IntegerWrappingSub
        | IntegerWrappingMul
        | IntegerSaturatingAdd
        | IntegerSaturatingSub
        | IntegerSaturatingMul
        | IntegerOverflowingAdd
        | IntegerOverflowingSub
        | IntegerOverflowingMul => integer_intrinsic_binary(id, &values[0], &values[1]),
        IntegerTryFrom => unreachable!(),
        _ => unreachable!("extended integer intrinsic was handled before dispatch"),
    }
}

fn integer_to_float(value: Value, f32_target: bool) -> Result<Value, String> {
    macro_rules! convert {
        ($value:expr) => {
            if f32_target {
                Value::F32($value as f32)
            } else {
                Value::F64($value as f64)
            }
        };
    }
    Ok(match value {
        Value::I8(v) => convert!(v),
        Value::I16(v) => convert!(v),
        Value::I32(v) => convert!(v),
        Value::I64(v) => convert!(v),
        Value::I128(v) => convert!(v),
        Value::Isize(v) => convert!(v),
        Value::U8(v) => convert!(v),
        Value::U16(v) => convert!(v),
        Value::U32(v) => convert!(v),
        Value::U64(v) => convert!(v),
        Value::U128(v) => convert!(v),
        Value::Usize(v) => convert!(v),
        value => {
            return Err(format!(
                "integer conversion expects an integer, found {}",
                value.type_name()
            ));
        }
    })
}

fn try_cast_integer(value: Value, target: IntegerType) -> Result<Result<Value, String>, String> {
    let source_name = value.type_name();
    enum Number {
        Signed(i128),
        Unsigned(u128),
    }
    let number = match value {
        Value::I8(v) => Number::Signed(v.into()),
        Value::I16(v) => Number::Signed(v.into()),
        Value::I32(v) => Number::Signed(v.into()),
        Value::I64(v) => Number::Signed(v.into()),
        Value::I128(v) => Number::Signed(v),
        Value::Isize(v) => Number::Signed(v as i128),
        Value::U8(v) => Number::Unsigned(v.into()),
        Value::U16(v) => Number::Unsigned(v.into()),
        Value::U32(v) => Number::Unsigned(v.into()),
        Value::U64(v) => Number::Unsigned(v.into()),
        Value::U128(v) => Number::Unsigned(v),
        Value::Usize(v) => Number::Unsigned(v as u128),
        value => {
            return Err(format!(
                "try_from expects an integer, found {}",
                value.type_name()
            ));
        }
    };
    macro_rules! target_value {
        ($ty:ty, $ctor:path) => {{
            match number {
                Number::Signed(v) => <$ty>::try_from(v).ok().map($ctor),
                Number::Unsigned(v) => <$ty>::try_from(v).ok().map($ctor),
            }
        }};
    }
    let result = match target {
        IntegerType::I8 => target_value!(i8, Value::I8),
        IntegerType::I16 => target_value!(i16, Value::I16),
        IntegerType::I32 => target_value!(i32, Value::I32),
        IntegerType::I64 => target_value!(i64, Value::I64),
        IntegerType::I128 => target_value!(i128, Value::I128),
        IntegerType::Isize => target_value!(isize, Value::Isize),
        IntegerType::U8 => target_value!(u8, Value::U8),
        IntegerType::U16 => target_value!(u16, Value::U16),
        IntegerType::U32 => target_value!(u32, Value::U32),
        IntegerType::U64 => target_value!(u64, Value::U64),
        IntegerType::U128 => target_value!(u128, Value::U128),
        IntegerType::Usize => target_value!(usize, Value::Usize),
    };
    Ok(result
        .ok_or_else(|| format!("value of type `{source_name}` is outside the `{target}` range")))
}

fn integer_intrinsic_binary(
    id: rils_builtins::IntrinsicId,
    left: &Value,
    right: &Value,
) -> Result<Value, String> {
    use rils_builtins::IntrinsicId::*;
    macro_rules! apply {
        ($a:expr, $b:expr, $ctor:path) => {{
            let checked = match id {
                IntegerCheckedAdd => $a.checked_add($b),
                IntegerCheckedSub => $a.checked_sub($b),
                IntegerCheckedMul => $a.checked_mul($b),
                IntegerCheckedDiv => $a.checked_div($b),
                IntegerCheckedRem => $a.checked_rem($b),
                _ => None,
            };
            if matches!(
                id,
                IntegerCheckedAdd
                    | IntegerCheckedSub
                    | IntegerCheckedMul
                    | IntegerCheckedDiv
                    | IntegerCheckedRem
            ) {
                return Ok(Value::Option {
                    value: checked.map(|v| std::rc::Rc::new($ctor(v))),
                    element_type: Some(Type::of_value(left).unwrap_or(Type::Unknown)),
                });
            }
            let direct = match id {
                IntegerWrappingAdd => $ctor($a.wrapping_add($b)),
                IntegerWrappingSub => $ctor($a.wrapping_sub($b)),
                IntegerWrappingMul => $ctor($a.wrapping_mul($b)),
                IntegerSaturatingAdd => $ctor($a.saturating_add($b)),
                IntegerSaturatingSub => $ctor($a.saturating_sub($b)),
                IntegerSaturatingMul => $ctor($a.saturating_mul($b)),
                IntegerOverflowingAdd => {
                    let (v, o) = $a.overflowing_add($b);
                    return tuple_value($ctor(v), o);
                }
                IntegerOverflowingSub => {
                    let (v, o) = $a.overflowing_sub($b);
                    return tuple_value($ctor(v), o);
                }
                IntegerOverflowingMul => {
                    let (v, o) = $a.overflowing_mul($b);
                    return tuple_value($ctor(v), o);
                }
                _ => unreachable!(),
            };
            Ok(direct)
        }};
    }
    match (left, right) {
        (Value::I8(a), Value::I8(b)) => apply!(*a, *b, Value::I8),
        (Value::I16(a), Value::I16(b)) => apply!(*a, *b, Value::I16),
        (Value::I32(a), Value::I32(b)) => apply!(*a, *b, Value::I32),
        (Value::I64(a), Value::I64(b)) => apply!(*a, *b, Value::I64),
        (Value::I128(a), Value::I128(b)) => apply!(*a, *b, Value::I128),
        (Value::Isize(a), Value::Isize(b)) => apply!(*a, *b, Value::Isize),
        (Value::U8(a), Value::U8(b)) => apply!(*a, *b, Value::U8),
        (Value::U16(a), Value::U16(b)) => apply!(*a, *b, Value::U16),
        (Value::U32(a), Value::U32(b)) => apply!(*a, *b, Value::U32),
        (Value::U64(a), Value::U64(b)) => apply!(*a, *b, Value::U64),
        (Value::U128(a), Value::U128(b)) => apply!(*a, *b, Value::U128),
        (Value::Usize(a), Value::Usize(b)) => apply!(*a, *b, Value::Usize),
        _ => Err(format!(
            "integer intrinsic operands must have the same type, found {} and {}",
            left.type_name(),
            right.type_name()
        )),
    }
}

fn tuple_value(value: Value, overflowed: bool) -> Result<Value, String> {
    let types = [Type::of_value(&value).unwrap_or(Type::Unknown), Type::Bool];
    Ok(Value::Tuple(std::rc::Rc::new(
        crate::value::SequenceValue {
            elements: std::cell::RefCell::new(vec![
                crate::value::FieldSlot {
                    value: Some(value),
                    type_annotation: types[0].clone(),
                    references: 0,
                },
                crate::value::FieldSlot {
                    value: Some(Value::Bool(overflowed)),
                    type_annotation: Type::Bool,
                    references: 0,
                },
            ]),
            element_type: std::cell::RefCell::new(None),
        },
    )))
}

macro_rules! integer_binary {
    ($left:expr, $operator:expr, $right:expr, $constructor:path) => {{
        use BinaryOp::*;
        let overflow = || "integer overflow".to_string();
        match $operator {
            Add => $left
                .checked_add($right)
                .map($constructor)
                .ok_or_else(overflow),
            Subtract => $left
                .checked_sub($right)
                .map($constructor)
                .ok_or_else(overflow),
            Multiply => $left
                .checked_mul($right)
                .map($constructor)
                .ok_or_else(overflow),
            Divide if $right == 0 => Err("division by zero".into()),
            Divide => $left
                .checked_div($right)
                .map($constructor)
                .ok_or_else(overflow),
            Remainder if $right == 0 => Err("division by zero".into()),
            Remainder => $left
                .checked_rem($right)
                .map($constructor)
                .ok_or_else(overflow),
            Greater => Ok(Value::Bool($left > $right)),
            GreaterEqual => Ok(Value::Bool($left >= $right)),
            Less => Ok(Value::Bool($left < $right)),
            LessEqual => Ok(Value::Bool($left <= $right)),
            Equal | NotEqual => unreachable!("equality is handled before numeric dispatch"),
        }
    }};
}

macro_rules! float_binary {
    ($left:expr, $operator:expr, $right:expr, $constructor:path) => {{
        use BinaryOp::*;
        match $operator {
            Add => Ok($constructor($left + $right)),
            Subtract => Ok($constructor($left - $right)),
            Multiply => Ok($constructor($left * $right)),
            Divide if $right == 0.0 => Err("division by zero".into()),
            Divide => Ok($constructor($left / $right)),
            Remainder if $right == 0.0 => Err("division by zero".into()),
            Remainder => Ok($constructor($left % $right)),
            Greater => Ok(Value::Bool($left > $right)),
            GreaterEqual => Ok(Value::Bool($left >= $right)),
            Less => Ok(Value::Bool($left < $right)),
            LessEqual => Ok(Value::Bool($left <= $right)),
            Equal | NotEqual => unreachable!("equality is handled before numeric dispatch"),
        }
    }};
}

pub(crate) fn negate(value: Value) -> Result<Value, String> {
    macro_rules! signed {
        ($value:expr, $constructor:path) => {
            $value
                .checked_neg()
                .map($constructor)
                .ok_or_else(|| "integer overflow".to_string())
        };
    }
    match value {
        Value::I8(value) => signed!(value, Value::I8),
        Value::I16(value) => signed!(value, Value::I16),
        Value::I32(value) => signed!(value, Value::I32),
        Value::I64(value) => signed!(value, Value::I64),
        Value::I128(value) => signed!(value, Value::I128),
        Value::Isize(value) => signed!(value, Value::Isize),
        Value::F32(value) => Ok(Value::F32(-value)),
        Value::F64(value) => Ok(Value::F64(-value)),
        value => Err(format!(
            "unary `-` expects a signed number, found {}",
            value.type_name()
        )),
    }
}

pub(crate) fn binary(left: Value, operator: BinaryOp, right: Value) -> Result<Value, String> {
    match (left, right) {
        (Value::I8(left), Value::I8(right)) => {
            integer_binary!(left, operator, right, Value::I8)
        }
        (Value::I16(left), Value::I16(right)) => {
            integer_binary!(left, operator, right, Value::I16)
        }
        (Value::I32(left), Value::I32(right)) => {
            integer_binary!(left, operator, right, Value::I32)
        }
        (Value::I64(left), Value::I64(right)) => {
            integer_binary!(left, operator, right, Value::I64)
        }
        (Value::I128(left), Value::I128(right)) => {
            integer_binary!(left, operator, right, Value::I128)
        }
        (Value::Isize(left), Value::Isize(right)) => {
            integer_binary!(left, operator, right, Value::Isize)
        }
        (Value::U8(left), Value::U8(right)) => {
            integer_binary!(left, operator, right, Value::U8)
        }
        (Value::U16(left), Value::U16(right)) => {
            integer_binary!(left, operator, right, Value::U16)
        }
        (Value::U32(left), Value::U32(right)) => {
            integer_binary!(left, operator, right, Value::U32)
        }
        (Value::U64(left), Value::U64(right)) => {
            integer_binary!(left, operator, right, Value::U64)
        }
        (Value::U128(left), Value::U128(right)) => {
            integer_binary!(left, operator, right, Value::U128)
        }
        (Value::Usize(left), Value::Usize(right)) => {
            integer_binary!(left, operator, right, Value::Usize)
        }
        (Value::F32(left), Value::F32(right)) => {
            float_binary!(left, operator, right, Value::F32)
        }
        (Value::F64(left), Value::F64(right)) => {
            float_binary!(left, operator, right, Value::F64)
        }
        (left, right) => Err(format!(
            "operator expects numbers of the same type, found {} and {}",
            left.type_name(),
            right.type_name()
        )),
    }
}
