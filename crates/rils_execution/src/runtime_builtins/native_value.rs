//! Value conversion used by generated native function callback bridges.

use crate::Value;

pub trait NativeValue: Sized {
    fn from_value(value: &Value) -> Result<Self, String>;
    fn into_value(self) -> Value;
}

impl NativeValue for Value {
    fn from_value(value: &Value) -> Result<Self, String> {
        Ok(value.clone())
    }

    fn into_value(self) -> Value {
        self
    }
}

macro_rules! scalar {
    ($($rust:ty => $variant:ident),* $(,)?) => {
        $(
            impl NativeValue for $rust {
                fn from_value(value: &Value) -> Result<Self, String> {
                    match value {
                        Value::$variant(value) => Ok(*value),
                        value => Err(format!("expected {}, found {}", stringify!($rust), value.type_name())),
                    }
                }

                fn into_value(self) -> Value {
                    Value::$variant(self)
                }
            }
        )*
    };
}

scalar!(
    bool => Bool,
    char => Char,
    i8 => I8,
    i16 => I16,
    i32 => I32,
    i64 => I64,
    i128 => I128,
    isize => Isize,
    u8 => U8,
    u16 => U16,
    u32 => U32,
    u64 => U64,
    u128 => U128,
    usize => Usize,
    f32 => F32,
    f64 => F64,
);

impl NativeValue for () {
    fn from_value(value: &Value) -> Result<Self, String> {
        match value {
            Value::Unit => Ok(()),
            value => Err(format!("expected (), found {}", value.type_name())),
        }
    }

    fn into_value(self) -> Value {
        Value::Unit
    }
}
