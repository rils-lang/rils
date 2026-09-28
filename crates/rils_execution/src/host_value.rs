//! Host-facing ownership choices for an execution result.

use std::any::Any;

use crate::Value;
use rils_stdlib::stdlib::{float::Number as FloatNumber, integer::Number as IntegerNumber};

/// Maps a Rust host type to the exact Rust payload stored for its Rils type.
pub trait RilsHostType: Sized + 'static {
    type Native: 'static;

    fn from_native(value: Self::Native) -> Self;
    fn as_native_ref(value: &Self::Native) -> &Self;
}

macro_rules! direct_host_type {
    ($($ty:ty),* $(,)?) => {$(
        impl RilsHostType for $ty {
            type Native = Self;

            fn from_native(value: Self::Native) -> Self { value }
            fn as_native_ref(value: &Self::Native) -> &Self { value }
        }
    )*};
}

direct_host_type!((), bool, char);

macro_rules! wrapped_host_type {
    ($wrapper:ident; $($ty:ty),* $(,)?) => {$(
        impl RilsHostType for $ty {
            type Native = $wrapper<$ty>;

            fn from_native(value: Self::Native) -> Self { value.0 }
            fn as_native_ref(value: &Self::Native) -> &Self { &value.0 }
        }
    )*};
}

wrapped_host_type!(IntegerNumber; i8, i16, i32, i64, i128, isize, u8, u16, u32, u64, u128, usize);
wrapped_host_type!(FloatNumber; f32, f64);

impl RilsHostType for String {
    type Native = rils_stdlib::stdlib::string::String;

    fn from_native(value: Self::Native) -> Self {
        value.into()
    }

    fn as_native_ref(value: &Self::Native) -> &Self {
        value.as_ref()
    }
}

/// A script result whose payload stays managed by the Rils runtime.
///
/// Access to a Rust value is explicit: borrow during a callback, clone it,
/// or consume an owned result. Only payloads whose native Rust layout is `T`
/// can currently be exposed as `&T`.
pub struct RilsValue {
    value: Value,
}

impl RilsValue {
    pub fn new(value: Value) -> Self {
        Self { value }
    }

    pub fn with_ref<T: RilsHostType, R>(
        &self,
        callback: impl FnOnce(&T) -> R,
    ) -> Result<R, String> {
        with_rust_value::<T::Native, _>(&self.value, |value| callback(T::as_native_ref(value)))
    }

    pub fn get_cloned<T: RilsHostType + Clone>(&self) -> Result<T, String> {
        self.with_ref(Clone::clone)
    }

    pub fn into_owned<T: RilsHostType>(self) -> Result<T, Box<(Self, String)>> {
        match self.value {
            Value::Native(object) => {
                object
                    .into_rust::<T::Native>()
                    .map(T::from_native)
                    .map_err(|error| {
                        let (object, message) = *error;
                        Box::new((Self::new(Value::Native(object)), message))
                    })
            }
            Value::Dynamic(object) => {
                object
                    .into_rust::<T::Native>()
                    .map(T::from_native)
                    .map_err(|error| {
                        let (object, message) = *error;
                        Box::new((Self::new(Value::Dynamic(object)), message))
                    })
            }
            value => move_legacy::<T::Native>(value)
                .map(T::from_native)
                .map_err(|error| {
                    let (value, message) = *error;
                    Box::new((Self::new(value), message))
                }),
        }
    }
}

pub(crate) fn with_rust_value<T: 'static, R>(
    value: &Value,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    match value {
        Value::Native(object) => object.with(callback),
        Value::Dynamic(object) => object.with(|value| value.with(callback))?,
        Value::Reference(reference) => reference.with_rust(callback),
        Value::Unit => borrow_legacy(&(), callback),
        Value::Bool(value) => borrow_legacy(value, callback),
        Value::I16(value) => borrow_legacy(value, callback),
        Value::I64(value) => borrow_legacy(value, callback),
        Value::I128(value) => borrow_legacy(value, callback),
        Value::Isize(value) => borrow_legacy(value, callback),
        Value::U8(value) => borrow_legacy(value, callback),
        Value::U16(value) => borrow_legacy(value, callback),
        Value::U32(value) => borrow_legacy(value, callback),
        Value::U64(value) => borrow_legacy(value, callback),
        Value::U128(value) => borrow_legacy(value, callback),
        Value::Usize(value) => borrow_legacy(value, callback),
        Value::F32(value) => borrow_legacy(value, callback),
        Value::F64(value) => borrow_legacy(value, callback),
        Value::Char(value) => borrow_legacy(value, callback),
        _ => Err(format!("{} has no Rust borrow view", value.type_name())),
    }
}

fn borrow_legacy<T: 'static, U: 'static, R>(
    value: &U,
    callback: impl FnOnce(&T) -> R,
) -> Result<R, String> {
    (value as &dyn Any)
        .downcast_ref::<T>()
        .map(callback)
        .ok_or_else(|| format!("value is not {}", std::any::type_name::<T>()))
}

macro_rules! move_scalar {
    ($item:ident, $variant:ident, $ty:ty) => {
        (Box::new($item) as Box<dyn Any>)
            .downcast::<$ty>()
            .map(|item| *item)
            .map_err(|item| {
                Box::new((
                    Value::$variant(*item.downcast().expect("original variant type")),
                    format!("value is not {}", std::any::type_name::<$ty>()),
                ))
            })
    };
}

fn move_legacy<T: 'static>(value: Value) -> Result<T, Box<(Value, String)>> {
    // Match once so that a failed downcast can put the original payload back.
    match value {
        Value::Unit => (Box::new(()) as Box<dyn Any>)
            .downcast::<T>()
            .map(|item| *item)
            .map_err(|_| {
                Box::new((
                    Value::Unit,
                    format!("value is not {}", std::any::type_name::<T>()),
                ))
            }),
        Value::Bool(item) => move_scalar!(item, Bool, T),
        Value::I16(item) => move_scalar!(item, I16, T),
        Value::I64(item) => move_scalar!(item, I64, T),
        Value::I128(item) => move_scalar!(item, I128, T),
        Value::Isize(item) => move_scalar!(item, Isize, T),
        Value::U8(item) => move_scalar!(item, U8, T),
        Value::U16(item) => move_scalar!(item, U16, T),
        Value::U32(item) => move_scalar!(item, U32, T),
        Value::U64(item) => move_scalar!(item, U64, T),
        Value::U128(item) => move_scalar!(item, U128, T),
        Value::Usize(item) => move_scalar!(item, Usize, T),
        Value::F32(item) => move_scalar!(item, F32, T),
        Value::F64(item) => move_scalar!(item, F64, T),
        Value::Char(item) => move_scalar!(item, Char, T),
        value => Err(Box::new((
            value,
            "result is not an owned native Rust value".into(),
        ))),
    }
}
