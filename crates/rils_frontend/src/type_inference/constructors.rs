//! Retain constructor identity through imports and lexical bindings.

use super::*;

pub(super) fn standard_constructor(path: &str) -> Option<&'static str> {
    let declaration = rils_builtins::builtin_function(path)?;
    (declaration.backend == rils_builtins::BuiltinBackend::Metadata
        && matches!(declaration.path, "Some" | "Ok" | "Err"))
    .then_some(declaration.path)
}

impl Inferencer<'_> {
    pub(super) fn sum_constructor(&self, callee: &Expr) -> Option<&'static str> {
        match callee {
            Expr::Variable { name, .. } => self.lookup(name)?.constructor,
            Expr::Path { segments, .. } | Expr::GenericPath { segments, .. } => {
                let segments = self
                    .host_types
                    .resolved_expression_path(callee)
                    .unwrap_or(segments);
                standard_constructor(&segments.join("::"))
            }
            _ => None,
        }
    }
}
