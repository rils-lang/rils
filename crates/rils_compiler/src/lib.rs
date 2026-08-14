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

use rils_frontend::{
    analysis::DiagnosticSeverity,
    ast::Program,
    source::{SourceFile, Span},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub span: Span,
    source_name: Option<String>,
    source: Option<String>,
}

impl CompileError {
    pub fn new(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            source_name: None,
            source: None,
        }
    }

    pub fn unsupported(message: impl Into<String>, span: Span) -> Self {
        Self::new(message, span)
    }

    pub fn with_source(
        mut self,
        source_name: impl Into<String>,
        source: impl Into<String>,
    ) -> Self {
        self.source_name = Some(source_name.into());
        self.source = Some(source.into());
        self
    }

    pub fn source_name(&self) -> Option<&str> {
        self.source_name.as_deref()
    }

    pub fn source_text(&self) -> Option<&str> {
        self.source.as_deref()
    }

    pub fn render(&self, source_name: &str, source: &str) -> String {
        let source_name = self.source_name().unwrap_or(source_name);
        let source = self.source_text().unwrap_or(source);
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
    let tokens = rils_frontend::lexer::lex(source)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    let program = rils_frontend::parser::parse(tokens)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    compile_program_with_host(&program, host)
}

pub fn compile_program(program: &Program) -> Result<mir::MirProgram, CompileError> {
    compile_program_with_host(program, &HostContract::new())
}

pub fn compile_program_with_host(
    program: &Program,
    host: &HostContract,
) -> Result<mir::MirProgram, CompileError> {
    compile_program_with_host_and_sources(program, host, Vec::new())
}

pub fn compile_program_with_host_and_sources(
    program: &Program,
    host: &HostContract,
    sources: Vec<SourceFile>,
) -> Result<mir::MirProgram, CompileError> {
    let mut program = program.clone();
    let signatures = host.signatures();
    rils_frontend::resolve_numeric_literals_with_host_functions(&mut program, &signatures)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    let analysis =
        rils_frontend::analysis::analyze_program_with_host_functions(&program, &signatures);
    if let Some(diagnostic) = analysis
        .diagnostics
        .into_iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError::new(diagnostic.message, diagnostic.span));
    }
    mir::lower(hir::lower_with_host(
        &program,
        host,
        &analysis.expression_types,
        sources,
    )?)
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
