use super::*;

pub(super) fn integer_intrinsic_completion(item: &rils_builtins::IntrinsicDeclaration) -> Value {
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

pub(super) fn integer_constant_completion(
    item: &rils_builtins::IntegerConstantDeclaration,
) -> Value {
    json!({
        "label": item.name,
        "kind": 21,
        "detail": format!("const {}: {:?}", item.name, item.value_type),
        "documentation": { "kind": "markdown", "value": item.documentation },
        "sortText": format!("0_{}", item.name)
    })
}

pub(super) fn builtin_member_completion(ty: &Type, member: &rils_builtins::BuiltinMember) -> Value {
    let detail = rils_frontend::standard_library::builtin_member_type(ty, member.name)
        .map(|member_type| function_declaration(member.name, &member_type))
        .unwrap_or_else(|| format!("fn {}(...) ", member.name));
    json!({
        "label": member.name,
        "kind": 2,
        "detail": detail,
        "documentation": { "kind": "markdown", "value": member.documentation },
        "sortText": format!("0_{}", member.name)
    })
}

pub(super) fn method_completion_target(text: &str, byte_offset: usize) -> Option<(usize, String)> {
    let end = floor_char_boundary(text, byte_offset.min(text.len()));
    let before = &text[..end];
    let mut prefix_start = before.len();
    for (index, character) in before.char_indices().rev() {
        if character == '_' || character.is_alphanumeric() {
            prefix_start = index;
        } else {
            break;
        }
    }
    let dot_offset = prefix_start.checked_sub(1)?;
    (text.as_bytes().get(dot_offset) == Some(&b'.'))
        .then(|| (dot_offset, before[prefix_start..].to_owned()))
}

pub(super) fn identifier_before(text: &str, end: usize) -> Option<&str> {
    let before = &text[..floor_char_boundary(text, end.min(text.len()))];
    let mut start = before.len();
    for (index, character) in before.char_indices().rev() {
        if character == '_' || character.is_alphanumeric() {
            start = index;
        } else {
            break;
        }
    }
    (start < before.len()).then(|| &before[start..])
}

pub(super) fn analysis(document: &Document) -> Option<&DocumentAnalysis> {
    document.analysis.as_ref().ok()
}

pub(super) fn completion_target(text: &str, byte_offset: usize) -> Option<(String, String)> {
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

pub(super) fn qualified_path_at(text: &str, byte_offset: usize) -> Option<String> {
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

pub(super) fn resolve_path_alias(text: &str, qualifier: &str) -> String {
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

pub(super) fn resolve_project_path(current: &str, qualifier: &str) -> Option<String> {
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

pub(super) fn module_segments(path: &str) -> Vec<String> {
    path.split("::")
        .filter(|segment| !segment.is_empty())
        .map(str::to_owned)
        .collect()
}

pub(super) fn join_module_path(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else {
        format!("{parent}::{child}")
    }
}

pub(super) fn public_completion_item(statement: &Stmt, prefix: &str) -> Option<Value> {
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

pub(super) fn declaration_name_span(statement: &Stmt, expected: &str) -> Option<Span> {
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

pub(super) fn split_qualified_name(name: &str) -> Result<(&str, &str), ()> {
    name.rsplit_once("::").ok_or(())
}

pub(super) fn signature_declaration(name: &str, signature: &FunctionSignature) -> String {
    function_declaration(name, &signature.as_type())
}

pub(super) fn diagnostics(
    text: &str,
    result: &Result<DocumentAnalysis, FrontendError>,
) -> Vec<Value> {
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

pub(super) fn string_at(value: &Value, path: &[&str]) -> Result<String, AnyError> {
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

pub(super) fn u32_at(value: &Value, path: &[&str]) -> Result<u32, AnyError> {
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

pub(super) fn invalid_data(message: impl Into<String>) -> AnyError {
    Box::new(io::Error::new(io::ErrorKind::InvalidData, message.into()))
}

pub(super) fn range(text: &str, span: Span) -> Value {
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

pub(super) fn position(text: &str, byte_offset: usize) -> [u32; 2] {
    let safe_offset = byte_offset.min(text.len());
    let before = &text[..floor_char_boundary(text, safe_offset)];
    let line = before.bytes().filter(|byte| *byte == b'\n').count() as u32;
    let line_start = before.rfind('\n').map_or(0, |index| index + 1);
    let character = before[line_start..].encode_utf16().count() as u32;
    [line, character]
}

pub(super) fn offset(text: &str, target_line: u32, target_character: u32) -> usize {
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

pub(super) fn floor_char_boundary(text: &str, mut offset: usize) -> usize {
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    offset
}

pub(super) fn text_at(text: &str, span: Span) -> &str {
    let start = floor_char_boundary(text, span.start.min(text.len()));
    let end = floor_char_boundary(text, span.end.min(text.len()));
    &text[start..end.max(start)]
}

pub(super) fn kind_label(kind: SymbolKind) -> &'static str {
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

pub(super) fn function_declaration(name: &str, ty: &Type) -> String {
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

pub(super) fn document_symbol_kind(kind: SymbolKind) -> u32 {
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

pub(super) fn semantic_token_kind(kind: SymbolKind) -> u32 {
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

pub(super) fn compatible_symbol_kinds(left: SymbolKind, right: SymbolKind) -> bool {
    left == right
        || matches!(
            (left, right),
            (SymbolKind::Type, SymbolKind::Module) | (SymbolKind::Module, SymbolKind::Type)
        )
}

pub(super) fn workspace_roots(initialization: &Value) -> Vec<PathBuf> {
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

pub(super) fn file_uri_to_path(uri: &str) -> Option<PathBuf> {
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

pub(super) fn path_to_file_uri(path: &Path) -> String {
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

pub(super) fn percent_encode_path(path: &str) -> String {
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

pub(super) fn hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}
