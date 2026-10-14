//! Shared conversion of declaration-generated native return values.

use crate::{
    Type, Value,
    value::{OwnedIteratorValue, storage::TypedStorageContext},
};
use rils_stdlib::stdlib::{
    iterator::Iter, option::Option as NativeOption, result::Result as NativeResult,
};

pub(crate) trait NativeOutput: Sized + 'static {
    fn output_type() -> Type;
    fn into_value(self) -> Result<Value, String>;
}

macro_rules! scalar {
    ($($rust:ty => $ty:expr, $constructor:ident;)*) => {$(
        impl NativeOutput for $rust {
            fn output_type() -> Type { $ty }
            fn into_value(self) -> Result<Value, String> { Ok(crate::value::$constructor(self).into()) }
        }
    )*};
}
scalar! {
    bool => Type::Bool, native_bool;
    char => Type::Char, native_char;
}

impl<T: NativeOutput> NativeOutput for NativeOption<T> {
    fn output_type() -> Type {
        Type::Option(Box::new(T::output_type()))
    }
    fn into_value(self) -> Result<Value, String> {
        let item = match self {
            Self::Some(value) => Some(value.into_value()?),
            Self::None => None,
        };
        TypedStorageContext::new(&[], &[]).construct_option(&Self::output_type(), item)
    }
}

impl<T: NativeOutput, E: NativeOutput> NativeOutput for NativeResult<T, E> {
    fn output_type() -> Type {
        Type::Result(Box::new(T::output_type()), Box::new(E::output_type()))
    }
    fn into_value(self) -> Result<Value, String> {
        let branch = match self {
            Self::Ok(value) => Ok(value.into_value()?),
            Self::Err(error) => Err(error.into_value()?),
        };
        TypedStorageContext::new(&[], &[]).construct_result(&Self::output_type(), branch)
    }
}

impl<A: NativeOutput, B: NativeOutput> NativeOutput for (A, B) {
    fn output_type() -> Type {
        Type::Tuple(vec![A::output_type(), B::output_type()])
    }
    fn into_value(self) -> Result<Value, String> {
        TypedStorageContext::new(&[], &[]).construct_tuple(
            &Self::output_type(),
            vec![self.0.into_value()?, self.1.into_value()?],
        )
    }
}

impl<T: NativeOutput> NativeOutput for Iter<T> {
    fn output_type() -> Type {
        Type::Named {
            name: "Iter".into(),
            arguments: vec![T::output_type()],
        }
    }
    fn into_value(self) -> Result<Value, String> {
        Ok(Value::from_owned_iterator(
            OwnedIteratorValue::from_generator(self.map(T::into_value), T::output_type())?
                .with_iterator_type(Self::output_type()),
        ))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/native_output.rs"]
mod tests;
