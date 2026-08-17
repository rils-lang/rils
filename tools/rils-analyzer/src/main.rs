use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
use rils_compiler::{HOST_CONTRACT_ABI_VERSION, HostContract};
use rils_frontend::{
    FrontendError, FunctionSignature, SourceId, Span, Type,
    analysis::{
        DiagnosticSeverity, DocumentAnalysis, SymbolKind, analyze_with_source_id,
        analyze_with_source_id_and_external_exports,
    },
    ast::Stmt,
    lexer::{lex, lex_with_source_id},
    parser::parse,
};
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
                    "enum", "interface", "property", "method", "enumMember", "namespace"
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
        projects: Vec::new(),
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
    projects: Vec<Project>,
    next_source_id: u32,
}

impl Server {
    fn load_projects(&mut self, initialization: &Value) -> Result<(), AnyError> {
        let mut seen = HashSet::new();
        self.projects = workspace_roots(initialization)
            .into_iter()
            .map(|root| {
                Project::discover(&root, Some(&root))
                    .map_err(|error| invalid_data(error.to_string()))
            })
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|project| seen.insert(project.root().to_path_buf()))
            .collect();
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
                let uri = string_at(&notification.params, &["textDocument", "uri"])?;
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
        let source_id = self.source_id_for_uri(&uri);
        self.documents.insert(
            uri.clone(),
            Document {
                source_id,
                text,
                analysis: analyze_with_source_id("", source_id, &self.host_functions),
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
        self.host_functions = self
            .host_contract
            .functions()
            .map(|function| (function.name.clone(), function.signature.clone()))
            .collect();
        for function in self.host_contract.functions() {
            if function.receiver.is_some()
                && let Some((_, method)) = function.name.rsplit_once("::")
            {
                self.host_functions
                    .insert(format!("HostHandle::{method}"), function.signature.clone());
            }
        }
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
            self.workspace_documents.insert(uri.clone());
            self.documents.insert(
                uri,
                Document {
                    source_id,
                    analysis: analyze_with_source_id(&text, source_id, &self.host_functions),
                    text,
                },
            );
        }
        self.reanalyze_documents();
        self.refresh_project_symbol_links();
        Ok(())
    }

    fn reanalyze_documents(&mut self) {
        let exports = project_index::collect_external_exports(self);
        for document in self.documents.values_mut() {
            document.analysis = analyze_with_source_id_and_external_exports(
                &document.text,
                document.source_id,
                &self.host_functions,
                &exports,
            );
        }
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
        let uri = string_at(params, &["textDocument", "uri"])?;
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

use support::*;

#[cfg(test)]
mod tests;
