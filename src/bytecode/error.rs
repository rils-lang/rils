use std::{error::Error, fmt};

use crate::source::{Span, format_diagnostic};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeError {
    pub message: String,
    pub span: Span,
}

impl BytecodeError {
    pub(super) fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        format_diagnostic(
            source_name,
            source,
            self.span,
            &format!("bytecode error: {}", self.message),
        )
    }
}

impl fmt::Display for BytecodeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "bytecode error: {}", self.message)
    }
}

impl Error for BytecodeError {}
