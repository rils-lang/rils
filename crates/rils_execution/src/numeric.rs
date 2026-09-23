#![allow(non_upper_case_globals)]

use crate::{IntegerType, Type, ast::BinaryOp, value::Value};

mod float_methods;
mod native;

pub fn integer_constant(target: IntegerType, constant: rils_builtins::IntegerConstantId) -> Value {
    native::integer::constant(target, constant)
        .expect("all integer constants have a native definition")
        .expect("native integer constant has no failure path")
}

pub fn float_constant(target: crate::FloatType, constant: rils_builtins::FloatConstantId) -> Value {
    float_methods::constant(target, constant)
}

pub fn cast_integer(value: Value, target: IntegerType) -> Result<Value, String> {
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

pub fn execute_integer_intrinsic(
    id: rils_builtins::BuiltinId,
    target: Option<IntegerType>,
    values: &[Value],
) -> Result<Value, String> {
    use rils_builtins::builtin_ids::*;
    if id == IntegerTryFrom && target.is_none() {
        return Err("integer try_from is missing its target type".into());
    }
    native::integer::call(id, target, values)
        .unwrap_or_else(|| Err("unknown integer intrinsic or receiver type".into()))
}

pub fn execute_intrinsic(
    id: rils_builtins::BuiltinId,
    target: Option<IntegerType>,
    values: &[Value],
) -> Result<Value, String> {
    if float_methods::handles(id) {
        if target.is_some() {
            return Err("float intrinsic cannot have an integer target".into());
        }
        return float_methods::execute(id, values);
    }
    execute_integer_intrinsic(id, target, values)
}

fn tuple_value(value: Value, overflowed: bool) -> Result<Value, String> {
    let types = [Type::of_value(&value).unwrap_or(Type::Unknown), Type::Bool];
    Ok(Value::Tuple(std::rc::Rc::new(
        crate::value::SequenceValue {
            active_iterators: std::cell::Cell::new(0),
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

pub fn negate(value: Value) -> Result<Value, String> {
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

pub fn binary(left: Value, operator: BinaryOp, right: Value) -> Result<Value, String> {
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

/// Executes an integer operation whose operand type has already been proven by the compiler.
///
/// The VM still validates dynamic values at its trust boundary, but this avoids dispatching
/// across every numeric representation for each integer instruction.
#[inline]
pub fn integer_binary_typed(
    left: Value,
    integer: IntegerType,
    operator: BinaryOp,
    right: Value,
) -> Result<Value, String> {
    if matches!(operator, BinaryOp::Equal | BinaryOp::NotEqual) {
        let equal = left == right;
        return Ok(Value::Bool(if operator == BinaryOp::Equal {
            equal
        } else {
            !equal
        }));
    }

    macro_rules! typed {
        ($value:ident, $constructor:path) => {
            match (left, right) {
                (Value::$value(left), Value::$value(right)) => {
                    integer_binary!(left, operator, right, $constructor)
                }
                (left, right) => Err(format!(
                    "typed integer operator expects {integer}, found {} and {}",
                    left.type_name(),
                    right.type_name()
                )),
            }
        };
    }

    match integer {
        IntegerType::I8 => typed!(I8, Value::I8),
        IntegerType::I16 => typed!(I16, Value::I16),
        IntegerType::I32 => typed!(I32, Value::I32),
        IntegerType::I64 => typed!(I64, Value::I64),
        IntegerType::I128 => typed!(I128, Value::I128),
        IntegerType::Isize => typed!(Isize, Value::Isize),
        IntegerType::U8 => typed!(U8, Value::U8),
        IntegerType::U16 => typed!(U16, Value::U16),
        IntegerType::U32 => typed!(U32, Value::U32),
        IntegerType::U64 => typed!(U64, Value::U64),
        IntegerType::U128 => typed!(U128, Value::U128),
        IntegerType::Usize => typed!(Usize, Value::Usize),
    }
}
