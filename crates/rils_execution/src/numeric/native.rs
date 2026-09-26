//! Native integer methods generated from their Rust standard-library definitions.

use std::rc::Rc;

use crate::{Type, Value};
use rils_stdlib::stdlib::{
    float::Number as FloatNumber,
    integer::{Integer, Number},
    option::Option,
    result::Result,
};

pub(super) trait NativeInput: Sized {
    fn from_value(value: &Value) -> std::result::Result<Self, String>;
}

pub(super) trait NativeOutput {
    fn into_value(self) -> std::result::Result<Value, String>;
}

impl NativeOutput for bool {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(Value::Bool(self))
    }
}

macro_rules! float_bridge {
    ($($primitive:ty => $variant:ident),* $(,)?) => {$ (
        impl NativeInput for FloatNumber<$primitive> {
            fn from_value(value: &Value) -> std::result::Result<Self, String> {
                match value {
                    Value::$variant(value) => Ok(Self(*value)),
                    value => Err(format!("expected {}, found {}", stringify!($primitive), value.type_name())),
                }
            }
        }
        impl NativeOutput for FloatNumber<$primitive> {
            fn into_value(self) -> std::result::Result<Value, String> {
                Ok(Value::$variant(self.0))
            }
        }
    )* };
}

float_bridge!(f32 => F32, f64 => F64);

impl NativeInput for u32 {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        match value {
            Value::U32(value) => Ok(*value),
            value => Err(format!("expected u32, found {}", value.type_name())),
        }
    }
}

impl NativeInput for Integer {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        if let Value::Native(_) = value {
            if let Some(inner) = super::i8_payload(value) {
                return Ok(Self::Signed(inner.into(), "i8"));
            }
            if let Some(inner) = super::i32_payload(value) {
                return Ok(Self::Signed(inner.into(), "i32"));
            }
            if let Some(inner) = super::usize_payload(value) {
                return Ok(Self::Unsigned(inner as u128, "usize"));
            }
        }
        macro_rules! signed {
            ($value:expr, $name:literal) => {
                Ok(Self::Signed((*$value).into(), $name))
            };
        }
        macro_rules! unsigned {
            ($value:expr, $name:literal) => {
                Ok(Self::Unsigned((*$value).into(), $name))
            };
        }
        match value {
            Value::I8(value) => signed!(value, "i8"),
            Value::I16(value) => signed!(value, "i16"),
            Value::I32(value) => signed!(value, "i32"),
            Value::I64(value) => signed!(value, "i64"),
            Value::I128(value) => signed!(value, "i128"),
            Value::Isize(value) => Ok(Self::Signed(*value as i128, "isize")),
            Value::U8(value) => unsigned!(value, "u8"),
            Value::U16(value) => unsigned!(value, "u16"),
            Value::U32(value) => unsigned!(value, "u32"),
            Value::U64(value) => unsigned!(value, "u64"),
            Value::U128(value) => unsigned!(value, "u128"),
            Value::Usize(value) => Ok(Self::Unsigned(*value as u128, "usize")),
            value => Err(format!(
                "try_from expects an integer, found {}",
                value.type_name()
            )),
        }
    }
}

impl NativeOutput for u32 {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(Value::U32(self))
    }
}
impl NativeOutput for f32 {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(Value::F32(self))
    }
}
impl NativeOutput for f64 {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(Value::F64(self))
    }
}
macro_rules! integer_bridge {
    ($($primitive:ty => $variant:ident => $constructor:path),* $(,)?) => {$(
        impl NativeInput for Number<$primitive> {
            fn from_value(value: &Value) -> std::result::Result<Self, String> {
                match value {
                    Value::$variant(value) => Ok(Self(*value)),
                    Value::Native(object) if object.descriptor().rils_type() == &Type::Integer(crate::IntegerType::$variant) => {
                        object.with::<Self, _>(|value| *value)
                    }
                    value => Err(format!("expected {}, found {}", stringify!($variant).to_ascii_lowercase(), value.type_name())),
                }
            }
        }
        impl NativeOutput for Number<$primitive> {
            fn into_value(self) -> std::result::Result<Value, String> {
                Ok($constructor(self.0))
            }
        }
        impl NativeOutput for Option<Number<$primitive>> {
            fn into_value(self) -> std::result::Result<Value, String> {
                Ok(Value::Option {
                    value: match self {
                        Option::Some(value) => Some(Rc::new($constructor(value.0))),
                        Option::None => None,
                    },
                    element_type: Some(Type::Integer(crate::IntegerType::$variant)),
                })
            }
        }
        impl NativeOutput for Result<Number<$primitive>, String> {
            fn into_value(self) -> std::result::Result<Value, String> {
                Ok(Value::Result {
                    value: match self {
                        Result::Ok(value) => Ok(Rc::new($constructor(value.0))),
                        Result::Err(message) => Err(Rc::new(Value::String(message.into()))),
                    },
                    ok_type: Some(Type::Integer(crate::IntegerType::$variant)),
                    error_type: Some(Type::String),
                })
            }
        }
        impl NativeOutput for (Number<$primitive>, bool) {
            fn into_value(self) -> std::result::Result<Value, String> {
                super::tuple_value($constructor(self.0.0), self.1)
            }
        }
    )*};
}

integer_bridge!(
    i8 => I8 => super::native_i8,
    i16 => I16 => Value::I16, i32 => I32 => super::native_i32,
    i64 => I64 => Value::I64, i128 => I128 => Value::I128,
    isize => Isize => Value::Isize, u8 => U8 => Value::U8,
    u16 => U16 => Value::U16, u32 => U32 => Value::U32,
    u64 => U64 => Value::U64, u128 => U128 => Value::U128,
    usize => Usize => super::native_usize,
);

pub(super) mod integer {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::integer_definition!(decl_rils_native);
}

pub(super) mod float {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::float_definition!(decl_rils_native);
}
