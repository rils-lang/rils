use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
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
use rils_frontend::{
    analyze_program_with_host_and_source_id_and_external_exports,
    analyze_with_host_and_source_id_and_external_exports,
};
use rils_host::{HOST_CONTRACT_ABI_VERSION, HostContract};
use rils_project::{LanguagePackageKind, Project, ProjectOrigin};
use serde_json::{Value, json};

type AnyError = Box<dyn Error + Send + Sync>;

mod project_index;

struct Document {
    source_id: SourceId,
    text: String,
    analysis: Result<DocumentAnalysis, FrontendError>,
}

fn main() -> Result<(), AnyError> {
    let (connection, io_threads) = Connection::stdio();
    let capabilities = json!({
        "textDocumentSync": 1,
        "definitionProvider": true,
        "referencesProvider": true,
        "hoverProvider": true,
        "signatureHelpProvider": {
            "triggerCharacters": ["(", ","]
        },
        "completionProvider": {
            "triggerCharacters": [":", "."]
        },
        "inlayHintProvider": true,
        "documentSymbolProvider": true,
        "semanticTokensProvider": {
            "legend": {
                "tokenTypes": [
                    "variable", "parameter", "function", "type", "class",
                    "enum", "interface", "property", "method", "enumMember", "namespace",
                    "keyword"
                ],
                "tokenModifiers": ["declaration"]
            },
            "full": true
        }
    });
    let initialization = connection.initialize(capabilities)?;

    let mut server = Server {
        connection,
        documents: HashMap::new(),
        workspace_documents: HashSet::new(),
        host_contract: HostContract::new(),
        host_functions: HashMap::new(),
        host_types: HashSet::new(),
        projects: Vec::new(),
        compilation: CompilationSession::default(),
        next_source_id: 1,
    };
    server.load_projects(&initialization)?;
    server.load_host_manifests(&initialization)?;
    server.load_workspace()?;
    server.run()?;
    io_threads.join()?;
    Ok(())
}

struct Server {
    connection: Connection,
    documents: HashMap<String, Document>,
    workspace_documents: HashSet<String>,
    host_contract: HostContract,
    host_functions: HashMap<String, FunctionSignature>,
    host_types: HashSet<String>,
    projects: Vec<Project>,
    compilation: CompilationSession,
    next_source_id: u32,
}

fn project_session_name(project: &Project) -> String {
    project.root().to_string_lossy().into_owned()
}

impl Server {
    /// Analyze the current revision, falling back to the last valid syntax
    /// tree while an editor is in the middle of an invalid edit. The fallback
    /// is deliberately kept in `SourceDatabase` and never used by compiler
    /// entry points, so diagnostics still report the current syntax error.
    fn analyze_source(
        &self,
        source_id: SourceId,
        external_exports: &HashMap<String, Vec<rils_frontend::analysis::ExternalModuleExport>>,
    ) -> Result<DocumentAnalysis, FrontendError> {
        match self.parse_source(source_id) {
            Ok(program) => Ok(
                analyze_program_with_host_and_source_id_and_external_exports(
                    &program,
                    source_id,
                    &self.host_contract,
                    external_exports,
                ),
            ),
            Err(error) => {
                let Some(program) = self.compilation.sources().last_valid_parse(source_id) else {
                    return Err(error);
                };
                let mut analysis = analyze_program_with_host_and_source_id_and_external_exports(
                    &program,
                    source_id,
                    &self.host_contract,
                    external_exports,
                );
                analysis
                    .diagnostics
                    .push(AnalysisDiagnostic::error(error.to_string(), error.span()));
                Ok(analysis)
            }
        }
    }

    fn parse_capabilities_for_uri(&self, uri: &str) -> ParseCapabilities {
        if self.projects.iter().any(|project| {
            matches!(project.origin(), ProjectOrigin::Language(_))
                && (project
                    .modules()
                    .any(|file| path_to_file_uri(&file.path) == uri)
                    || project
                        .prelude()
                        .is_some_and(|path| path_to_file_uri(path) == uri))
        }) {
            ParseCapabilities::STANDARD_LIBRARY
        } else {
            ParseCapabilities::USER
        }
    }

    fn project_for_source(&self, source_id: SourceId) -> Option<&Project> {
        let uri = self
            .compilation
            .sources()
            .source_file(source_id)
            .map(|file| file.name.as_str())?;
        self.projects.iter().find(|project| {
            project
                .modules()
                .any(|file| path_to_file_uri(&file.path) == uri)
                || project
                    .prelude()
                    .is_some_and(|path| path_to_file_uri(path) == uri)
        })
    }

