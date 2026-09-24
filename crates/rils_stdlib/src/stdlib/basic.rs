//! Basic owned and formatting types exposed by the Rils standard library.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::boxed)]
mod boxed {
    /// An owned heap indirection for recursive data structures.
    #[rils_struct]
    pub struct Box<T>(std::boxed::Box<T>);

    impl<T> Box<T> {
        pub fn from_std(value: std::boxed::Box<T>) -> Self {
            Self(value)
        }

        pub fn into_std(self) -> std::boxed::Box<T> {
            self.0
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
