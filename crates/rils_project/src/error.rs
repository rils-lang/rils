use std::{error::Error, fmt};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectError {
    pub message: String,
}

pub(crate) fn project_error(message: impl Into<String>) -> ProjectError {
    ProjectError {
        message: message.into(),
    }
}

impl fmt::Display for ProjectError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for ProjectError {}
