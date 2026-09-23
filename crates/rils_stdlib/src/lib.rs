//! Shared Rust source definitions for Rils standard-library APIs.

pub mod stdlib;

pub const NATIVE_DERIVES: &[rils_syntax::derive::NativeDeriveDefinition] =
    rils_syntax::rils_derive_registry!("src/stdlib");
