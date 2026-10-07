//! Value conversion used by generated native function callback bridges.

use crate::Value;

pub trait NativeValue: Sized {
    fn from_value(value: &Value) -> Result<Self, String>;
    fn from_owned_value(value: Value) -> Result<Self, String> {
        Self::from_value(&value)
    }
    fn into_value(self) -> Value;
}

impl NativeValue for Value {
    fn from_value(value: &Value) -> Result<Self, String> {
        Ok(value.clone())
    }

    fn into_value(self) -> Value {
        self
    }

    fn from_owned_value(value: Value) -> Result<Self, String> {
        Ok(value)
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
);

macro_rules! native_scalar {
    ($( $rust:ty => $read:ident => $construct:ident; )*) => {
        $(
            impl NativeValue for $rust {
                fn from_value(value: &Value) -> Result<Self, String> {
                    value.$read().ok_or_else(|| {
                        format!("expected {}, found {}", stringify!($rust), value.type_name())
                    })
                }

                fn into_value(self) -> Value {
                    Value::$construct(self)
                }
            }
        )*
    };
}

native_scalar! {
    i8 => as_i8 => from_i8;
    i16 => as_i16 => from_i16;
    i32 => as_i32 => from_i32;
    i64 => as_i64 => from_i64;
    i128 => as_i128 => from_i128;
    isize => as_isize => from_isize;
    u8 => as_u8 => from_u8;
    u16 => as_u16 => from_u16;
    u32 => as_u32 => from_u32;
    u64 => as_u64 => from_u64;
    u128 => as_u128 => from_u128;
    usize => as_usize => from_usize;
    f32 => as_f32 => from_f32;
    f64 => as_f64 => from_f64;
}

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
