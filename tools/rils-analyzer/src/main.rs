use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
use rils_frontend::{
    DefinitionData, FrontendError, FunctionSignature, ProjectSemanticIndex, SourceDatabase,
    SourceId, Span, Type,
    analysis::{
        DiagnosticSeverity, DocumentAnalysis, SymbolContainer, SymbolKind, SymbolOccurrence,
    },
    ast::Stmt,
    lexer::{lex, lex_with_source_id},
    parser::parse,
};
use rils_frontend::{
    analyze_program_with_host_and_source_id_and_external_exports,
    analyze_with_host_and_source_id_and_external_exports,
};
use rils_host::{HOST_CONTRACT_ABI_VERSION, HostContract};
use rils_project::Project;
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
        project_semantics: HashMap::new(),
        next_source_id: 1,
        sources: SourceDatabase::default(),
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
    project_semantics: HashMap<PathBuf, ProjectSemanticIndex>,
    next_source_id: u32,
    sources: SourceDatabase,
}

impl Server {
    fn load_projects(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut seen = HashSet::new();
        self.projects = workspace_roots(initialization)
            .into_iter()
            .map(|root| workspace_projects(&root))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .filter(|project| seen.insert(project.root().to_path_buf()))
            .collect();
        self.project_semantics.clear();
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
        self.sources
            .set_source_with_id(source_id, uri.clone(), text.clone());
        let analysis = self.sources.parse(source_id).map(|program| {
            analyze_program_with_host_and_source_id_and_external_exports(
                &program,
                source_id,
                &self.host_contract,
                &HashMap::new(),
            )
        });
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
        self.sources
            .source_text(document.source_id)
            .filter(|source| *source == document.text)
            .and_then(|_| self.sources.try_parse(document.source_id))
            .unwrap_or_else(|| {
                let tokens = lex_with_source_id(&document.text, document.source_id)
                    .map_err(FrontendError::Lex)?;
                parse(tokens).map_err(FrontendError::Parse)
            })
            .ok()
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
            .flat_map(|project| project.modules().cloned())
            .collect::<Vec<_>>();
        for project_file in files {
            let path = &project_file.path;
            let Ok(text) = fs::read_to_string(path) else {
                continue;
            };
            let uri = path_to_file_uri(path);
            let source_id = self.source_id_for_uri(&uri);
            self.sources
                .set_source_with_id(source_id, uri.clone(), text.clone());
            self.workspace_documents.insert(uri.clone());
            self.documents.insert(
                uri,
                Document {
                    source_id,
                    analysis: self.sources.parse(source_id).map(|program| {
                        analyze_program_with_host_and_source_id_and_external_exports(
                            &program,
                            source_id,
                            &self.host_contract,
                            &HashMap::new(),
                        )
                    }),
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
            self.sources
                .set_source_with_id(document.source_id, uri.clone(), document.text.clone());
        }
        self.rebuild_project_semantics();
        let exports = project_index::collect_external_exports(self);
        for document in self.documents.values_mut() {
            document.analysis = self.sources.parse(document.source_id).map(|program| {
                analyze_program_with_host_and_source_id_and_external_exports(
                    &program,
                    document.source_id,
                    &self.host_contract,
                    &exports,
                )
            });
        }
        self.rebuild_project_semantics();
    }

    fn rebuild_project_semantics(&mut self) {
        self.project_semantics = self
            .projects
            .iter()
            .map(|project| {
                let mut index = ProjectSemanticIndex::default();
                for file in project.modules() {
                    if let Some(document) = self.documents.get(&path_to_file_uri(&file.path)) {
                        index.register(&file.module_path, document.source_id);
                        if let Ok(analysis) = &document.analysis {
                            index.index_def_map(&analysis.def_map);
                        }
                    }
                }
                (project.root().to_path_buf(), index)
            })
            .collect();
    }

    fn project_semantics(&self, project: &Project) -> Option<&ProjectSemanticIndex> {
        self.project_semantics.get(project.root())
    }

    fn document_uri_for_source(&self, source: SourceId) -> Option<&str> {
        self.documents
            .iter()
            .find(|(_, document)| document.source_id == source)
            .map(|(uri, _)| uri.as_str())
    }

    fn project_definition_by_id(&self, id: rils_frontend::DefId) -> Option<&DefinitionData> {
        self.project_semantics
            .values()
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

fn workspace_projects(root: &Path) -> Result<Vec<Project>, AnyError> {
    let manifest = root.join("rils.toml");
    if manifest.is_file() {
        return Ok(vec![Project::from_file(manifest)?]);
    }

    let mut projects = vec![Project::from_root(root)?];
    let mut manifests = Vec::new();
    collect_nested_project_manifests(root, &mut manifests)?;
    manifests.sort_by(|left, right| {
        left.components()
            .count()
            .cmp(&right.components().count())
            .then_with(|| left.cmp(right))
    });

    let mut configured_roots = Vec::new();
    for manifest in manifests {
        let project_root = manifest
            .parent()
            .expect("project manifest always has a parent");
        if configured_roots
            .iter()
            .any(|configured_root: &PathBuf| project_root.starts_with(configured_root))
        {
            continue;
        }
        let project = Project::from_file(&manifest)?;
        configured_roots.push(project.root().to_path_buf());
        projects.push(project);
    }
    Ok(projects)
}

fn collect_nested_project_manifests(
    root: &Path,
    manifests: &mut Vec<PathBuf>,
) -> std::io::Result<()> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() {
            continue;
        }
        if matches!(
            entry.file_name().to_str(),
            Some(".git" | ".rils" | "target" | "node_modules" | "dist" | "Library")
        ) {
            continue;
        }
        let manifest = path.join("rils.toml");
        if manifest.is_file() {
            manifests.push(manifest);
            continue;
        }
        collect_nested_project_manifests(&path, manifests)?;
    }
    Ok(())
}

mod completion;
mod navigation;
mod signature_help;
mod support;
mod symbols;

use support::*;

#[cfg(test)]
#[path = "../tests/unit/analyzer.rs"]
mod tests;
