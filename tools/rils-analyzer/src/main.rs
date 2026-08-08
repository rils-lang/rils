use std::{
    collections::{HashMap, HashSet},
    error::Error,
    fs, io,
    path::{Path, PathBuf},
};

use lsp_server::{Connection, Message, Notification, Request, Response};
use rils_frontend::{
    FrontendError, Span, Type,
    analysis::{DiagnosticSeverity, DocumentAnalysis, SymbolKind, analyze},
};
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
    };
    server.load_workspace(&initialization)?;
    server.run()?;
    io_threads.join()?;
    Ok(())
}

struct Server {
    connection: Connection,
    documents: HashMap<String, Document>,
    workspace_documents: HashSet<String>,
}

impl Server {
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
        let analysis = analyze(&text);
        let diagnostics = diagnostics(&text, &analysis);
        self.documents
            .insert(uri.clone(), Document { text, analysis });
        self.publish_diagnostics(&uri, diagnostics)
    }

    fn load_workspace(&mut self, initialization: &Value) -> Result<(), AnyError> {
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
        for root in roots {
            let mut files = Vec::new();
            collect_rils_files(&root, &mut files)?;
            for path in files {
                let Ok(text) = fs::read_to_string(&path) else {
                    continue;
                };
                let uri = path_to_file_uri(&path);
                self.workspace_documents.insert(uri.clone());
                self.documents.insert(
                    uri,
                    Document {
                        analysis: analyze(&text),
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
            return Ok(Value::Null);
        };
        let Some(symbol) = current_analysis
            .symbols
            .iter()
            .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        else {
            return Ok(Value::Null);
        };
        if let Some(definition_span) = symbol.definition_span {
            return Ok(json!({
                "uri": uri,
                "range": range(&document.text, definition_span)
            }));
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

fn analysis(document: &Document) -> Option<&DocumentAnalysis> {
    document.analysis.as_ref().ok()
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

fn collect_rils_files(root: &Path, output: &mut Vec<PathBuf>) -> io::Result<()> {
    if !root.is_dir() {
        return Ok(());
    }
    let Ok(entries) = fs::read_dir(root) else {
        return Ok(());
    };
    for entry in entries {
        let Ok(entry) = entry else {
            continue;
        };
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(".git" | "target" | "node_modules" | "dist")
            ) {
                continue;
            }
            collect_rils_files(&path, output)?;
        } else if file_type.is_file()
            && path
                .extension()
                .is_some_and(|extension| extension == "rils")
        {
            output.push(path);
        }
    }
    Ok(())
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
        Document, Server, Type, analysis, diagnostics, file_uri_to_path, function_declaration,
        offset, path_to_file_uri, position,
    };
    use lsp_server::Connection;
    use serde_json::json;
    use std::collections::{HashMap, HashSet};

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
        let ty = Type::function(Vec::new(), Type::function(Vec::new(), Type::Int));
        assert_eq!(
            function_declaration("make_value", &ty),
            "fn make_value() -> fn() -> int"
        );
    }

    #[test]
    fn hover_shows_expanded_type_aliases() {
        let text = "struct Box<T> { value: T }\ntype ValueBox<T> = Box<T>;\ntype IntBox = ValueBox<int>;\nlet value: IntBox = Box { value: 1 };";
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
            Some("```rils\ntype IntBox = Box<int>\n```")
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
        let text = "fn value(flag: bool) -> int { if flag { 1 } }";
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
        let text = "fn value() -> int { return 1; 2 }";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert!(output.iter().any(|diagnostic| {
            diagnostic["message"] == "unreachable statement" && diagnostic["severity"] == 2
        }));
    }

    #[test]
    fn publishes_static_type_diagnostics() {
        let text = "fn value(input: int) -> int { input } value(\"wrong\")";
        let result = rils_frontend::analysis::analyze(text);
        let output = diagnostics(text, &result);
        assert!(output.iter().any(|diagnostic| {
            diagnostic["message"]
                .as_str()
                .is_some_and(|message| message.contains("argument expects `int`"))
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
