mod catalog;
mod numeric;

pub use catalog::*;
pub use numeric::*;

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
