pub mod hir;
mod host;
pub mod mir;

pub use host::{
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

use std::{collections::BTreeMap, error::Error, fmt};

use rils_frontend::{
    analysis::DiagnosticSeverity,
    ast::{EnumVariant, Program, Stmt},
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

pub fn analyze_with_host(
    source: &str,
    host: &HostContract,
) -> Result<rils_frontend::analysis::DocumentAnalysis, rils_frontend::FrontendError> {
    let tokens = rils_frontend::lexer::lex(source).map_err(rils_frontend::FrontendError::Lex)?;
    let mut program =
        rils_frontend::parser::parse(tokens).map_err(rils_frontend::FrontendError::Parse)?;
    inject_host_enum_declarations(&mut program, host);
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    Ok(
        rils_frontend::analysis::analyze_program_with_host_declarations(
            &program,
            &signatures,
            &host_types,
        ),
    )
}

pub fn analyze_with_host_and_source_id_and_external_exports(
    source: &str,
    source_id: rils_frontend::SourceId,
    host: &HostContract,
    external_exports: &std::collections::HashMap<
        String,
        Vec<rils_frontend::analysis::ExternalModuleExport>,
    >,
) -> Result<rils_frontend::analysis::DocumentAnalysis, rils_frontend::FrontendError> {
    let tokens = rils_frontend::lexer::lex_with_source_id(source, source_id)
        .map_err(rils_frontend::FrontendError::Lex)?;
    let mut program =
        rils_frontend::parser::parse(tokens).map_err(rils_frontend::FrontendError::Parse)?;
    inject_host_enum_declarations(&mut program, host);
    let signatures = host.signatures();
    let host_types = host
        .types()
        .map(|declaration| declaration.name.clone())
        .collect();
    Ok(
        rils_frontend::analysis::analyze_program_with_source_id_and_external_exports_and_host_types(
            &program,
            source_id,
            &signatures,
            &host_types,
            external_exports,
        ),
    )
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
    inject_host_enum_declarations(&mut program, host);
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
    rils_frontend::resolve_numeric_literals_with_host_functions(&mut program, &signatures)
        .map_err(|error| CompileError::new(error.message, error.span))?;
    let analysis = rils_frontend::analysis::analyze_program_with_host_declarations(
        &program,
        &signatures,
        &host_types,
    );
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

#[derive(Default)]
struct HostEnumModule {
    enums: Vec<(String, Vec<String>)>,
    children: BTreeMap<String, HostEnumModule>,
}

fn inject_host_enum_declarations(program: &mut Program, host: &HostContract) {
    let mut root = HostEnumModule::default();
    let mut flag_types = Vec::new();
    for declaration in host.types() {
        let Some(definition) = declaration.enum_definition.as_ref() else {
            continue;
        };
        let mut segments = declaration.name.split("::").collect::<Vec<_>>();
        let Some(name) = segments.pop() else {
            continue;
        };
        let mut module = &mut root;
        for segment in segments {
            module = module.children.entry(segment.to_owned()).or_default();
        }
        module.enums.push((
            name.to_owned(),
            definition.variants.keys().cloned().collect(),
        ));
        if definition.flags {
            flag_types.push(declaration.name.clone());
        }
    }
    let mut declarations = host_enum_module_statements(root);
    declarations.extend(flag_types.into_iter().map(|name| Stmt::Impl {
        generic_parameters: Vec::new(),
        trait_name: Some("BitFlags".into()),
        target: rils_frontend::Type::named(name),
        associated_types: Vec::new(),
        methods: Vec::new(),
        span: Span::default(),
    }));
    program.statements.splice(0..0, declarations);
}

fn host_enum_module_statements(module: HostEnumModule) -> Vec<Stmt> {
    let mut statements = module
        .enums
        .into_iter()
        .map(|(name, variants)| Stmt::Public {
            statement: Box::new(Stmt::Enum {
                attributes: Vec::new(),
                name: name.clone(),
                name_span: Span::default(),
                generic_parameters: Vec::new(),
                variants: variants
                    .into_iter()
                    .map(|name| EnumVariant::Unit {
                        name,
                        span: Span::default(),
                    })
                    .collect(),
                span: Span::default(),
            }),
            span: Span::default(),
        })
        .collect::<Vec<_>>();
    statements.extend(
        module
            .children
            .into_iter()
            .map(|(name, child)| Stmt::Public {
                statement: Box::new(Stmt::Module {
                    name: name.clone(),
                    name_span: Span::default(),
                    statements: Some(host_enum_module_statements(child)),
                    span: Span::default(),
                }),
                span: Span::default(),
            }),
    );
    statements
}

#[cfg(test)]
#[path = "../tests/unit/compiler.rs"]
mod tests;
