//! Formatting contracts and the temporary formatting destination.

use rils_builtins_macros::decl_rils;

#[decl_rils(core::fmt)]
mod native {
    use super::super::{basic::FormatError, result::Result, string::String};

    /// Diagnostic textual formatting.
    #[rils_trait]
    pub trait Debug: ::std::fmt::Debug {
        /// Writes the diagnostic representation into a formatter.
        fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError>;
    }

    /// User-facing textual formatting.
    #[rils_trait]
    pub trait Display: ::std::fmt::Display {
        /// Writes the user-facing representation into a formatter.
        fn fmt(&self, formatter: &mut Formatter) -> Result<(), FormatError>;
    }

    /// A transient formatting destination supplied by format macros.
    #[rils_struct]
    #[derive(Default)]
    pub struct Formatter(std::string::String);

    impl Formatter {
        /// Appends text to this formatting destination.
        #[export_rils]
        #[rils_legacy_id(core::fmt::write_str)]
        pub fn write_str(&mut self, value: String) -> Result<(), FormatError> {
            self.0.push_str(&std::string::String::from(value));
            Result::Ok(())
        }

        /// Writes the structural Debug representation used by derived implementations.
        #[export_rils]
        #[rils_legacy_id(core::fmt::write_derived_debug)]
        #[rils_ref_any(value)]
        pub fn write_derived_debug(
            &mut self,
            value: &dyn std::fmt::Debug,
        ) -> Result<(), FormatError> {
            use std::fmt::Write;
            match write!(&mut self.0, "{value:?}") {
                Ok(()) => Result::Ok(()),
                Err(_) => Result::Err(FormatError),
            }
        }
    }
}

pub use native::{Debug, Display, Formatter};
