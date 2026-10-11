//! Native integer methods generated from their Rust standard-library definitions.

use crate::{Type, Value, value::storage::TypedStorageContext};
use rils_stdlib::stdlib::{
    float::Number as FloatNumber,
    integer::{Integer, Number},
    option::Option,
    result::Result,
};

pub(crate) trait NativeInput: Sized {
    fn from_value(value: &Value) -> std::result::Result<Self, String>;
}

pub(crate) trait NativeOutput {
    fn into_value(self) -> std::result::Result<Value, String>;
}

impl NativeOutput for bool {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(Value::Bool(self))
    }
}

macro_rules! float_bridge {
    ($($primitive:ty => $variant:ident => $payload:path => $constructor:path),* $(,)?) => {$ (
        impl NativeInput for FloatNumber<$primitive> {
            fn from_value(value: &Value) -> std::result::Result<Self, String> {
                $payload(value)
                    .map(Self)
                    .ok_or_else(|| format!("expected {}, found {}", stringify!($primitive), value.type_name()))
            }
        }
        impl NativeOutput for FloatNumber<$primitive> {
            fn into_value(self) -> std::result::Result<Value, String> {
                Ok($constructor(self.0))
            }
        }
    )* };
}

float_bridge!(
    f32 => F32 => super::f32_payload => super::native_f32,
    f64 => F64 => super::f64_payload => super::native_f64,
);

impl NativeInput for u32 {
    fn from_value(value: &Value) -> std::result::Result<Self, String> {
        super::u32_payload(value)
            .ok_or_else(|| format!("expected u32, found {}", value.type_name()))
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
        }
        let lowered = super::lower_migrated_integer(value.clone());
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
        match &lowered {
            Value::I16(value) => signed!(value, "i16"),
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
        Ok(super::native_u32(self))
    }
}
impl NativeOutput for f32 {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(super::native_f32(self))
    }
}
impl NativeOutput for f64 {
    fn into_value(self) -> std::result::Result<Value, String> {
        Ok(super::native_f64(self))
    }
}
macro_rules! integer_input {
    (I8, $value:expr) => {
        super::i8_payload($value)
    };
    (I32, $value:expr) => {
        super::i32_payload($value)
    };
    ($variant:ident, $value:expr) => {
        match $value {
            Value::$variant(value) => Some(*value),
            _ => None,
        }
    };
}

macro_rules! integer_bridge {
    ($($primitive:ty => $variant:ident => $constructor:path),* $(,)?) => {$(
        impl NativeInput for Number<$primitive> {
            fn from_value(value: &Value) -> std::result::Result<Self, String> {
                if let Some(inner) = integer_input!($variant, value) {
                    Ok(Self(inner))
                } else if let Value::Native(object) = value
                    && object.descriptor().rils_type() == &Type::Integer(crate::IntegerType::$variant)
                {
                    object.with::<Self, _>(|value| *value)
                } else {
                    Err(format!("expected {}, found {}", stringify!($variant).to_ascii_lowercase(), value.type_name()))
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
                let item = match self {
                    Option::Some(value) => Some($constructor(value.0)),
                    Option::None => None,
                };
                TypedStorageContext::new(&[], &[]).construct_option(
                    &Type::Option(Box::new(Type::Integer(crate::IntegerType::$variant))),
                    item,
                )
            }
        }
        impl NativeOutput for Result<Number<$primitive>, String> {
            fn into_value(self) -> std::result::Result<Value, String> {
                let branch = match self {
                    Result::Ok(value) => Ok($constructor(value.0)),
                    Result::Err(message) => Err(crate::value::native_string(message)),
                };
                TypedStorageContext::new(&[], &[]).construct_result(
                    &Type::Result(Box::new(Type::Integer(crate::IntegerType::$variant)), Box::new(Type::String)),
                    branch,
                )
            }
        }
        impl NativeOutput for (Number<$primitive>, bool) {
            fn into_value(self) -> std::result::Result<Value, String> {
                TypedStorageContext::new(&[], &[]).construct_tuple(
                    &Type::Tuple(vec![Type::Integer(crate::IntegerType::$variant), Type::Bool]),
                    vec![$constructor(self.0.0), Value::Bool(self.1)],
                )
            }
        }
    )*};
}

integer_bridge!(
    i8 => I8 => super::native_i8,
    i16 => I16 => super::native_i16, i32 => I32 => super::native_i32,
    i64 => I64 => super::native_i64, i128 => I128 => super::native_i128,
    isize => Isize => super::native_isize, u8 => U8 => super::native_u8,
    u16 => U16 => super::native_u16, u32 => U32 => super::native_u32,
    u64 => U64 => super::native_u64, u128 => U128 => super::native_u128,
    usize => Usize => super::native_usize,
);

pub(super) mod integer {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::integer_definition!(decl_rils_native);
}

pub(super) mod float {
    use rils_stdlib_macros::decl_rils_native;

    rils_stdlib::float_definition!(decl_rils_native);
}
