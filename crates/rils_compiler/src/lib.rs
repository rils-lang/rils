pub mod hir;
pub mod mir;

pub use rils_frontend::{
    analyze_program_with_host_and_source_id_and_external_exports, analyze_with_host,
    analyze_with_host_and_source_id_and_external_exports,
};
pub use rils_host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_INLINE_VALUE_MAX_BYTES,
    HOST_INLINE_VALUE_MAX_FIELDS, HOST_MANIFEST_FORMAT_VERSION, HOST_MANIFEST_HEADER_SIZE,
    HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES, HOST_MANIFEST_MAGIC,
    HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_ENUM_VARIANTS, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HOST_MANIFEST_MAX_TYPES, HostCallKind,
    HostContract, HostEnumDefinition, HostFunctionDeclaration, HostModuleDeclaration, HostReceiver,
    HostThreadAffinity, HostTypeDeclaration, HostTypeTransport, HostValueFieldType,
    HostValueLayout,
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
    compile_program_with_host_and_sources_and_entry(program, host, sources, None)
}

fn compile_program_with_host_and_sources_and_entry(
    program: &Program,
    host: &HostContract,
    sources: Vec<SourceFile>,
    entry: Option<(rils_frontend::SourceId, String)>,
) -> Result<mir::MirProgram, CompileError> {
    let mut program = program.clone();
    rils_frontend::inject_host_enum_declarations(&mut program, host);
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    if let Some(error) = rils_frontend::resolve_host_type_names(&mut program, &host_types)
        .into_iter()
        .next()
    {
        return Err(CompileError::new(error.message, error.span));
    }
    let analysis = rils_frontend::analysis::analyze_program_with_host_declarations(
        &program,
        &signatures,
        &host_types,
    );
    if let Some(diagnostic) = analysis
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError::new(
            diagnostic.message.clone(),
            diagnostic.span,
        ));
    }
    let entry = entry
        .map(|(source, module_path)| {
            analysis
                .def_map
                .definitions()
                .find(|definition| {
                    definition.name == "main"
                        && definition.span.source == source
                        && definition.kind == rils_frontend::semantic::SymbolKind::Function
                        && definition.container
                            == Some(rils_frontend::SymbolContainer::Module(module_path.clone()))
                })
                .map(|definition| definition.id)
                .ok_or_else(|| {
                    CompileError::new(
                        "project entry definition was not preserved during analysis",
                        Span::default(),
                    )
                })
        })
        .transpose()?;
    mir::lower(hir::lower_with_host(
        &program, host, &analysis, sources, entry,
    )?)
}

pub fn compile_program_with_host_and_session(
    host: &HostContract,
    session: &rils_frontend::CompilationSession,
    project: rils_frontend::ProjectId,
) -> Result<mir::MirProgram, CompileError> {
    let Some(semantics) = session.project(project) else {
        return Err(CompileError::new(
            "compilation project is not registered in this session",
            Span::default(),
        ));
    };
    let syntax = session.project_syntax(project).ok_or_else(|| {
        CompileError::new(
            "compilation project has no registered syntax state",
            Span::default(),
        )
    })?;
    let mut syntax = syntax.clone();
    let mut host_program = Program {
        statements: Vec::new(),
        type_references: Vec::new(),
        macros: Vec::new(),
    };
    rils_frontend::inject_host_enum_declarations(&mut host_program, host);
    if !host_program.statements.is_empty() {
        syntax.push_root(host_program);
    }
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    let prepare = |program: &mut Program| -> Result<(), CompileError> {
        if let Some(error) = rils_frontend::resolve_host_type_names(program, &host_types)
            .into_iter()
            .next()
        {
            return Err(CompileError::new(error.message, error.span));
        }
        Ok(())
    };
    for program in syntax.roots_mut() {
        prepare(program)?;
    }
    for (_, program) in syntax.modules_mut() {
        prepare(program)?;
    }
    let analysis = rils_frontend::analyze_project_with_host_declarations(
        &syntax,
        semantics.module_graph(),
        &signatures,
        &host_types,
    );
    if let Some(diagnostic) = analysis
        .diagnostics
        .iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        return Err(CompileError::new(
            diagnostic.message.clone(),
            diagnostic.span,
        ));
    }
    let entry = (|| {
        let source = semantics.entry_source()?;
        let module = semantics.module(source)?;
        Some((source, module.path.clone()))
    })();
    let entry = entry
        .map(|(source, module_path)| {
            analysis
                .def_map
                .definitions()
                .find(|definition| {
                    definition.name == "main"
                        && definition.span.source == source
                        && definition.kind == rils_frontend::semantic::SymbolKind::Function
                        && definition.container
                            == Some(rils_frontend::SymbolContainer::Module(module_path.clone()))
                })
                .map(|definition| definition.id)
                .ok_or_else(|| {
                    CompileError::new(
                        "project entry definition was not preserved during analysis",
                        Span::default(),
                    )
                })
        })
        .transpose()?;
    mir::lower(hir::lower_project_with_host(
        &syntax,
        semantics.module_graph(),
        host,
        &analysis,
        session.sources().source_files(),
        entry,
    )?)
}

#[cfg(test)]
#[path = "../tests/unit/compiler.rs"]
mod tests;
