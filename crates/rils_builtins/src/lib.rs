#[macro_use]
mod catalog;
pub mod native_definitions;
mod numeric;

pub use catalog::*;
pub use numeric::*;
pub use rils_stdlib::NATIVE_DERIVES;
pub use rils_stdlib::stdlib::iterator::BLANKET_TRAIT_IMPLS;
pub use rils_stdlib::stdlib::ops::callable_trait_kind;

#[doc(hidden)]
pub use rils_builtins_macros::type_pattern as __type_pattern;

/// Converts Rust-style type syntax into a static [`TypePattern`].
#[macro_export]
macro_rules! type_pattern {
    ($($ty:tt)+) => {{
        use $crate::TypePattern;
        $crate::__type_pattern!($($ty)+)
    }};
}
