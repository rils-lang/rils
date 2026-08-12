use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
use rils_compiler::{HOST_CONTRACT_ABI_VERSION, HostContract};
use rils_frontend::{
    FrontendError, FunctionSignature, Span, Type,
    analysis::{DiagnosticSeverity, DocumentAnalysis, SymbolKind, analyze_with_host_functions},
    ast::Stmt,
    lexer::lex,
    parser::parse,
};
use rils_project::Project;
use serde_json::{Value, json};

type AnyError = Box<dyn Error + Send + Sync>;

struct Document {
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
        "completionProvider": {
            "triggerCharacters": [":"]
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
            _ => {}
        }
        Ok(())
    }

    fn update_document(&mut self, uri: String, text: String) -> Result<(), AnyError> {
        let analysis = analyze_with_host_functions(&text, &self.host_functions);
        let diagnostics = diagnostics(&text, &analysis);
        self.documents
            .insert(uri.clone(), Document { text, analysis });
        self.publish_diagnostics(&uri, diagnostics)
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
        Ok(())
    }

    fn load_workspace(&mut self) -> Result<(), AnyError> {
        for project in &self.projects {
            for project_file in project.modules() {
                let path = &project_file.path;
                let Ok(text) = fs::read_to_string(path) else {
                    continue;
                };
                let uri = path_to_file_uri(path);
                self.workspace_documents.insert(uri.clone());
                self.documents.insert(
                    uri,
                    Document {
                        analysis: analyze_with_host_functions(&text, &self.host_functions),
                        text,
                    },
                );
            }
        }
        Ok(())
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

    fn definition(&self, params: &Value) -> Result<Value, AnyError> {
        let (uri, document, offset) = self.document_and_offset(params)?;
        let Some(current_analysis) = analysis(document) else {
            return Ok(self
                .project_definition(&uri, document, offset)
                .unwrap_or(Value::Null));
        };
        let Some(symbol) = current_analysis
            .symbols
            .iter()
            .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        else {
            return Ok(self
                .project_definition(&uri, document, offset)
                .unwrap_or(Value::Null));
        };
        if let Some(definition_span) = symbol.definition_span {
            return Ok(json!({
                "uri": uri,
                "range": range(&document.text, definition_span)
            }));
        }
        if let Some(definition) = self.project_definition(&uri, document, offset) {
            return Ok(definition);
        }
        for (candidate_uri, candidate_document) in &self.documents {
            let Some(candidate_analysis) = analysis(candidate_document) else {
                continue;
            };
            if let Some(definition) = candidate_analysis.symbols.iter().find(|candidate| {
                candidate.is_definition
                    && candidate.name == symbol.name
                    && compatible_symbol_kinds(candidate.kind, symbol.kind)
            }) {
                return Ok(json!({
                    "uri": candidate_uri,
                    "range": range(&candidate_document.text, definition.span)
                }));
            }
        }
        Ok(Value::Null)
    }

    fn project_definition(&self, uri: &str, document: &Document, offset: usize) -> Option<Value> {
        let path = file_uri_to_path(uri)?;
        let project = self
            .projects
            .iter()
            .find(|project| project.module_for_file(&path).is_some())?;
        let current = &project.module_for_file(&path)?.module_path;
        let qualified = qualified_path_at(&document.text, offset)?;
        let (qualifier, member) = qualified.rsplit_once("::")?;
        let qualifier = resolve_path_alias(&document.text, qualifier);
        let module_path = resolve_project_path(current, &qualifier)?;
        let file = project.module(&module_path)?;
        let target_uri = path_to_file_uri(&file.path);
        let owned_source;
        let source = if let Some(document) = self.documents.get(&target_uri) {
            document.text.as_str()
        } else {
            owned_source = fs::read_to_string(&file.path).ok()?;
            &owned_source
        };
        let program = parse(lex(source).ok()?).ok()?;
        let span = program.statements.iter().find_map(|statement| {
            let Stmt::Public { statement, .. } = statement else {
                return None;
            };
            declaration_name_span(statement, member)
        })?;
        Some(json!({
            "uri": target_uri,
            "range": range(source, span)
        }))
    }

    fn references(&self, params: &Value) -> Result<Value, AnyError> {
        let (_uri, document, offset) = self.document_and_offset(params)?;
        let Some(current_analysis) = analysis(document) else {
            return Ok(json!([]));
        };
        let Some(symbol) = current_analysis
            .symbols
            .iter()
            .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        else {
            return Ok(json!([]));
        };
        let include_declaration = params
            .pointer("/context/includeDeclaration")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        let locations = self
            .documents
            .iter()
            .flat_map(|(candidate_uri, candidate_document)| {
                analysis(candidate_document)
                    .into_iter()
                    .flat_map(|analysis| analysis.symbols.iter())
                    .filter(|candidate| {
                        candidate.name == symbol.name
                            && compatible_symbol_kinds(candidate.kind, symbol.kind)
                            && (include_declaration || !candidate.is_definition)
                    })
                    .map(|candidate| {
                        json!({
                            "uri": candidate_uri,
                            "range": range(&candidate_document.text, candidate.span)
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>();
        Ok(json!(locations))
    }

    fn hover(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document, offset) = self.document_and_offset(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(Value::Null);
        };
        let Some(symbol) = analysis
            .symbols
            .iter()
            .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        else {
            return Ok(Value::Null);
        };
        let detail = match (&symbol.detail, &symbol.inferred_type) {
            (Some(detail), _) => detail.clone(),
            (_, Some(inferred))
                if matches!(symbol.kind, SymbolKind::Function | SymbolKind::Method) =>
            {
                function_declaration(&symbol.name, inferred)
            }
            (_, Some(inferred)) if symbol.kind == SymbolKind::Parameter => {
                format!("parameter {}: {inferred}", symbol.name)
            }
            (_, Some(inferred)) if symbol.kind == SymbolKind::Variable => {
                format!("let {}: {inferred}", symbol.name)
            }
            _ => format!("{} {}", kind_label(symbol.kind), symbol.name),
        };
        Ok(json!({
            "contents": {
                "kind": "markdown",
                "value": format!("```rils\n{detail}\n```")
            },
            "range": range(&document.text, symbol.span)
        }))
    }

    fn completion(&self, params: &Value) -> Result<Value, AnyError> {
        let (uri, document, offset) = self.document_and_offset(params)?;
        if let Some((receiver, member_prefix)) = method_completion_target(&document.text, offset)
            && let Some(analysis) = analysis(document)
            && analysis.symbols.iter().any(|symbol| {
                symbol.name == receiver
                    && symbol.inferred_type.as_ref().is_some_and(Type::is_integer)
            })
        {
            let items = rils_builtins::INTEGER_INTRINSICS
                .iter()
                .filter(|item| {
                    item.kind == rils_builtins::IntrinsicKind::Method
                        && item.name.starts_with(&member_prefix)
                })
                .map(integer_intrinsic_completion)
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let Some((qualifier, member_prefix)) = completion_target(&document.text, offset) else {
            return Ok(json!([]));
        };
        if rils_builtins::IntegerType::from_name(&qualifier).is_some() {
            let items = rils_builtins::INTEGER_INTRINSICS
                .iter()
                .filter(|item| {
                    item.kind == rils_builtins::IntrinsicKind::AssociatedFunction
                        && item.name.starts_with(&member_prefix)
                })
                .map(integer_intrinsic_completion)
                .collect::<Vec<_>>();
            return Ok(json!(items));
        }
        let qualifier = resolve_path_alias(&document.text, &qualifier);
        let nested_prefix = format!("{qualifier}::");
        let mut module_names = HashSet::new();
        let mut items = Vec::new();

        for module in self.host_contract.modules() {
            let Some(remainder) = module.name.strip_prefix(&nested_prefix) else {
                continue;
            };
            let child = remainder.split("::").next().unwrap_or(remainder);
            if child.starts_with(&member_prefix) && module_names.insert(child.to_owned()) {
                let full_name = format!("{qualifier}::{child}");
                items.push(json!({
                    "label": child,
                    "kind": 9,
                    "detail": format!("host module {full_name}"),
                    "sortText": format!("0_{child}")
                }));
            }
        }
        for function in self.host_contract.functions() {
            let Ok((module, name)) = split_qualified_name(&function.name) else {
                continue;
            };
            if module != qualifier || !name.starts_with(&member_prefix) {
                continue;
            }
            let declaration = signature_declaration(name, &function.signature);
            items.push(json!({
                "label": name,
                "kind": 3,
                "detail": declaration,
                "documentation": {
                    "kind": "markdown",
                    "value": format!(
                        "```rils\n{}\n```\n\nHost capability: `{}`",
                        signature_declaration(&function.name, &function.signature),
                        function.capability
                    )
                },
                "sortText": format!("1_{name}")
            }));
        }
        self.add_project_completions(
            &uri,
            &qualifier,
            &member_prefix,
            &mut module_names,
            &mut items,
        );
        items.sort_by(|left, right| left["sortText"].as_str().cmp(&right["sortText"].as_str()));
        items.dedup_by(|left, right| left["label"] == right["label"]);
        Ok(json!(items))
    }

    fn add_project_completions(
        &self,
        uri: &str,
        qualifier: &str,
        member_prefix: &str,
        module_names: &mut HashSet<String>,
        items: &mut Vec<Value>,
    ) {
        let Some(path) = file_uri_to_path(uri) else {
            return;
        };
        let Some(project) = self
            .projects
            .iter()
            .find(|project| project.module_for_file(&path).is_some())
        else {
            return;
        };
        let current = project
            .module_for_file(&path)
            .map(|file| file.module_path.as_str())
            .unwrap_or_default();
        let Some(module_path) = resolve_project_path(current, qualifier) else {
            return;
        };
        let nested_prefix = if module_path.is_empty() {
            String::new()
        } else {
            format!("{module_path}::")
        };
        for file in project.modules() {
            if file.module_path == module_path {
                continue;
            }
            let Some(remainder) = file.module_path.strip_prefix(&nested_prefix) else {
                continue;
            };
            if remainder.is_empty() {
                continue;
            }
            let child = remainder.split("::").next().unwrap_or(remainder);
            if child.starts_with(member_prefix) && module_names.insert(child.to_owned()) {
                items.push(json!({
                    "label": child,
                    "kind": 9,
                    "detail": format!("module {}", join_module_path(&module_path, child)),
                    "sortText": format!("0_{child}")
                }));
            }
        }
        let Some(file) = project.module(&module_path) else {
            return;
        };
        let owned_source;
        let source = if let Some(document) = self.documents.get(&path_to_file_uri(&file.path)) {
            document.text.as_str()
        } else {
            let Ok(text) = fs::read_to_string(&file.path) else {
                return;
            };
            owned_source = text;
            &owned_source
        };
        let Ok(tokens) = lex(source) else {
            return;
        };
        let Ok(program) = parse(tokens) else {
            return;
        };
        for statement in &program.statements {
            let Stmt::Public { statement, .. } = statement else {
                continue;
            };
            if let Some(item) = public_completion_item(statement, member_prefix) {
                items.push(item);
            }
        }
    }

    fn inlay_hints(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!([]));
        };
        let start = params
            .pointer("/range/start")
            .map(|position| {
                offset(
                    &document.text,
                    position.get("line").and_then(Value::as_u64).unwrap_or(0) as u32,
                    position
                        .get("character")
                        .and_then(Value::as_u64)
                        .unwrap_or(0) as u32,
                )
            })
            .unwrap_or(0);
        let end = params
            .pointer("/range/end")
            .map(|position| {
                offset(
                    &document.text,
                    position
                        .get("line")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX) as u32,
                    position
                        .get("character")
                        .and_then(Value::as_u64)
                        .unwrap_or(u64::MAX) as u32,
                )
            })
            .unwrap_or(document.text.len());
        let hints = analysis
            .inlay_hints
            .iter()
            .filter(|hint| start <= hint.position && hint.position <= end)
            .map(|hint| {
                json!({
                    "position": {
                        "line": position(&document.text, hint.position)[0],
                        "character": position(&document.text, hint.position)[1]
                    },
                    "label": hint.label,
                    "kind": 1,
                    "tooltip": format!("Inferred type for `{}`", text_at(&document.text, hint.span))
                })
            })
            .collect::<Vec<_>>();
        Ok(json!(hints))
    }

    fn document_symbols(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!([]));
        };
        let symbols = analysis
            .symbols
            .iter()
            .filter(|symbol| symbol.is_definition)
            .map(|symbol| {
                json!({
                    "name": symbol.name,
                    "kind": document_symbol_kind(symbol.kind),
                    "range": range(&document.text, symbol.span),
                    "selectionRange": range(&document.text, symbol.span)
                })
            })
            .collect::<Vec<_>>();
        Ok(json!(symbols))
    }

    fn semantic_tokens(&self, params: &Value) -> Result<Value, AnyError> {
        let (_, document) = self.document(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(json!({ "data": [] }));
        };
        let mut symbols = analysis.symbols.iter().collect::<Vec<_>>();
        symbols.sort_by_key(|symbol| symbol.span.start);

        let mut previous_line = 0_u32;
        let mut previous_character = 0_u32;
        let mut data = Vec::with_capacity(symbols.len() * 5);
        for symbol in symbols {
            let start = position(&document.text, symbol.span.start);
            let end = position(&document.text, symbol.span.end);
            if start[0] != end[0] {
                continue;
            }
            let delta_line = start[0] - previous_line;
            let delta_start = if delta_line == 0 {
                start[1] - previous_character
            } else {
                start[1]
            };
            data.extend([
                delta_line,
                delta_start,
                end[1] - start[1],
                semantic_token_kind(symbol.kind),
                u32::from(symbol.is_definition),
            ]);
            previous_line = start[0];
            previous_character = start[1];
        }
        Ok(json!({ "data": data }))
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

fn integer_intrinsic_completion(item: &rils_builtins::IntrinsicDeclaration) -> Value {
    let detail = match item.kind {
        rils_builtins::IntrinsicKind::Method => {
            format!("fn {}(...) -> {:?}", item.name, item.signature.result)
        }
        rils_builtins::IntrinsicKind::AssociatedFunction => {
            format!("fn {}(value) -> {:?}", item.name, item.signature.result)
        }
    };
    json!({
        "label": item.name,
        "kind": 2,
        "detail": detail,
        "documentation": { "kind": "markdown", "value": item.documentation },
        "sortText": format!("0_{}", item.name)
    })
}

fn method_completion_target(text: &str, byte_offset: usize) -> Option<(String, String)> {
    let end = floor_char_boundary(text, byte_offset.min(text.len()));
    let before = &text[..end];
    let mut start = before.len();
    for (index, character) in before.char_indices().rev() {
        if character == '.' || character == '_' || character.is_alphanumeric() {
            start = index;
        } else {
            break;
        }
    }
    let token = &before[start..];
    let (receiver, prefix) = token.rsplit_once('.')?;
    (!receiver.is_empty()
        && receiver.chars().all(|ch| ch == '_' || ch.is_alphanumeric())
        && prefix.chars().all(|ch| ch == '_' || ch.is_alphanumeric()))
    .then(|| (receiver.to_owned(), prefix.to_owned()))
}

fn analysis(document: &Document) -> Option<&DocumentAnalysis> {
    document.analysis.as_ref().ok()
}

fn completion_target(text: &str, byte_offset: usize) -> Option<(String, String)> {
    let end = floor_char_boundary(text, byte_offset.min(text.len()));
    let before = &text[..end];
    let mut start = before.len();
    for (index, character) in before.char_indices().rev() {
        if character == ':' || character == '_' || character.is_alphanumeric() {
            start = index;
        } else {
            break;
        }
    }
    let token = &before[start..];
    let separator = token.rfind("::")?;
    let qualifier = &token[..separator];
    let member_prefix = &token[separator + 2..];
    if qualifier.is_empty()
        || !member_prefix
            .chars()
            .all(|character| character == '_' || character.is_alphanumeric())
    {
        return None;
    }
    Some((qualifier.to_owned(), member_prefix.to_owned()))
}

fn qualified_path_at(text: &str, byte_offset: usize) -> Option<String> {
    let offset = floor_char_boundary(text, byte_offset.min(text.len()));
    let allowed =
        |character: char| character == ':' || character == '_' || character.is_alphanumeric();
    let mut start = offset;
    for (index, character) in text[..offset].char_indices().rev() {
        if !allowed(character) {
            break;
        }
        start = index;
    }
    let mut end = offset;
    for (relative, character) in text[offset..].char_indices() {
        if !allowed(character) {
            break;
        }
        end = offset + relative + character.len_utf8();
    }
    let path = text[start..end].trim_matches(':');
    path.contains("::").then(|| path.to_owned())
}

fn resolve_path_alias(text: &str, qualifier: &str) -> String {
    let (root, suffix) = qualifier
        .split_once("::")
        .map_or((qualifier, None), |(root, suffix)| (root, Some(suffix)));
    for line in text.lines() {
        let Some(import) = line.trim().strip_prefix("use ") else {
            continue;
        };
        let import = import.trim_end_matches(';').trim();
        let (path, alias) = import.split_once(" as ").map_or_else(
            || (import, import.rsplit("::").next().unwrap_or(import)),
            |(path, alias)| (path.trim(), alias.trim()),
        );
        if alias != root {
            continue;
        }
        return suffix.map_or_else(|| path.to_owned(), |suffix| format!("{path}::{suffix}"));
    }
    qualifier.to_owned()
}

fn resolve_project_path(current: &str, qualifier: &str) -> Option<String> {
    let mut segments = qualifier.split("::").filter(|segment| !segment.is_empty());
    let first = segments.next()?;
    let mut resolved = match first {
        "crate" => Vec::new(),
        "self" => module_segments(current),
        "super" => {
            let mut path = module_segments(current);
            path.pop()?;
            path
        }
        name => vec![name.to_owned()],
    };
    for segment in segments {
        if segment == "super" {
            resolved.pop()?;
        } else if segment != "self" && segment != "crate" {
            resolved.push(segment.to_owned());
        }
    }
    Some(resolved.join("::"))
}

fn module_segments(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

fn join_module_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else {
        format!("{parent}::{child}")
    }
}

fn public_completion_item(statement: &Stmt, prefix: &str) -> Option<Value> {
    let (name, kind, detail) = match statement {
        Stmt::Function {
            name,
            parameters,
            return_type,
            ..
        } => {
            let parameters = parameters
                .iter()
                .map(|parameter| match &parameter.type_annotation {
                    Some(ty) => format!("{}: {ty}", parameter.name),
                    None => parameter.name.clone(),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let result = return_type
                .as_ref()
                .map_or(String::new(), |ty| format!(" -> {ty}"));
            (name, 3, format!("fn {name}({parameters}){result}"))
        }
        Stmt::Struct { name, .. } => (name, 22, format!("struct {name}")),
        Stmt::Enum { name, .. } => (name, 13, format!("enum {name}")),
        Stmt::Trait { name, .. } => (name, 8, format!("trait {name}")),
        Stmt::TypeAlias { name, target, .. } => (name, 25, format!("type {name} = {target}")),
        Stmt::Module { name, .. } => (name, 9, format!("module {name}")),
        Stmt::Use { path, alias, .. } => {
            let name = alias.as_ref().or_else(|| path.last())?;
            (name, 18, format!("use {}", path.join("::")))
        }
        _ => return None,
    };
    name.starts_with(prefix).then(|| {
        json!({
            "label": name,
            "kind": kind,
            "detail": detail,
            "sortText": format!("1_{name}")
        })
    })
}

fn declaration_name_span(statement: &Stmt, expected: &str) -> Option<Span> {
    match statement {
        Stmt::Function {
            name, name_span, ..
        }
        | Stmt::Struct {
            name, name_span, ..
        }
        | Stmt::Enum {
            name, name_span, ..
        }
        | Stmt::TypeAlias {
            name, name_span, ..
        }
        | Stmt::Trait {
            name, name_span, ..
        }
        | Stmt::Module {
            name, name_span, ..
        } if name == expected => Some(*name_span),
        Stmt::Use {
            path,
            alias,
            alias_span,
            span,
        } if alias.as_deref().or_else(|| path.last().map(String::as_str)) == Some(expected) => {
            Some(alias_span.unwrap_or(*span))
        }
        _ => None,
    }
}

fn split_qualified_name(name: &str) -> Result<(&str, &str), ()> {
    name.rsplit_once("::").ok_or(())
}

fn signature_declaration(name: &str, signature: &FunctionSignature) -> String {
    function_declaration(name, &signature.as_type())
}

fn diagnostics(text: &str, result: &Result<DocumentAnalysis, FrontendError>) -> Vec<Value> {
    match result {
        Ok(analysis) => analysis
            .diagnostics
            .iter()
            .map(|diagnostic| {
                json!({
                    "range": range(text, diagnostic.span),
                    "severity": match diagnostic.severity {
                        DiagnosticSeverity::Error => 1,
                        DiagnosticSeverity::Warning => 2,
                    },
                    "source": "rils",
                    "message": diagnostic.message
                })
            })
            .collect(),
        Err(error) => vec![json!({
            "range": range(text, error.span()),
            "severity": 1,
            "source": "rils",
            "message": error.to_string()
        })],
    }
}

fn string_at(value: &Value, path: &[&str]) -> Result<String, AnyError> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| invalid_data(format!("missing `{}`", path.join("."))))?;
    }
    current
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| invalid_data(format!("`{}` is not a string", path.join("."))))
}

fn u32_at(value: &Value, path: &[&str]) -> Result<u32, AnyError> {
    let mut current = value;
    for segment in path {
        current = current
            .get(*segment)
            .ok_or_else(|| invalid_data(format!("missing `{}`", path.join("."))))?;
    }
    current
        .as_u64()
        .and_then(|number| u32::try_from(number).ok())
        .ok_or_else(|| invalid_data(format!("`{}` is not a u32", path.join("."))))
}

fn invalid_data(message: impl Into<String>) -> AnyError {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

fn range(text: &str, span: Span) -> Value {
    json!({
        "start": {
            "line": position(text, span.start)[0],
            "character": position(text, span.start)[1]
        },
        "end": {
            "line": position(text, span.end)[0],
            "character": position(text, span.end)[1]
        }
    })
}

fn position(text: &str, byte_offset: usize) -> [u32; 2] {
    let safe_offset = byte_offset.min(text.len());
    let before = &text[..floor_char_boundary(text, safe_offset)];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let character = before[line_start..].encode_utf16().count() as u32;
    [line, character]
}

fn offset(text: &str, target_line: u32, target_character: u32) -> usize {
    let mut line = 0_u32;
    let mut character = 0_u32;
    for (index, value) in text.char_indices() {
        if line == target_line && character >= target_character {
            return index;
        }
        if value == '\n' {
            if line == target_line {
                return index;
            }
            line += 1;
            character = 0;
        } else if line == target_line {
            character += value.len_utf16() as u32;
        }
    }
    text.len()
}

fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

fn text_at(text: &str, span: Span) -> &str {
    let start = floor_char_boundary(text, span.start.min(text.len()));
    let end = floor_char_boundary(text, span.end.min(text.len()));
    &text[start..end.max(start)]
}

fn kind_label(kind: SymbolKind) -> &'static str {
    match kind {
        SymbolKind::Variable => "let",
        SymbolKind::Parameter => "parameter",
        SymbolKind::Function => "fn",
        SymbolKind::Macro => "macro",
        SymbolKind::Type => "type",
        SymbolKind::Trait => "trait",
        SymbolKind::Method => "method",
        SymbolKind::Field => "field",
        SymbolKind::Variant => "variant",
        SymbolKind::Module => "module",
    }
}

fn function_declaration(name: &str, ty: &Type) -> String {
    match ty {
        Type::Function {
            parameters: Some(parameters),
            return_type,
        } => {
            let parameters = parameters
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ");
            format!("fn {name}({parameters}) -> {return_type}")
        }
        _ => format!("fn {name}: {ty}"),
    }
}

fn document_symbol_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Variable | SymbolKind::Parameter => 13,
        SymbolKind::Function => 12,
        SymbolKind::Macro => 12,
        SymbolKind::Type => 23,
        SymbolKind::Trait => 11,
        SymbolKind::Method => 6,
        SymbolKind::Field => 8,
        SymbolKind::Variant => 22,
        SymbolKind::Module => 2,
    }
}

