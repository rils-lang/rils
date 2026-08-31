use std::{io::Write, rc::Rc};

use crate::Value;

pub type OutputHandler = dyn Fn(&str, bool) -> Result<(), String>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostFormatKind {
    Display,
    Debug,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HostFormatSpec {
    pub kind: HostFormatKind,
    pub alternate: bool,
    pub precision: Option<usize>,
}

pub type HostValueFormatter = dyn Fn(&Value, HostFormatSpec) -> Result<Option<String>, String>;

pub fn default_output_handler() -> Rc<OutputHandler> {
    Rc::new(|text, newline| {
        let mut stdout = std::io::stdout().lock();
        stdout
            .write_all(text.as_bytes())
            .map_err(|error| error.to_string())?;
        if newline {
            stdout.write_all(b"\n").map_err(|error| error.to_string())?;
        }
        stdout.flush().map_err(|error| error.to_string())
    })
}
