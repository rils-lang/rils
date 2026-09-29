//! Shared ownership handles.

use rils_stdlib_macros::decl_rils;

use super::prelude::Option;

#[decl_rils(core::rc)]
mod native {
    use super::Option;

    /// A single-threaded reference-counted owning handle.
    #[rils_struct]
    pub struct Rc<T>(std::rc::Rc<T>);

    #[rils_impl]
    impl<T> Clone for Rc<T> {
        fn clone(&self) -> Self {
            Self(std::rc::Rc::clone(&self.0))
        }
    }

    impl<T> std::ops::Deref for Rc<T> {
        type Target = std::rc::Rc<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Rc<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::rc::Rc<T>> for Rc<T> {
        fn from(value: std::rc::Rc<T>) -> Self {
            Self(value)
        }
    }

    impl<T> From<Rc<T>> for std::rc::Rc<T> {
        fn from(value: Rc<T>) -> Self {
            value.0
        }
    }

    impl<T> Rc<T> {
        /// Creates a new reference-counted handle.
        #[export_rils]
        #[rils_legacy_id(core::rc::rc::new)]
        #[rils_native_bridge]
        pub fn new(value: T) -> Self {
            Self(std::rc::Rc::new(value))
        }

        /// Returns the number of strong handles to the shared value.
        #[export_rils]
        #[rils_legacy_id(core::rc::rc::strong_count)]
        pub fn strong_count(&self) -> usize {
            std::rc::Rc::strong_count(&self.0)
        }

        /// Creates a non-owning weak handle to this allocation.
        #[export_rils]
        #[rils_legacy_id(core::rc::rc::downgrade)]
        pub fn downgrade(&self) -> Weak<T> {
            Weak(std::rc::Rc::downgrade(&self.0))
        }
    }

    /// A non-owning reference to an [`Rc<T>`] allocation.
    #[rils_struct]
    pub struct Weak<T>(std::rc::Weak<T>);

    impl<T> std::ops::Deref for Weak<T> {
        type Target = std::rc::Weak<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Weak<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::rc::Weak<T>> for Weak<T> {
        fn from(value: std::rc::Weak<T>) -> Self {
            Self(value)
        }
    }

    impl<T> From<Weak<T>> for std::rc::Weak<T> {
        fn from(value: Weak<T>) -> Self {
            value.0
        }
    }

    impl<T> Weak<T> {
        /// Attempts to upgrade this handle while the allocation is still alive.
        #[export_rils]
        #[rils_legacy_id(core::rc::weak::upgrade)]
        pub fn upgrade(&self) -> Option<Rc<T>> {
            match self.0.upgrade() {
                Some(value) => Option::Some(Rc(value)),
                None => Option::None,
            }
        }

        /// Returns the number of strong owners.
        #[export_rils]
        #[rils_legacy_id(core::rc::weak::strong_count)]
        pub fn strong_count(&self) -> usize {
            self.0.strong_count()
        }

        /// Returns the number of weak handles.
        #[export_rils]
        #[rils_legacy_id(core::rc::weak::weak_count)]
        pub fn weak_count(&self) -> usize {
            self.0.weak_count()
        }
    }
}

pub use native::{Rc, Weak};

/// Type-erased storage for a concrete Rils `Rc<T>` instantiation.
#[doc(hidden)]
#[derive(Clone)]
pub struct ErasedRc(pub std::rc::Rc<rils_value::DynamicValue>);

/// Type-erased storage for a concrete Rils `Weak<T>` instantiation.
#[doc(hidden)]
#[derive(Clone)]
pub struct ErasedWeak(pub std::rc::Weak<rils_value::DynamicValue>);

fn rc_matches(ty: &rils_syntax::Type) -> bool {
    matches!(ty, rils_syntax::Type::Named { name, arguments } if name == "Rc" && arguments.len() == 1)
}

fn weak_matches(ty: &rils_syntax::Type) -> bool {
    matches!(ty, rils_syntax::Type::Named { name, arguments } if name == "Weak" && arguments.len() == 1)
}

fn rc_layout(
    ty: &rils_syntax::Type,
    resolve: &mut rils_native::LayoutResolver<'_>,
) -> std::option::Option<Result<std::rc::Rc<rils_value::DynamicLayout>, std::string::String>> {
    let rils_syntax::Type::Named { arguments, .. } = ty else {
        return std::option::Option::None;
    };
    rc_matches(ty).then(|| {
        resolve(&arguments[0]).map(|_| rils_value::DynamicLayout::of::<ErasedRc>(ty.clone()))
    })
}

fn weak_layout(
    ty: &rils_syntax::Type,
    resolve: &mut rils_native::LayoutResolver<'_>,
) -> std::option::Option<Result<std::rc::Rc<rils_value::DynamicLayout>, std::string::String>> {
    let rils_syntax::Type::Named { arguments, .. } = ty else {
        return std::option::Option::None;
    };
    weak_matches(ty).then(|| {
        resolve(&arguments[0]).map(|_| rils_value::DynamicLayout::of::<ErasedWeak>(ty.clone()))
    })
}

fn clone_rc(view: rils_value::DynamicValueRef<'_>) -> Result<rils_value::DynamicValue, String> {
    rils_value::DynamicValue::from_rust(
        view.layout()?,
        view.with_rust::<ErasedRc, _>(Clone::clone)?,
    )
}

fn clone_weak(view: rils_value::DynamicValueRef<'_>) -> Result<rils_value::DynamicValue, String> {
    rils_value::DynamicValue::from_rust(
        view.layout()?,
        view.with_rust::<ErasedWeak, _>(Clone::clone)?,
    )
}

pub const NATIVE_LAYOUT_RC: rils_native::LayoutRegistration = rils_native::LayoutRegistration {
    matches: rc_matches,
    layout: rc_layout,
};
pub const NATIVE_LAYOUT_WEAK: rils_native::LayoutRegistration = rils_native::LayoutRegistration {
    matches: weak_matches,
    layout: weak_layout,
};
pub const NATIVE_ELEMENT_RC: rils_native::ElementRegistration = rils_native::ElementRegistration {
    matches: rc_matches,
    clone_borrowed: clone_rc,
};
pub const NATIVE_ELEMENT_WEAK: rils_native::ElementRegistration =
    rils_native::ElementRegistration {
        matches: weak_matches,
        clone_borrowed: clone_weak,
    };
