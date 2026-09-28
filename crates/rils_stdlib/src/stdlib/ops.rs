//! Callable contracts shared by Rils functions and native adapters.

use rils_stdlib_macros::decl_rils;

/// Invocation capability exported with the callable traits below.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallableTraitKind {
    Once,
    Mut,
    Shared,
}

impl CallableTraitKind {
    pub fn satisfies(self, required: Self) -> bool {
        self >= required
    }
}

/// Resolves a language callable trait to the capability it requires.
pub fn callable_trait_kind(name: &str) -> Option<CallableTraitKind> {
    match name {
        "FnOnce" | "core::ops::FnOnce" => Some(CallableTraitKind::Once),
        "FnMut" | "core::ops::FnMut" => Some(CallableTraitKind::Mut),
        "Fn" | "core::ops::Fn" => Some(CallableTraitKind::Shared),
        _ => None,
    }
}

// Only the native adapter may implement these supertraits. Rils function
// values satisfy the language traits through the runtime's callable check;
// stable Rust cannot blanket-implement a custom tuple-argument trait for an
// arbitrary `std::ops::Fn*` signature without enumerating tuple arities.
mod sealed {
    pub trait FnOnce<Args, Output> {}
    pub trait FnMut<Args, Output>: FnOnce<Args, Output> {}
    pub trait Fn<Args, Output>: FnMut<Args, Output> {}
}

#[decl_rils(core::ops)]
mod native {
    /// Applies a stateful callback twice to a value.
    #[rils_fn]
    pub fn apply_twice<T, F>(value: T, mut callback: F) -> T
    where
        F: std::ops::FnMut(T) -> T,
    {
        let intermediate = callback(value);
        callback(intermediate)
    }

    /// Combines two values with a two-argument callback.
    #[rils_fn]
    pub fn combine<T, U, V, F>(left: T, right: U, callback: F) -> V
    where
        F: std::ops::FnOnce(T, U) -> V,
    {
        callback(left, right)
    }

    /// Passes a value through two callbacks in order.
    #[rils_fn]
    pub fn chain<T, U, V, F, G>(first: F, value: T, second: G) -> V
    where
        F: std::ops::FnOnce(T) -> U,
        G: std::ops::FnOnce(U) -> V,
    {
        second(first(value))
    }

    /// A callable that accepts `Args` and returns `Output` at least once.
    #[rils_trait]
    pub trait FnOnce<Args, Output>: super::sealed::FnOnce<Args, Output> {}

    /// A callable that may update captured state on every invocation.
    #[rils_trait]
    pub trait FnMut<Args, Output>:
        super::sealed::FnMut<Args, Output> + FnOnce<Args, Output>
    {
    }

    /// A callable that accepts `Args` and returns `Output`.
    #[rils_trait]
    pub trait Fn<Args, Output>: super::sealed::Fn<Args, Output> + FnMut<Args, Output> {}
}

impl<T, Args, Output> native::FnOnce<Args, Output> for T where T: sealed::FnOnce<Args, Output> {}
impl<T, Args, Output> native::FnMut<Args, Output> for T where T: sealed::FnMut<Args, Output> {}
impl<T, Args, Output> native::Fn<Args, Output> for T where T: sealed::Fn<Args, Output> {}

pub use native::Fn as RilsFn;
pub use native::FnMut as RilsFnMut;
/// Rust callers cannot implement this exported trait for arbitrary types:
/// its required Rust-side supertrait is private to `rils_stdlib`.
///
/// ```compile_fail
/// use rils_stdlib::stdlib::ops::RilsFnOnce;
/// struct Forged;
/// impl RilsFnOnce<(), ()> for Forged {}
/// ```
pub use native::FnOnce as RilsFnOnce;
#[doc(hidden)]
pub use native::{__rils_try_apply_twice, __rils_try_chain, __rils_try_combine};
pub use native::{apply_twice, chain, combine};