    pub(crate) fn parse_tokens(
        &self,
        source_id: SourceId,
        tokens: Vec<rils_frontend::token::Token>,
    ) -> Result<rils_frontend::ast::Program, rils_frontend::parser::ParseError> {
        let capabilities = self
            .compilation
            .sources()
            .parse_capabilities(source_id)
            .unwrap_or(ParseCapabilities::USER);
        rils_frontend::parse_with_capabilities(
            tokens,
            rils_frontend::macros::STANDARD_NATIVE_MACROS,
            capabilities,
        )
    }

    pub(crate) fn parse_source(
        &self,
        source_id: SourceId,
    ) -> Result<rils_frontend::ast::Program, FrontendError> {
        self.compilation
            .sources()
            .try_parse(source_id)
            .ok_or_else(|| {
                FrontendError::Parse(rils_frontend::parser::ParseError {
                    message: "source is not available".into(),
                    span: Span::new(0, 0),
                })
            })?
    }

    fn load_projects(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut seen = HashSet::new();
        self.projects.clear();
        let stdlib_root = match language_package_root() {
            Ok(root) => Some(root),
            Err(error) => {
                self.show_workspace_error(error)?;
                None
            }
        };
        if let Some(root) = stdlib_root.as_deref() {
            match Project::from_language_package(
                root.join(rils_project::PROJECT_FILE_NAME),
                LanguagePackageKind::StandardLibrary,
            ) {
                Ok(project) => {
                    seen.insert(project.root().to_path_buf());
                    self.projects.push(project);
                }
                Err(error) => {
                    self.show_workspace_error(format!("failed to load standard library: {error}"))?
                }
            }
        }
        for root in workspace_roots(initialization) {
            match workspace::workspace_projects_with_language(&root, stdlib_root.as_deref()) {
                Ok(load) => {
                    for error in load.errors {
                        self.show_workspace_error(error)?;
                    }
                    self.projects
                        .extend(load.projects.into_iter().filter_map(|mut project| {
                            if !seen.insert(project.root().to_path_buf()) {
                                return None;
                            }
                            if stdlib_root.is_some() {
                                project
                                    .add_language_dependency(LanguagePackageKind::StandardLibrary);
                            }
                            Some(project)
                        }));
                }
                Err(error) => self.show_workspace_error(format!(
                    "failed to load workspace `{}`: {error}",
                    root.display()
                ))?,
            }
        }
        self.compilation.clear_projects();
        Ok(())
    }

