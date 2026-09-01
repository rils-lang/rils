//! Bytecode compiler, verified format, virtual machine, and bytecode host for Rils.

use std::{fs, path::Path};

use rils_driver::ProjectSources;
use rils_execution::{
    ExecutionLimits, FloatType, HostFormatKind, HostFormatSpec, HostValueFormatter, IntegerType,
    OutputHandler,
};
use rils_frontend::Span;
use rils_project::{Project, ProjectKind};

#[cfg(test)]
use rils_runtime::{Value, eval};

mod ast {
    pub(crate) use rils_frontend::ast::*;
}
mod environment {
    pub(crate) use rils_execution::environment::*;
}
mod formatting {
    pub(crate) use rils_execution::formatting::*;
}
mod hash_collections {
    pub(crate) use rils_execution::hash_collections::*;
}
mod hir {
    pub(crate) use rils_compiler::hir::*;
}
mod lexer {
    #[allow(unused_imports)]
    pub(crate) use rils_frontend::lexer::*;
}
mod macros {
    pub(crate) use rils_frontend::macros::*;
}
mod mir {
    pub(crate) use rils_compiler::mir::*;
}
mod numeric {
    pub(crate) use rils_execution::numeric::*;
}
mod output {
    pub(crate) use rils_execution::output::*;
}
mod parser {
    #[allow(unused_imports)]
    pub(crate) use rils_frontend::parser::*;
}
mod runtime_builtins {
    pub(crate) use rils_execution::runtime_builtins::*;
}
mod source {
    pub(crate) use rils_frontend::source::*;
}
mod standard_library {
    pub(crate) use rils_execution::standard_library::*;
}
mod types {
    pub(crate) use rils_frontend::types::*;
}
mod value {
    pub(crate) use rils_execution::value::*;
}

pub mod bytecode;
mod library;

pub use bytecode::{
    BYTECODE_FORMAT_VERSION, BYTECODE_HOST_ABI_VERSION, BYTECODE_LANGUAGE_VERSION, BytecodeError,
    BytecodeFormatError, BytecodeHost, BytecodeImport, BytecodeModule, CompileError,
};
pub use library::{LibraryFormatError, RilsLibrary};
pub use rils_host::{
    HOST_CONTRACT_ABI_VERSION, HOST_CONTRACT_HASH_ALGORITHM, HOST_MANIFEST_FORMAT_VERSION,
    HOST_MANIFEST_HEADER_SIZE, HOST_MANIFEST_JSON_FORMAT_VERSION, HOST_MANIFEST_JSON_MAX_BYTES,
    HOST_MANIFEST_MAGIC, HOST_MANIFEST_MAX_BYTES, HOST_MANIFEST_MAX_FUNCTIONS,
    HOST_MANIFEST_MAX_MODULES, HOST_MANIFEST_MAX_PARAMETERS, HOST_MANIFEST_MAX_TYPES, HostCallKind,
    HostContract, HostEnumDefinition, HostFunctionDeclaration, HostModuleDeclaration, HostReceiver,
    HostThreadAffinity, HostTypeDeclaration, HostTypeTransport, HostValueLayout,
};
/// Compiles Rils source into a verified, reusable in-memory bytecode module.
pub fn compile(source: &str) -> Result<BytecodeModule, CompileError> {
    bytecode::compile(source)
}

/// Compiles Rils source using declarations supplied by a host contract.
pub fn compile_with_host(
    source: &str,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    bytecode::compile_with_host(source, host)
}