fn semantic_token_kind(kind: SymbolKind) -> u32 {
    match kind {
        SymbolKind::Variable => 0,
        SymbolKind::Parameter => 1,
        SymbolKind::Function => 2,
        SymbolKind::Macro => 2,
        SymbolKind::Type => 3,
        SymbolKind::Trait => 6,
        SymbolKind::Field => 7,
        SymbolKind::Method => 8,
        SymbolKind::Variant => 9,
        SymbolKind::Module => 10,
    }
}

fn compatible_symbol_kinds(left: SymbolKind, right: SymbolKind) -> bool {
    left == right
        || matches!(
            (left, right),
            (SymbolKind::Type, SymbolKind::Module) | (SymbolKind::Module, SymbolKind::Type)
        )
}

fn workspace_roots(initialization: &Value) -> Vec<PathBuf> {
    let mut roots = initialization
        .get("workspaceFolders")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|folder| folder.get("uri").and_then(Value::as_str))
        .filter_map(file_uri_to_path)
        .collect::<Vec<_>>();
    if roots.is_empty()
        && let Some(root) = initialization
            .get("rootUri")
            .and_then(Value::as_str)
            .and_then(file_uri_to_path)
    {
        roots.push(root);
    }
    roots
}

fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
    let encoded = uri.strip_prefix("file://")?;
    let bytes = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = hex(bytes[index + 1])?;
            let low = hex(bytes[index + 2])?;
            decoded.push((high << 4) | low);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    let mut path = String::from_utf8(decoded).ok()?;
    if cfg!(windows)
        && path.starts_with('/')
        && path.as_bytes().get(2).is_some_and(|byte| *byte == b':')
    {
        path.remove(0);
    }
    Some(PathBuf::from(path))
}

