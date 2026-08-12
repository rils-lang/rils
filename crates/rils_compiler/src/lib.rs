pub mod hir;
mod host;
pub mod mir;

pub use host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_MANIFEST_FORMAT_VERSION,
    HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES,
    HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HostCallKind, HostContract,
    HostFunctionDeclaration, HostModuleDeclaration, HostThreadAffinity,
};

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

use rils_frontend::{analysis::DiagnosticSeverity, ast::Program, source::Span};

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
    compile_with_host(source, &HostContract::new())
}

pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<mir::MirProgram, CompileError> {
    let tokens = rils_frontend::lexer::lex(source).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    let program = rils_frontend::parser::parse(tokens).map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    compile_program_with_host(&program, host)
}

pub fn compile_program(program: &Program) -> Result<mir::MirProgram, CompileError> {
    compile_program_with_host(program, &HostContract::new())
}

pub fn compile_program_with_host(
    program: &Program,
    host: &HostContract,
) -> Result<mir::MirProgram, CompileError> {
    let mut program = program.clone();
    let signatures = host.signatures();
    rils_frontend::resolve_numeric_literals_with_host_functions(&mut program, &signatures)
        .map_err(|error| CompileError {
            message: error.message,
            span: error.span,
        })?;
    if let Some(diagnostic) =
        rils_frontend::analysis::analyze_program_with_host_functions(&program, &signatures)
            .diagnostics
            .into_iter()
            .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError {
            message: diagnostic.message,
            span: diagnostic.span,
        });
    }
    mir::lower(hir::lower_with_host(&program, host)?)
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
