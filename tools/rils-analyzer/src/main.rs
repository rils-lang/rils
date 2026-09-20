use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
use rils_frontend::analyze_with_host_and_source_id_and_external_exports;
use rils_frontend::{
    CompilationSession, DefinitionData, FrontendError, FunctionSignature, ProjectSemanticIndex,
    SourceId, Span, Type,
    analysis::{
        AnalysisDiagnostic, DiagnosticSeverity, DocumentAnalysis, SymbolContainer, SymbolKind,
        SymbolOccurrence,
    },
    ast::Stmt,
    lexer::{lex, lex_with_source_id},
    parser::{ParseCapabilities, parse},
};
use rils_host::{HOST_CONTRACT_ABI_VERSION, HostContract};
use rils_project::{LanguagePackageKind, Project, ProjectOrigin};
use serde_json::{Value, json};

type AnyError = Box<dyn Error + Send + Sync>;

mod document_analysis;
mod document_state;
mod host_manifests;
mod lifecycle;
mod server_state;
use server_state::{Document, Server};
mod project_completion;
mod project_index;
mod project_semantics;
mod workspace_loading;

fn main() -> Result<(), AnyError> {
    lifecycle::start()
}

fn project_session_name(project: &Project) -> String {
    project.root().to_string_lossy().into_owned()
}

mod completion;
mod navigation;
mod signature_help;
mod support;
mod symbols;
mod workspace;

use support::*;

use workspace::language_package_root;
#[cfg(test)]
use workspace::workspace_projects;

#[cfg(test)]
#[path = "../tests/unit/analyzer.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/unit/language_package.rs"]
mod language_package_tests;

#[cfg(test)]
#[path = "../tests/unit/workspace.rs"]
mod workspace_tests;

#[cfg(test)]
#[path = "../tests/unit/reexports.rs"]
mod reexport_tests;

#[cfg(test)]
#[path = "../tests/unit/edit_revisions.rs"]
mod edit_revision_tests;
