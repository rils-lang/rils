//! Constructors and temporary adapters for scalar types being migrated.

use crate::{IntegerType, Type, Value, value::NativeObject};
use rils_stdlib::stdlib::{float::Number as FloatNumber, integer::Number};

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

        pub(crate) fn lower_migrated_integer(value: Value) -> Value {
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
    native_i16, i16_payload, i16, I16, descriptor_i16;
    native_i64, i64_payload, i64, I64, descriptor_i64;
    native_i128, i128_payload, i128, I128, descriptor_i128;
    native_isize, isize_payload, isize, Isize, descriptor_isize;
    native_u8, u8_payload, u8, U8, descriptor_u8;
    native_u16, u16_payload, u16, U16, descriptor_u16;
    native_u32, u32_payload, u32, U32, descriptor_u32;
    native_u64, u64_payload, u64, U64, descriptor_u64;
    native_u128, u128_payload, u128, U128, descriptor_u128;
    native_usize, usize_payload, usize, Usize, descriptor_usize;
}

pub fn native_i32(value: i32) -> Value {
    Value::Native(
        NativeObject::new(super::native::integer::descriptor_i32(), Number(value))
            .expect("generated descriptor matches its Rust payload"),
    )
}

pub fn i32_payload(value: &Value) -> Option<i32> {
    let Value::Native(object) = value else {
        return None;
    };
    (object.descriptor().rils_type() == &Type::Integer(IntegerType::I32))
        .then(|| object.with::<Number<i32>, _>(|number| number.0).ok())
        .flatten()
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

macro_rules! native_floats {
    ($( $constructor:ident, $payload:ident, $rust:ty, $variant:ident, $descriptor:ident; )*) => {
        $(
            pub fn $constructor(value: $rust) -> Value {
                Value::Native(
                    NativeObject::new(super::native::float::$descriptor(), FloatNumber(value))
                        .expect("generated descriptor matches its Rust payload"),
                )
            }

            pub fn $payload(value: &Value) -> Option<$rust> {
                match value {
                    Value::$variant(value) => Some(*value),
                    Value::Native(object)
                        if object.descriptor().rils_type() == &Type::Float(crate::FloatType::$variant) =>
                    {
                        object.with::<FloatNumber<$rust>, _>(|number| number.0).ok()
                    }
                    _ => None,
                }
            }
        )*
    };
}

native_floats! {
    native_f32, f32_payload, f32, F32, descriptor_f32;
    native_f64, f64_payload, f64, F64, descriptor_f64;
}
