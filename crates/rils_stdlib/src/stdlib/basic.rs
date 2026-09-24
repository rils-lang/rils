//! Basic owned and formatting types exposed by the Rils standard library.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::boxed)]
mod boxed {
    /// An owned heap indirection for recursive data structures.
    #[rils_struct]
    pub struct Box<T>(std::boxed::Box<T>);

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
