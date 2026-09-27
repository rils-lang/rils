//! Constructors and temporary adapters for scalar types being migrated.

use crate::{IntegerType, Type, Value, value::NativeObject};
use rils_stdlib::stdlib::integer::Number;

macro_rules! migrated_integers {
    ($( $constructor:ident, $payload:ident, $rust:ty, $variant:ident, $descriptor:ident; )*) => {
        $(
            pub fn $constructor(value: $rust) -> Value {
                Value::Native(
                    NativeObject::new(super::native::integer::$descriptor(), Number(value))
                        .expect("generated descriptor matches its Rust payload"),
                )
            }

            pub fn $payload(value: &Value) -> Option<$rust> {
                match value {
                    Value::$variant(value) => Some(*value),
                    Value::Native(object)
                        if object.descriptor().rils_type() == &Type::Integer(IntegerType::$variant) =>
                    {
                        object.with::<Number<$rust>, _>(|number| number.0).ok()
                    }
                    _ => None,
                }
            }
        )*

        pub(super) fn lower_migrated_integer(value: Value) -> Value {
            match value {
                Value::Native(object) => {
                    $(
                        if object.descriptor().rils_type() == &Type::Integer(IntegerType::$variant) {
                            return match object.with::<Number<$rust>, _>(|number| number.0) {
                                Ok(value) => Value::$variant(value),
                                Err(_) => Value::Native(object),
                            };
                        }
                    )*
                    Value::Native(object)
                }
                value => value,
            }
        }

        pub(super) fn lift_migrated_integer(value: Value) -> Value {
            match value {
                $(Value::$variant(value) => $constructor(value),)*
                value => value,
            }
        }
    };
}

migrated_integers! {
    native_i32, i32_payload, i32, I32, descriptor_i32;
    native_usize, usize_payload, usize, Usize, descriptor_usize;
}

pub fn native_i8(value: i8) -> Value {
    Value::Native(
        NativeObject::new(super::native::integer::descriptor_i8(), Number(value))
            .expect("generated descriptor matches its Rust payload"),
    )
}

pub fn i8_payload(value: &Value) -> Option<i8> {
    let Value::Native(object) = value else {
        return None;
    };
    (object.descriptor().rils_type() == &Type::Integer(IntegerType::I8))
        .then(|| object.with::<Number<i8>, _>(|number| number.0).ok())
        .flatten()
}