/// Loads a Rils source file and its external modules, then compiles them into a
/// reusable in-memory bytecode module.
pub fn compile_file(path: impl AsRef<Path>) -> Result<BytecodeModule, CompileError> {
    let path = path.as_ref();
    let project = rils_driver::discover_entry_project(path)
        .map_err(|error| CompileError::new(error.to_string(), Span::default()))?;
    let mut host: Option<HostContract> = None;
    for manifest in project.host_manifests() {
        let bytes = fs::read(manifest).map_err(|error| {
            CompileError::new(
                format!(
                    "failed to read host manifest `{}`: {error}",
                    manifest.display()
                ),
                Span::default(),
            )
        })?;
        let fragment = HostContract::from_manifest_bytes(&bytes).map_err(|message| {
            CompileError::new(
                format!("invalid host manifest `{}`: {message}", manifest.display()),
                Span::default(),
            )
        })?;
        if let Some(host) = &mut host {
            host.merge(&fragment).map_err(|message| {
                CompileError::new(
                    format!(
                        "cannot merge host manifest `{}`: {message}",
                        manifest.display()
                    ),
                    Span::default(),
                )
            })?;
        } else {
            host = Some(fragment);
        }
    }
    let host = host.unwrap_or_default();
    compile_project_file_with_host(path, &project, &host, project.requires_entry())
}

/// Compiles a configured library project into a verified `.rilslib` artifact.
pub fn compile_library(path: impl AsRef<Path>) -> Result<RilsLibrary, CompileError> {
    let path = path.as_ref();
    let project = if path.is_file() && path.file_name().is_some_and(|name| name == "rils.toml") {
        Project::from_file(path)
    } else {
        Project::discover(path, None)
    }
    .map_err(|error| CompileError::new(error.to_string(), Span::default()))?;
    if project.kind() != ProjectKind::Lib {
        return Err(CompileError::new(
            format!(
                "project `{}` is executable; only library projects can produce .rilslib artifacts",
                project.name()
            ),
            Span::default(),
        ));
    }
    let requested_source = path.is_file()
        && path
            .extension()
            .is_some_and(|extension| extension == "rils")
        && project.module_for_file(path).is_some();
    let entry = if requested_source {
        path.to_path_buf()
    } else {
        project
            .modules()
            .find(|file| {
                project
                    .source_roots()
                    .iter()
                    .any(|root| file.path.starts_with(root))
            })
            .map(|file| file.path.clone())
            .ok_or_else(|| {
                CompileError::new(
                    format!("library project `{}` contains no modules", project.name()),
                    Span::default(),
                )
            })?
    };
    let module = compile_file(entry)?;
    RilsLibrary::new(project.name(), module)
        .map_err(|error| CompileError::new(error.to_string(), Span::default()))
}

/// Loads and compiles a Rils module tree using declarations supplied by a host contract.
pub fn compile_file_with_host(
    path: impl AsRef<Path>,
    host: &HostContract,
) -> Result<BytecodeModule, CompileError> {
    let path = path.as_ref();
    let project = rils_driver::discover_entry_project(path)
        .map_err(|error| CompileError::new(error.to_string(), Span::default()))?;
    compile_project_file_with_host(path, &project, host, project.requires_entry())
}

fn compile_project_file_with_host(
    path: &Path,
    project: &Project,
    host: &HostContract,
    require_entry: bool,
) -> Result<BytecodeModule, CompileError> {
    let mut sources = ProjectSources::default();
    sources.register_project(project);
    let result = (|| {
        let source = fs::read_to_string(path).map_err(|error| {
            CompileError::new(
                format!("failed to load `{}`: {error}", path.display()),
                Span::default(),
            )
        })?;
        let source_id = sources.register_source(path, &source);
        if require_entry && project.manifest_path().is_some() {
            sources.set_entry_source(source_id);
        }
        let mut program = sources
            .parse(source_id, macros::STANDARD_NATIVE_MACROS)
            .map_err(|error| CompileError::new(error.to_string(), error.span()))?;
        rils_driver::load_file_modules(
            &mut program,
            path,
            project,
            macros::STANDARD_NATIVE_MACROS,
            &mut sources,
            require_entry,
        )
        .map_err(|error| CompileError::new(error.to_string(), error.span()))?;
        sources.analyze_project(host);
        bytecode::compile_program_with_host_and_session(
            host,
            sources.session(),
            sources.project_id(),
        )
    })();
    result.map_err(|error| locate_compile_error(error, &sources))
}

fn locate_compile_error(error: CompileError, sources: &ProjectSources) -> CompileError {
    if error.source_name().is_some() {
        return error;
    }
    let Some((source_name, source)) = sources.location(error.span.source) else {
        return error;
    };
    error.with_source(source_name, source)
}
