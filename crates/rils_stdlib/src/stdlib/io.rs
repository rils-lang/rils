//! Host-provided IO error types.

use rils_builtins_macros::decl_rils;

#[decl_rils(std::io)]
mod native {
    use super::super::{prelude::Result, string::String};
    use std::io::Write;

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

    fn error(source: std::io::Error) -> Error {
        Error {
            kind: source.kind(),
            message: source.to_string(),
            path: None,
        }
    }

    fn write_text(text: &str) -> Result<(), Error> {
        match std::io::stdout().lock().write_all(text.as_bytes()) {
            Ok(()) => Result::Ok(()),
            Err(source) => Result::Err(error(source)),
        }
    }

    /// Reads one line from standard input.
    #[rils_fn]
    pub fn read_line() -> Result<rils_stdlib::stdlib::string::String, rils_stdlib::stdlib::io::Error>
    {
        let mut line = std::string::String::new();
        match std::io::stdin().read_line(&mut line) {
            Ok(_) => Result::Ok(line.into()),
            Err(source) => Result::Err(error(source)),
        }
    }

    /// Prints values without a trailing newline.
    #[rils_fn]
    #[rils_variadic]
    pub fn print(values: &[String]) {
        let text = values
            .iter()
            .cloned()
            .map(std::string::String::from)
            .collect::<std::string::String>();
        let _ = std::io::stdout().lock().write_all(text.as_bytes());
    }

    /// Prints values followed by a newline.
    #[rils_fn]
    #[rils_variadic]
    pub fn println(values: &[String]) {
        let mut text = values
            .iter()
            .cloned()
            .map(std::string::String::from)
            .collect::<std::string::String>();
        text.push('\n');
        let _ = std::io::stdout().lock().write_all(text.as_bytes());
    }

    /// Writes a value to standard output.
    #[rils_fn]
    #[rils_any(value)]
    pub fn write(value: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let text: std::string::String = value.into();
        write_text(&text)
    }

    /// Writes a value and a newline.
    #[rils_fn]
    #[rils_any(value)]
    pub fn write_line(value: String) -> Result<(), rils_stdlib::stdlib::io::Error> {
        let mut text: std::string::String = value.into();
        text.push('\n');
        write_text(&text)
    }

    /// Flushes standard output.
    #[rils_fn]
    pub fn flush() -> Result<(), rils_stdlib::stdlib::io::Error> {
        match std::io::stdout().lock().flush() {
            Ok(()) => Result::Ok(()),
            Err(source) => Result::Err(error(source)),
        }
    }
}

pub use native::{Error, ErrorKind, flush, print, println, read_line, write, write_line};