    fn show_workspace_error(&self, message: String) -> Result<(), AnyError> {
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                "window/showMessage".to_owned(),
                json!({"type": 1, "message": message}),
            )))?;
        Ok(())
    }

    fn run(&mut self) -> Result<(), AnyError> {
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if self.connection.handle_shutdown(&request)? {
                        return Ok(());
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => {
                    self.handle_notification(notification)?;
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    fn handle_notification(&mut self, notification: Notification) -> Result<(), AnyError> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let uri = string_at(&notification.params, &["textDocument", "uri"])?;
                let text = string_at(&notification.params, &["textDocument", "text"])?;
                self.update_document(uri, text)?;
            }
            "textDocument/didChange" => {
                let uri = string_at(&notification.params, &["textDocument", "uri"])?;
                let text = notification
                    .params
                    .pointer("/contentChanges/0/text")
                    .and_then(Value::as_str)
                    .ok_or_else(|| invalid_data("missing full document change"))?
                    .to_owned();
                self.update_document(uri, text)?;
            }
            "textDocument/didClose" => {
                let uri = normalize_document_uri(&string_at(
                    &notification.params,
                    &["textDocument", "uri"],
                )?);
                if !self.workspace_documents.contains(&uri) {
                    self.documents.remove(&uri);
                }
                self.publish_diagnostics(&uri, Vec::new())?;
            }
            "rils/hostManifestChanged" => {
                let paths = notification
                    .params
                    .get("hostManifestPaths")
                    .and_then(Value::as_array)
                    .map(|paths| {
                        paths
                            .iter()
                            .filter_map(Value::as_str)
                            .map(PathBuf::from)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if let Err(error) = self.reload_host_manifests(paths) {
                    self.connection
                        .sender
                        .send(Message::Notification(Notification::new(
                            "window/showMessage".to_owned(),
                            json!({
                                "type": 1,
                                "message": format!("Rils host manifest reload failed: {error}"),
                            }),
                        )))?;
                } else {
                    self.reanalyze_documents();
                    self.refresh_project_symbol_links();
                    self.publish_all_diagnostics()?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn update_document(&mut self, uri: String, text: String) -> Result<(), AnyError> {
        let uri = normalize_document_uri(&uri);
        let source_id = self.source_id_for_uri(&uri);
        let capabilities = self
            .compilation
            .sources()
            .parse_capabilities(source_id)
            .unwrap_or_else(|| self.parse_capabilities_for_uri(&uri));
        self.compilation
            .sources_mut()
            .set_source_with_id_and_capabilities(
                source_id,
                uri.clone(),
                text.clone(),
                capabilities,
            );
        let analysis = self.analyze_source(source_id, &HashMap::new());
        self.documents.insert(
            uri.clone(),
            Document {
                source_id,
                text,
                analysis,
            },
        );
        self.reanalyze_documents();
        self.refresh_project_symbol_links();
        self.publish_all_diagnostics()
    }

    fn source_id_for_uri(&mut self, uri: &str) -> SourceId {
        if let Some(document) = self.documents.get(uri) {
            return document.source_id;
        }
        let source_id = SourceId::new(self.next_source_id);
        self.next_source_id = self
            .next_source_id
            .checked_add(1)
            .expect("source id overflow");
        source_id
    }

    fn parsed_document(&self, document: &Document) -> Option<rils_frontend::ast::Program> {
        if self
            .compilation
            .sources()
            .source_text(document.source_id)
            .is_some_and(|source| source == document.text)
        {
            return self.parse_source(document.source_id).ok();
        }
        let tokens = lex_with_source_id(&document.text, document.source_id).ok()?;
        self.parse_tokens(document.source_id, tokens).ok()
    }

    fn load_host_manifests(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut paths = initialization
            .pointer("/initializationOptions/hostManifestPaths")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(PathBuf::from)
            .collect::<Vec<_>>();
        if paths.is_empty() {
            for project in &self.projects {
                paths.extend(project.host_manifests().iter().cloned());
            }
        }
        self.reload_host_manifests(paths)
    }

    fn reload_host_manifests(&mut self, paths: Vec<PathBuf>) -> Result<(), AnyError> {
        let mut paths = paths;
        paths.sort();
        paths.dedup();
        let mut merged: Option<HostContract> = None;
        for path in paths {
            let bytes = fs::read(&path).map_err(|error| {
                invalid_data(format!(
                    "failed to read host manifest `{}`: {error}",
                    path.display()
                ))
            })?;
            let contract = HostContract::from_manifest_bytes(&bytes).map_err(|error| {
                invalid_data(format!(
                    "invalid host manifest `{}`: {error}",
                    path.display()
                ))
            })?;
            if contract.host_abi_version() != HOST_CONTRACT_ABI_VERSION {
                return Err(invalid_data(format!(
                    "host manifest `{}` uses ABI {}, but analyzer supports ABI {HOST_CONTRACT_ABI_VERSION}",
                    path.display(),
                    contract.host_abi_version()
                )));
            }
            if let Some(target) = &mut merged {
                target.merge(&contract).map_err(invalid_data)?;
            } else {
                merged = Some(contract);
            }
        }
        self.host_contract = merged.unwrap_or_default();
        self.host_functions = self.host_contract.signatures();
        self.host_types = self
            .host_contract
            .types()
            .map(|declaration| declaration.name.clone())
            .collect();
        Ok(())
    }

    fn load_workspace(&mut self) -> Result<(), AnyError> {
        let files = self
            .projects
            .iter()
            .flat_map(|project| project.modules().map(|file| file.path.clone()))
            .chain(
                self.projects
                    .iter()
                    .filter_map(|project| project.prelude().map(Path::to_path_buf)),
            )
            .collect::<HashSet<_>>();
        for path in files {
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let uri = path_to_file_uri(&path);
            let source_id = self.source_id_for_uri(&uri);
            let capabilities = self.parse_capabilities_for_uri(&uri);
            self.compilation
                .sources_mut()
                .set_source_with_id_and_capabilities(
                    source_id,
                    uri.clone(),
                    text.clone(),
                    capabilities,
                );
            self.workspace_documents.insert(uri.clone());
            self.documents.insert(
                uri,
                Document {
                    source_id,
                    analysis: self.analyze_source(source_id, &HashMap::new()),
                    text,
                },
            );
        }
        self.reanalyze_documents();
        self.refresh_project_symbol_links();
        Ok(())
    }

    fn reanalyze_documents(&mut self) {
        for (uri, document) in &self.documents {
            let capabilities = self
                .compilation
                .sources()
                .parse_capabilities(document.source_id)
                .unwrap_or_else(|| self.parse_capabilities_for_uri(uri));
            self.compilation
                .sources_mut()
                .set_source_with_id_and_capabilities(
                    document.source_id,
                    uri.clone(),
                    document.text.clone(),
                    capabilities,
                );
        }
        self.rebuild_project_semantics(None);
        let exports = project_index::collect_external_exports(self);
        let analyses = self
            .documents
            .values()
            .map(|document| {
                let external_exports = self
                    .project_for_source(document.source_id)
                    .map(|project| exports.for_project(self, project, true))
                    .unwrap_or_default();
                (
                    document.source_id,
                    self.analyze_source(document.source_id, &external_exports),
                )
            })
            .collect::<HashMap<_, _>>();
        for document in self.documents.values_mut() {
            document.analysis = analyses
                .get(&document.source_id)
                .cloned()
                .expect("analysis collected for every document");
        }
        self.rebuild_project_semantics(Some(&exports));
    }

    fn rebuild_project_semantics(
        &mut self,
        inherited_exports: Option<&project_index::ProjectExportIndex>,
    ) {
        self.compilation.clear_projects();
        for project in &self.projects {
            self.compilation
                .register_project(project_session_name(project));
        }
        let standard_library = self
            .projects
            .iter()
            .find(|project| {
                project.origin() == ProjectOrigin::Language(LanguagePackageKind::StandardLibrary)
            })
            .and_then(|project| self.compilation.project_id(&project_session_name(project)));
        for project in &self.projects {
            let project_id = self
                .compilation
                .register_project(project_session_name(project));
            let mut index = ProjectSemanticIndex::default();
            for dependency in project.language_dependencies() {
                match dependency {
                    LanguagePackageKind::StandardLibrary => {
                        if let Some(dependency) = standard_library {
                            index.add_dependency(dependency);
                        }
                    }
                }
            }
            let mut programs = Vec::new();
            for file in project.modules() {
                if let Some(document) = self.documents.get(&path_to_file_uri(&file.path)) {
                    let module = index.register(&file.module_path, document.source_id);
                    if let Some(program) =
                        self.parse_source(document.source_id).ok().or_else(|| {
                            self.compilation
                                .sources()
                                .last_valid_parse(document.source_id)
                        })
                    {
                        programs.push((module, program));
                    }
                }
            }
            let prelude_program = project.prelude().and_then(|path| {
                self.documents
                    .get(&path_to_file_uri(path))
                    .and_then(|document| {
                        self.parse_source(document.source_id).ok().or_else(|| {
                            self.compilation
                                .sources()
                                .last_valid_parse(document.source_id)
                        })
                    })
            });
            self.compilation.replace_project(project_id, index);
            let syntax = self
                .compilation
                .project_syntax_mut(project_id)
                .expect("registered project must have syntax storage");
            if let Some(program) = prelude_program {
                syntax.push_root(program);
            }
            for (module, program) in programs {
                syntax.insert_module(module, program);
            }

            let analysis = {
                let semantics = self
                    .compilation
                    .project(project_id)
                    .expect("registered project must have semantic storage");
                let syntax = self
                    .compilation
                    .project_syntax(project_id)
                    .expect("registered project must have syntax storage");
                let language_dependency = project
                    .language_dependencies()
                    .any(|dependency| dependency == LanguagePackageKind::StandardLibrary);
                if language_dependency {
                    if let Some(exports) = inherited_exports {
                        let language_exports = exports.dependencies_for_project(self, project);
                        rils_frontend::analyze_project_with_host_and_external_exports(
                            syntax,
                            semantics.module_graph(),
                            &self.host_contract,
                            &language_exports,
                        )
                    } else {
                        rils_frontend::analyze_project_with_host(
                            syntax,
                            semantics.module_graph(),
                            &self.host_contract,
                        )
                    }
                } else {
                    rils_frontend::analyze_project_with_host(
                        syntax,
                        semantics.module_graph(),
                        &self.host_contract,
                    )
                }
            };
            self.compilation
                .project_mut(project_id)
                .expect("registered project must have semantic storage")
                .index_def_map(&analysis.def_map);
            self.compilation
                .set_project_analysis(project_id, &self.host_contract, analysis);
        }
    }

    fn project_semantics(&self, project: &Project) -> Option<&ProjectSemanticIndex> {
        self.compilation
            .project_id(&project_session_name(project))
            .and_then(|id| self.compilation.project(id))
    }

    fn project_analysis(&self, project: &Project) -> Option<&DocumentAnalysis> {
        self.compilation
            .project_id(&project_session_name(project))
            .and_then(|id| self.compilation.project_analysis(id, &self.host_contract))
    }

    fn document_uri_for_source(&self, source: SourceId) -> Option<&str> {
        self.documents
            .iter()
            .find(|(_, document)| document.source_id == source)
            .map(|(uri, _)| uri.as_str())
    }

    fn project_definition_by_id(&self, id: rils_frontend::DefId) -> Option<&DefinitionData> {
        if let Some(definition) = self.projects.iter().find_map(|project| {
            self.project_analysis(project)
                .and_then(|analysis| analysis.def_map.definition(id))
        }) {
            return Some(definition);
        }
        self.compilation
            .projects()
            .find_map(|index| index.definition(id))
    }

    fn publish_all_diagnostics(&mut self) -> Result<(), AnyError> {
        let diagnostics = self
            .documents
            .iter()
            .map(|(uri, document)| (uri.clone(), diagnostics(&document.text, &document.analysis)))
            .collect::<Vec<_>>();
        for (uri, diagnostics) in diagnostics {
            self.publish_diagnostics(&uri, diagnostics)?;
        }
        Ok(())
    }

    fn refresh_project_symbol_links(&mut self) {
        let mut links = Vec::new();
        for (uri, document) in &self.documents {
            let Some(document_analysis) = analysis(document) else {
                continue;
            };
            for (index, symbol) in document_analysis.symbols.iter().enumerate() {
                if symbol.is_definition || symbol.definition_id.is_some() {
                    continue;
                }
                if let Some(target) =
                    self.project_symbol_id(uri, document, symbol.span.start, symbol.kind)
                {
                    links.push((uri.clone(), index, target));
                }
            }
        }
        for (uri, index, target) in links {
            let Some(Ok(analysis)) = self
                .documents
                .get_mut(&uri)
                .map(|document| &mut document.analysis)
            else {
                continue;
            };
            if let Some(symbol) = analysis.symbols.get_mut(index) {
                symbol.definition_id = Some(target);
            }
        }
    }

    fn publish_diagnostics(&self, uri: &str, diagnostics: Vec<Value>) -> Result<(), AnyError> {
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/publishDiagnostics".to_owned(),
                json!({ "uri": uri, "diagnostics": diagnostics }),
            )))?;
        Ok(())
    }

    fn handle_request(&self, request: Request) -> Result<(), AnyError> {
        let result = match request.method.as_str() {
            "textDocument/definition" => self.definition(&request.params),
            "textDocument/references" => self.references(&request.params),
            "textDocument/hover" => self.hover(&request.params),
            "textDocument/signatureHelp" => self.signature_help(&request.params),
            "textDocument/completion" => self.completion(&request.params),
            "textDocument/inlayHint" => self.inlay_hints(&request.params),
            "textDocument/documentSymbol" => self.document_symbols(&request.params),
            "textDocument/semanticTokens/full" => self.semantic_tokens(&request.params),
            _ => {
                self.connection
                    .sender
                    .send(Message::Response(Response::new_err(
                        request.id,
                        -32601,
                        format!("unsupported request: {}", request.method),
                    )))?;
                return Ok(());
            }
        };

        let response = match result {
            Ok(value) => Response::new_ok(request.id, value),
            Err(error) => Response::new_err(request.id, -32603, error.to_string()),
        };
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }

    fn document<'a>(&'a self, params: &Value) -> Result<(String, &'a Document), AnyError> {
        let uri = normalize_document_uri(&string_at(params, &["textDocument", "uri"])?);
        let document = self
            .documents
            .get(&uri)
            .ok_or_else(|| invalid_data(format!("document is not open: {uri}")))?;
        Ok((uri, document))
    }

    fn document_and_offset<'a>(
        &'a self,
        params: &Value,
    ) -> Result<(String, &'a Document, usize), AnyError> {
        let (uri, document) = self.document(params)?;
        let line = u32_at(params, &["position", "line"])?;
        let character = u32_at(params, &["position", "character"])?;
        let offset = offset(&document.text, line, character);
        Ok((uri, document, offset))
    }
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
