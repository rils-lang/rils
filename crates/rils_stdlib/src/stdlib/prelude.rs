//! Shared Rust types for definitions inside `rils_stdlib`.

pub use super::option::Option;
pub use super::result::Result;

pub fn some<T>(value: T) -> Option<T> {
    Option::Some(value)
}

pub fn ok<T, E>(value: T) -> Result<T, E> {
    Result::Ok(value)
}

pub fn err<T, E>(error: E) -> Result<T, E> {
    Result::Err(error)
}
