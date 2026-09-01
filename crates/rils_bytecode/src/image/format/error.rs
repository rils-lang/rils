use std::{error::Error, fmt, io, path::Path};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeFormatError {
    pub message: String,
}

impl BytecodeFormatError {
    pub(super) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }

    pub(super) fn io(action: &str, path: &Path, error: io::Error) -> Self {
        Self::new(format!("failed to {action} `{}`: {error}", path.display()))
    }
}

impl fmt::Display for BytecodeFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bytecode format error: {}", self.message)
    }
}

impl Error for BytecodeFormatError {}