fn path_to_file_uri(path: &Path) -> String {
    let path = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .into_owned();
    let path = path
        .strip_prefix(r"\\?\")
        .unwrap_or(&path)
        .replace('\\', "/");
    let encoded = percent_encode_path(&path);
    if path.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b':' | b'-' | b'_' | b'.' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Document, Project, Server, Type, analysis, diagnostics, file_uri_to_path,
        function_declaration, offset, path_to_file_uri, position,
    };
    use lsp_server::Connection;
    use rils_compiler::HostContract;
    use rils_frontend::FunctionSignature;
    use serde_json::json;
    use std::{
        collections::{HashMap, HashSet},
        fs,
    };

    #[test]
    fn positions_use_utf16_characters() {
        let source = "let 名字 = \"😀\";\n名字";
        for byte_offset in [0, 4, 10, source.len()] {
            let position = position(source, byte_offset);
            assert_eq!(offset(source, position[0], position[1]), byte_offset);
        }
    }

    #[test]
    fn formats_higher_order_function_declarations() {
        let ty = Type::function(Vec::new(), Type::function(Vec::new(), Type::I32));
        assert_eq!(
            function_declaration("make_value", &ty),
            "fn make_value() -> fn() -> i32"
        );
    }

    #[test]
    fn hover_shows_expanded_type_aliases() {
        let text = "struct Box<T> { value: T }\ntype ValueBox<T> = Box<T>;\ntype IntBox = ValueBox<i32>;\nlet value: IntBox = Box { value: 1 };";
        let uri = "file:///aliases.rils".to_owned();
        let (connection, _client) = Connection::memory();
        let mut documents = HashMap::new();
        documents.insert(
            uri.clone(),
            Document {
                text: text.into(),
                analysis: rils_frontend::analysis::analyze(text),
            },
        );
        let server = Server {
            connection,
            documents,
            workspace_documents: HashSet::new(),
            host_contract: HostContract::new(),
            host_functions: HashMap::new(),
            projects: Vec::new(),
        };

        let hover = server
            .hover(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 2, "character": 5 }
            }))
            .unwrap();
        assert_eq!(
            hover
                .pointer("/contents/value")
                .and_then(|value| value.as_str()),
            Some("```rils\ntype IntBox = Box<i32>\n```")
        );
    }

    #[test]
    fn completes_host_modules_functions_and_aliases() {
        let mut contract = HostContract::new();
        contract
            .register_function(
                100,
                "unity_engine::math::add",
                FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
                "unity.math",
            )
            .unwrap();
        contract
            .register_function(
                101,
                "unity_engine::math::subtract",
                FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
                "unity.math",
            )
            .unwrap();
        contract
            .register_function(
                102,
                "unity_engine::time::frame_count",
                FunctionSignature::fixed(
                    Vec::new(),
                    Type::Integer(rils_frontend::IntegerType::U64),
                ),
                "unity.time",
            )
            .unwrap();
        let host_functions = contract
            .functions()
            .map(|function| (function.name.clone(), function.signature.clone()))
            .collect::<HashMap<_, _>>();
        let text = "use unity_engine::math as math;\nmath::a";
        let uri = "file:///completion.rils".to_owned();
        let (connection, _client) = Connection::memory();
        let mut documents = HashMap::new();
        documents.insert(
            uri.clone(),
            Document {
                text: text.into(),
                analysis: rils_frontend::analysis::analyze_with_host_functions(
                    text,
                    &host_functions,
                ),
            },
        );
        let server = Server {
            connection,
            documents,
            workspace_documents: HashSet::new(),
            host_contract: contract,
            host_functions,
            projects: Vec::new(),
        };

        let functions = server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 7 }
            }))
            .unwrap();
        assert_eq!(functions.as_array().unwrap().len(), 1);
        assert_eq!(functions[0]["label"], "add");
        assert_eq!(functions[0]["detail"], "fn add(i32, i32) -> i32");
        assert!(
            functions[0]
                .pointer("/documentation/value")
                .and_then(|value| value.as_str())
                .is_some_and(|value| value.contains("unity.math"))
        );

        let modules = server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 0, "character": 18 }
            }))
            .unwrap();
        assert!(
            modules
                .as_array()
                .is_some_and(|items| items.iter().any(|item| item["label"] == "math"))
        );
    }

    #[test]
    fn completes_integer_intrinsic_methods_and_associated_functions() {
        let text = "let value: i32 = 1;\nvalue.checked_add(1i32);\ni16::try_from(1usize);";
        let uri = "file:///intrinsics.rils".to_owned();
        let (connection, _client) = Connection::memory();
        let mut documents = HashMap::new();
        documents.insert(
            uri.clone(),
            Document {
                text: text.into(),
                analysis: rils_frontend::analysis::analyze(text),
            },
        );
        let server = Server {
            connection,
            documents,
            workspace_documents: HashSet::new(),
            host_contract: HostContract::new(),
            host_functions: HashMap::new(),
            projects: Vec::new(),
        };

        let methods = server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 11 }
            }))
            .unwrap();
        assert!(
            methods
                .as_array()
                .is_some_and(|items| { items.iter().any(|item| item["label"] == "checked_add") })
        );

        let associated = server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 2, "character": 9 }
            }))
            .unwrap();
        assert_eq!(associated[0]["label"], "try_from");
    }

    #[test]
    fn completes_project_modules_public_items_and_crate_aliases() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "rils-analyzer-project-test-{}-{unique}",
            std::process::id()
        ));
        let scripts = root.join("Assets/Res/rils-script");
        fs::create_dir_all(&scripts).unwrap();
        fs::write(
            root.join("rils.toml"),
            "[project]\nname = \"unity_game\"\nscript_paths = [\"Assets/Res/rils-script\"]\n",
        )
        .unwrap();
        fs::write(
            scripts.join("math.rils"),
            "pub fn add(left: i32, right: i32) -> i32 { left + right }\nfn hidden() {}",
        )
        .unwrap();
        let entry = scripts.join("main.rils");
        let text = "use crate::math as math;\nfn main() { math::add(1, 2); }";
        fs::write(&entry, text).unwrap();
        let project = Project::from_file(root.join("rils.toml")).unwrap();
        let uri = path_to_file_uri(&entry);
        let (connection, _client) = Connection::memory();
        let mut documents = HashMap::new();
        documents.insert(
            uri.clone(),
            Document {
                text: text.into(),
                analysis: rils_frontend::analysis::analyze(text),
            },
        );
        let server = Server {
            connection,
            documents,
            workspace_documents: HashSet::new(),
            host_contract: HostContract::new(),
            host_functions: HashMap::new(),
            projects: vec![project],
        };
        let completion = server
            .completion(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 19 }
            }))
            .unwrap();
        assert!(completion.as_array().is_some_and(|items| {
            items.iter().any(|item| item["label"] == "add")
                && !items.iter().any(|item| item["label"] == "hidden")
        }));
        let definition = server
            .definition(&json!({
                "textDocument": { "uri": uri },
                "position": { "line": 1, "character": 20 }
            }))
            .unwrap();
        let expected_uri = path_to_file_uri(&scripts.join("math.rils"));
        assert_eq!(definition["uri"].as_str(), Some(expected_uri.as_str()));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn loads_binary_host_manifest_from_initialization_options() {
        let mut contract = HostContract::new();
        contract
            .register_function(
                100,
                "unity_engine::math::add",
                FunctionSignature::fixed(vec![Type::I32, Type::I32], Type::I32),
                "unity.math",
            )
            .unwrap();
        let path = std::env::temp_dir().join(format!(
            "rils-analyzer-host-manifest-{}.rilhm",
            std::process::id()
        ));
        fs::write(&path, contract.to_manifest_bytes().unwrap()).unwrap();
        let (connection, _client) = Connection::memory();
        let mut server = Server {
            connection,
            documents: HashMap::new(),
            workspace_documents: HashSet::new(),
            host_contract: HostContract::new(),
            host_functions: HashMap::new(),
            projects: Vec::new(),
        };
        let result = server.load_host_manifests(&json!({
            "initializationOptions": {
                "hostManifestPaths": [path.to_string_lossy()]
            }
        }));
        fs::remove_file(path).unwrap();
        result.unwrap();
        assert!(
            server
                .host_contract
                .function("unity_engine::math::add")
                .is_some()
        );
        assert!(
            server
                .host_functions
                .contains_key("unity_engine::math::add")
        );
    }

    #[test]
    fn parse_errors_remain_diagnostics_not_request_failures() {
        let text = "let =";
        let result = rils_frontend::analysis::analyze(text);
        let document = Document {
            text: text.into(),
            analysis: result,
        };
        assert!(analysis(&document).is_none());
        assert_eq!(diagnostics(&document.text, &document.analysis).len(), 1);
    }

    #[test]
    fn publishes_control_flow_diagnostics() {
        let text = "fn value(flag: bool) -> i32 { if flag { 1 } }";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert_eq!(output.len(), 1);
        assert!(
            output[0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("not all paths return"))
        );
    }

    #[test]
    fn publishes_ownership_diagnostics() {
        let text = "fn invalid() { let value = \"owned\"; let moved = value; value; }";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert_eq!(output.len(), 1);
        assert!(
            output[0]["message"]
                .as_str()
                .is_some_and(|message| message.contains("use of moved value"))
        );
    }

    #[test]
    fn publishes_warnings_with_lsp_warning_severity() {
        let text = "fn value() -> i32 { return 1; 2 }";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert!(output.iter().any(|diagnostic| {
            diagnostic["message"] == "unreachable statement" && diagnostic["severity"] == 2
        }));
    }

    #[test]
    fn publishes_static_type_diagnostics() {
        let text = "fn value(input: i32) -> i32 { input } value(\"wrong\")";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert!(output.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("argument expects `i32`"))
                && diagnostic["severity"] == 1
        }));
    }

    #[test]
    fn file_uris_round_trip_for_workspace_indexing() {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("examples")
            .join("hello.rils");
        let uri = path_to_file_uri(&path);
        let decoded = file_uri_to_path(&uri).unwrap();
        assert_eq!(
            decoded.canonicalize().unwrap(),
            path.canonicalize().unwrap()
        );
    }
}
