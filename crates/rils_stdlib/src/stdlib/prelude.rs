//! Shared Rust types for definitions inside `rils_stdlib`.

pub use super::option::Option;
pub use super::result::Result;
pub use super::traits::{BitFlags, Default, Eq, Hash};
pub use super::traits::{Clone, Copy};
pub use crate::rils_type;

pub fn some<T>(value: T) -> Option<T> {
    Option::Some(value)
}

pub fn ok<T, E>(value: T) -> Result<T, E> {
    Result::Ok(value)
}

pub fn err<T, E>(error: E) -> Result<T, E> {
    Result::Err(error)
}
