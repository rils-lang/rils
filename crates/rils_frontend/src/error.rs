use std::{error::Error, fmt};

use crate::{lexer::LexError, parser::ParseError, source::Span};

#[derive(Clone, Debug, PartialEq)]
pub enum FrontendError {
    Lex(LexError),
    Parse(ParseError),
}

impl FrontendError {
    pub fn span(&self) -> Span {
        match self {
            Self::Lex(error) => error.span,
            Self::Parse(error) => error.span,
        }
    }
}

impl fmt::Display for FrontendError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Lex(error) => write!(formatter, "lex error: {}", error.message),
            Self::Parse(error) => write!(formatter, "parse error: {}", error.message),
        }
    }
}

impl Error for FrontendError {}
