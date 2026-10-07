//! Basic owned and formatting types exposed by the Rils standard library.

use rils_stdlib_macros::decl_rils;

pub const NATIVE_EQUALITY_UNIT: rils_native::EqualityRegistration =
    rils_native::EqualityRegistration::of::<()>();
pub const NATIVE_EQUALITY_BOOL: rils_native::EqualityRegistration =
    rils_native::EqualityRegistration::of::<bool>();
pub const NATIVE_EQUALITY_CHAR: rils_native::EqualityRegistration =
    rils_native::EqualityRegistration::of::<char>();

pub const NATIVE_FORMAT_BOOL: rils_native::FormatRegistration =
    rils_native::FormatRegistration::of::<bool>();
pub const NATIVE_FORMAT_CHAR: rils_native::FormatRegistration =
    rils_native::FormatRegistration::of::<char>();

#[decl_rils(core::boxed)]
mod boxed {
    /// An owned heap indirection for recursive data structures.
    #[rils_struct]
    pub struct Box<T>(std::boxed::Box<T>);

    impl<T> Box<T> {
        /// Allocates a value on the heap.
        #[export_rils]
        pub fn new(value: T) -> Self {
            Self(std::boxed::Box::new(value))
        }

        /// Consumes the box and returns its value.
        #[export_rils]
        pub fn into_inner(self) -> T {
            *self.0
        }
    }

    impl<T> std::ops::Deref for Box<T> {
        type Target = std::boxed::Box<T>;

        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }

    impl<T> std::ops::DerefMut for Box<T> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }

    impl<T> From<std::boxed::Box<T>> for Box<T> {
        fn from(value: std::boxed::Box<T>) -> Self {
            Self(value)
        }
    }
}

#[decl_rils(core::format_error)]
mod formatting {
    /// An error produced while formatting a value.
    #[rils_struct]
    pub struct FormatError;
}

pub use boxed::Box;
pub use formatting::FormatError;

pub fn is_native_box(ty: &rils_syntax::Type) -> bool {
    box_layout::matches(ty)
}
