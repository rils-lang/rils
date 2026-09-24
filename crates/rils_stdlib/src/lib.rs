//! Shared Rust source definitions for Rils standard-library APIs.

pub mod stdlib;

/// Marks a field type for Rils declarations while preserving its Rust type.
#[macro_export]
macro_rules! rils_type {
    ($ty:ty) => {
        $ty
    };
}

pub const NATIVE_DERIVES: &[rils_syntax::derive::NativeDeriveDefinition] =
    rils_syntax::rils_derive_registry!("src/stdlib");
