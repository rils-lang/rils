use crate::{ast::BinaryOp, value::Value};

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
