//! Host-provided IO error types.

use rils_builtins_macros::decl_rils;

#[decl_rils(std::io)]
mod native {
    /// Portable categories for standard IO and filesystem failures.
    #[rils_enum]
    pub enum ErrorKind {
        /// The requested path or resource does not exist.
        NotFound,
        /// Access was denied by the host platform.
        PermissionDenied,
        /// A resource already exists at the requested location.
        AlreadyExists,
        /// An argument is not valid for the requested operation.
        InvalidInput,
        /// Input data is malformed or otherwise invalid.
        InvalidData,
        /// The operation did not finish before its deadline.
        TimedOut,
        /// The operation was interrupted and may be retried.
        Interrupted,
        /// Input ended before the requested data was available.
        UnexpectedEof,
        /// A write operation could not make progress.
        WriteZero,
        /// An error without a more specific portable category.
        Other,
    }

    /// An error returned by a standard IO or filesystem operation.
    #[rils_struct]
    pub struct Error {
        /// The portable category of the error.
        pub kind: std::io::ErrorKind,
        /// A human-readable description.
        pub message: std::string::String,
        /// The related filesystem path, when one is available.
        pub path: std::option::Option<std::string::String>,
    }
}

pub use native::{Error, ErrorKind};
