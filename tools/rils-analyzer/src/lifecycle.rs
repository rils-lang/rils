//! lifecycle for the analyzer service.
use super::*;

pub(super) fn start() -> Result<(), AnyError> {
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
            "triggerCharacters": [":", ".", "#", "!", " "]
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
    // Close the outgoing channel before waiting for its writer thread.
    drop(server);
    io_threads.join()?;
    Ok(())
}

impl Server {
    pub(super) fn run(&mut self) -> Result<(), AnyError> {
        let mut shutting_down = false;
        while let Ok(message) = self.connection.receiver.recv() {
            match message {
                Message::Request(request) => {
                    if shutting_down {
                        self.connection
                            .sender
                            .send(Message::Response(Response::new_err(
                                request.id,
                                -32600,
                                "server is shutting down".into(),
                            )))?;
                        continue;
                    }
                    if request.method == "shutdown" {
                        self.connection
                            .sender
                            .send(Message::Response(Response::new_ok(request.id, Value::Null)))?;
                        shutting_down = true;
                        continue;
                    }
                    self.handle_request(request)?;
                }
                Message::Notification(notification) => {
                    if notification.method == "exit" {
                        return Ok(());
                    }
                    if shutting_down {
                        continue;
                    }
                    let method = notification.method.clone();
                    if let Err(error) = self.handle_notification(notification) {
                        self.show_workspace_error(format!(
                            "Rils notification `{method}` failed: {error}"
                        ))?;
                    }
                }
                Message::Response(_) => {}
            }
        }
        Ok(())
    }

    pub(super) fn handle_notification(
        &mut self,
        notification: Notification,
    ) -> Result<(), AnyError> {
        match notification.method.as_str() {
            "textDocument/didOpen" => {
                let uri = string_at(&notification.params, &["textDocument", "uri"])?;
                let text = string_at(&notification.params, &["textDocument", "text"])?;
                self.update_document(uri, text)?;
            }
            "textDocument/didChange" => {
                let uri = string_at(&notification.params, &["textDocument", "uri"])?;
                let changes = notification
                    .params
                    .get("contentChanges")
                    .and_then(Value::as_array)
                    .ok_or_else(|| invalid_data("missing contentChanges array"))?;
                let mut text = None;
                for change in changes {
                    if change.get("range").is_some() {
                        return Err(invalid_data(
                            "incremental changes are unsupported; send full document text",
                        ));
                    }
                    text = Some(string_at(change, &["text"])?);
                }
                let Some(text) = text else {
                    return Ok(());
                };
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
                let paths = host_manifests::manifest_paths(&notification.params)?;
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

    pub(super) fn handle_request(&self, request: Request) -> Result<(), AnyError> {
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
            Err(error) => {
                let code = if error
                    .downcast_ref::<io::Error>()
                    .is_some_and(|error| error.kind() == io::ErrorKind::InvalidData)
                {
                    -32602
                } else {
                    -32603
                };
                Response::new_err(request.id, code, error.to_string())
            }
        };
        self.connection.sender.send(Message::Response(response))?;
        Ok(())
    }
}
