use std::{error::Error, fmt};

use rils_frontend::{FrontendError, Span};

#[derive(Clone, Debug, PartialEq)]
pub enum DriverError {
    Frontend(FrontendError),
    Message { message: String, span: Span },
}

impl DriverError {
    pub fn message(message: impl Into<String>, span: Span) -> Self {
        Self::Message {
            message: message.into(),
            span,
        }
    }

    pub fn span(&self) -> Span {
        match self {
            Self::Frontend(error) => error.span(),
            Self::Message { span, .. } => *span,
        }
    }
}

impl From<FrontendError> for DriverError {
    fn from(error: FrontendError) -> Self {
        Self::Frontend(error)
    }
}

impl fmt::Display for DriverError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frontend(error) => error.fmt(formatter),
            Self::Message { message, .. } => formatter.write_str(message),
        }
    }
}

impl Error for DriverError {}
