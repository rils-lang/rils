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

macro_rules! native_scalar {
    ($( $rust:ty => $construct:ident; )*) => {
        $(
            impl NativeValue for $rust {
                fn from_value(value: &Value) -> Result<Self, String> {
                    value.get_cloned::<$rust>().ok().ok_or_else(|| {
                        format!("expected {}, found {}", stringify!($rust), value.type_name())
                    })
                }

                fn into_value(self) -> Value {
                    Value::from(crate::value::$construct(self))
                }
            }
        )*
    };
}

native_scalar! {
    bool => native_bool;
    char => native_char;
}

impl NativeValue for () {
    fn from_value(value: &Value) -> Result<Self, String> {
        if value.is_unit() {
            Ok(())
        } else {
            Err(format!("expected (), found {}", value.type_name()))
        }
    }

    fn into_value(self) -> Value {
        Value::from(crate::value::native_unit())
    }
}
