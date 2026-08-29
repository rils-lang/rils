//! Immutable canonical resolution for named host types imported into Rils source.

mod side_table;

pub use side_table::{HostTypeResolutionResults, HostTypeResolutionView, resolve_host_types};

use crate::{ast::Stmt, source::Span};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTypeResolutionError {
    pub message: String,
    pub span: Span,
}

fn public_inner(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}

fn path_candidates(prefix: &[String], path: &[String]) -> Vec<String> {
    let Some(first) = path.first().map(String::as_str) else {
        return Vec::new();
    };
    if matches!(first, "crate" | "self" | "super") {
        let mut output = match first {
            "crate" => Vec::new(),
            "self" => prefix.to_vec(),
            "super" => {
                let mut output = prefix.to_vec();
                output.pop();
                output
            }
            _ => unreachable!(),
        };
        for segment in path.iter().skip(1) {
            match segment.as_str() {
                "crate" => output.clear(),
                "self" => {}
                "super" => {
                    output.pop();
                }
                _ => output.push(segment.clone()),
            }
        }
        return vec![output.join("::")];
    }
    let absolute = path.join("::");
    if prefix.is_empty() {
        vec![absolute]
    } else {
        vec![format!("{}::{absolute}", prefix.join("::")), absolute]
    }
}

#[cfg(test)]
#[path = "../tests/unit/host_type_resolution.rs"]
mod tests;
