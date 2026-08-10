pub mod hir;
pub mod mir;

mod ast {
    pub(crate) use rils_frontend::ast::*;
}
mod bytecode {
    pub(crate) use crate::CompileError;
}
mod source {
    pub(crate) use rils_frontend::source::*;
}
mod types {
    pub(crate) use rils_frontend::types::*;
}

use std::{error::Error, fmt};

use rils_frontend::{
    analysis::{DiagnosticSeverity, analyze_program},
    ast::Program,
    source::Span,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
}

impl CompileError {
    pub fn unsupported(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
        }
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        rils_frontend::source::format_diagnostic(
            source_name,
            source,
            self.span,
            &format!("compile error: {}", self.message),
        )
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "compile error: {}", self.message)
    }
}

impl Error for CompileError {}

pub fn compile(source: &str) -> Result<mir::MirProgram, CompileError> {
    let tokens = rils_frontend::lexer::lex(source).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    let program = rils_frontend::parser::parse(tokens).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    compile_program(&program)
}

pub fn compile_program(program: &Program) -> Result<mir::MirProgram, CompileError> {
    let mut program = program.clone();
    rils_frontend::resolve_numeric_literals(&mut program).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    if let Some(diagnostic) = analyze_program(&program)
        .diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError {
            message: diagnostic.message,
            span: diagnostic.span,
        });
    }
    mir::lower(hir::lower(&program)?)
}

#[cfg(test)]
mod tests {
    use super::compile;

    #[test]
    fn compiles_source_through_static_analysis_hir_and_mir() {
        let program = compile("fn add(left: i32, right: i32) -> i32 { left + right } add(1, 2)")
            .expect("source should lower to MIR");

        assert_eq!(program.entry, 0);
        assert_eq!(program.functions.len(), 2);
        assert!(
            program
                .functions
                .iter()
                .all(|function| !function.blocks.is_empty())
        );
    }

    #[test]
    fn rejects_static_errors_before_lowering() {
        let error = match compile("let value = 1; value = 2;") {
            Ok(_) => panic!("assignment should fail"),
            Err(error) => error,
        };

        assert!(error.message.contains("immutable"));
    }
}
