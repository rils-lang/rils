use std::{error::Error, fmt};

use crate::{
    FrontendError, Span, interpreter::RuntimeError, lexer::LexError, parser::ParseError,
    source::format_diagnostic,
};

#[derive(Clone, Debug, PartialEq)]
pub enum RilsError {
    Lex(LexError),
    Parse(ParseError),
    Runtime(RuntimeError),
    Located {
        error: Box<RilsError>,
        source_name: String,
        source: String,
    },
}

impl From<FrontendError> for RilsError {
    fn from(error: FrontendError) -> Self {
        match error {
            FrontendError::Lex(error) => Self::Lex(error),
            FrontendError::Parse(error) => Self::Parse(error),
        }
    }
}

impl RilsError {
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span,
            Self::Parse(error) => error.span,
            Self::Runtime(error) => error.span,
            Self::Located { error, .. } => error.span(),
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        match self {
            Self::Located {
                error,
                source_name,
                source,
            } => error.render(source_name, source),
            Self::Lex(error) => format_diagnostic(
                source_name,
                source,
                error.span,
                &format!("lex error: {}", error.message),
            ),
            Self::Parse(error) => format_diagnostic(
                source_name,
                source,
                error.span,
                &format!("parse error: {}", error.message),
            ),
            Self::Runtime(error) => {
                let mut diagnostic = format_diagnostic(
                    source_name,
                    source,
                    error.span,
                    &format!("runtime error: {}", error.message),
                );
                if !error.stack.is_empty() {
                    diagnostic.push_str("\n\nRils stack:");
                    for function in error.stack.iter().rev() {
                        diagnostic.push_str(&format!("\n  in {function}"));
                    }
                }
                diagnostic
            }
        }
    }
}

impl fmt::Display for RilsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(f, "lex error: {}", error.message),
            Self::Parse(error) => write!(f, "parse error: {}", error.message),
            Self::Runtime(error) => write!(f, "runtime error: {}", error.message),
            Self::Located { error, .. } => error.fmt(f),
        }
    }
}

impl Error for RilsError {}
