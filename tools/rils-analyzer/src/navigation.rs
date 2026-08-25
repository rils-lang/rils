use super::*;

impl Server {
    pub(super) fn definition(&self, params: &Value) -> Result<Value, AnyError> {
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
        if let Some(definition) = self.project_definition(&uri, document, offset) {
            return Ok(definition);
        }
        let target_id = symbol.symbol_id.or(symbol.definition_id);
        if let Some(target_id) = target_id {
            for (candidate_uri, candidate_document) in &self.documents {
                let Some(candidate_analysis) = analysis(candidate_document) else {
                    continue;
                };
                if let Some(definition) = candidate_analysis
                    .symbols
                    .iter()
                    .find(|candidate| candidate.symbol_id == Some(target_id))
                {
                    return Ok(json!({
                        "uri": candidate_uri,
                        "range": range(&candidate_document.text, definition.span)
                    }));
                }
            }
        }
        Ok(Value::Null)
    }

    fn project_definition(&self, uri: &str, document: &Document, offset: usize) -> Option<Value> {
        if let Some(symbol) = analysis(document).and_then(|current| {
            current
                .symbols
                .iter()
                .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        }) {
            if let Some(definition_span) = symbol
                .definition_span
                .filter(|span| span.source != document.source_id)
            {
                if let Some((target_uri, target_document)) = self
                    .documents
                    .iter()
                    .find(|(_, candidate)| candidate.source_id == definition_span.source)
                {
                    return Some(json!({
                        "uri": target_uri,
                        "range": range(&target_document.text, definition_span)
                    }));
                }
            }
        }
        let (target_uri, member) = self.project_member_target(uri, document, offset)?;
        let owned_source;
        let source = if let Some(document) = self.documents.get(&target_uri) {
            document.text.as_str()
        } else {
            owned_source = fs::read_to_string(file_uri_to_path(&target_uri)?).ok()?;
            &owned_source
        };
        let program = parse(lex(source).ok()?).ok()?;
        let span = program.statements.iter().find_map(|statement| {
            let Stmt::Public { statement, .. } = statement else {
                return None;
            };
            declaration_name_span(statement, &member)
        })?;
        Some(json!({
            "uri": target_uri,
            "range": range(source, span)
        }))
    }

    pub(super) fn references(&self, params: &Value) -> Result<Value, AnyError> {
        let (uri, document, offset) = self.document_and_offset(params)?;
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
        let target_id = if symbol.is_definition {
            symbol.symbol_id.or(symbol.definition_id)
        } else {
            self.project_symbol_id(&uri, document, offset, symbol.kind)
                .or(symbol.definition_id)
        };
        let Some(target_id) = target_id else {
            return Ok(json!([]));
        };
        let locations = self
            .documents
            .iter()
            .flat_map(|(candidate_uri, candidate_document)| {
                analysis(candidate_document)
                    .into_iter()
                    .flat_map(|analysis| analysis.symbols.iter())
                    .filter(|candidate| {
                        (candidate.symbol_id == Some(target_id)
                            || candidate.definition_id == Some(target_id))
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

    pub(super) fn project_symbol_id(
        &self,
        uri: &str,
        document: &Document,
        offset: usize,
        expected_kind: SymbolKind,
    ) -> Option<rils_frontend::SymbolId> {
        if let Some(symbol) = analysis(document).and_then(|current| {
            current
                .symbols
                .iter()
                .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end)
        }) {
            if let Some(definition_span) = symbol
                .definition_span
                .filter(|span| span.source != document.source_id)
            {
                if let Some(target_document) = self
                    .documents
                    .values()
                    .find(|candidate| candidate.source_id == definition_span.source)
                {
                    if let Some(target_analysis) = analysis(target_document) {
                        if let Some(definition) = target_analysis.symbols.iter().find(|candidate| {
                            candidate.is_definition
                                && candidate.span == definition_span
                                && candidate.name == symbol.name
                                && compatible_symbol_kinds(candidate.kind, expected_kind)
                        }) {
                            return definition.symbol_id;
                        }
                    }
                }
            }
        }
        let (target_uri, member) = self.project_member_target(uri, document, offset)?;
        let target_document = self.documents.get(&target_uri)?;
        let program =
            parse(lex_with_source_id(&target_document.text, target_document.source_id).ok()?)
                .ok()?;
        let public_span = program.statements.iter().find_map(|statement| {
            let Stmt::Public { statement, .. } = statement else {
                return None;
            };
            declaration_name_span(statement, &member)
        })?;
        analysis(target_document)?
            .symbols
            .iter()
            .find(|candidate| {
                candidate.is_definition
                    && candidate.span == public_span
                    && candidate.name == member
                    && compatible_symbol_kinds(candidate.kind, expected_kind)
            })?
            .symbol_id
    }

    fn project_member_target(
        &self,
        uri: &str,
        document: &Document,
        offset: usize,
    ) -> Option<(String, String)> {
        let path = file_uri_to_path(uri)?;
        let project = self
            .projects
            .iter()
            .find(|project| project.module_for_file(&path).is_some())?;
        let current = &project.module_for_file(&path)?.module_path;
        let qualified = qualified_path_at(&document.text, offset)
            .map(|qualified| resolve_path_alias(&document.text, &qualified))
            .or_else(|| imported_path_at(document, offset))?;
        let (qualifier, member) = qualified.rsplit_once("::")?;
        let module_path = resolve_project_path(current, qualifier)?;
        let file = project.module(&module_path)?;
        Some((path_to_file_uri(&file.path), member.to_owned()))
    }

    pub(super) fn hover(&self, params: &Value) -> Result<Value, AnyError> {
        let (uri, document, offset) = self.document_and_offset(params)?;
        let Some(analysis) = analysis(document) else {
            return Ok(Value::Null);
        };
        let symbol = analysis
            .symbols
            .iter()
            .find(|symbol| symbol.span.start <= offset && offset <= symbol.span.end);
        if symbol.is_none()
            && let Some(detail) = self.host_symbol_detail(document, offset, "")
        {
            let context = self
                .host_type_hover_path(document, offset)
                .map(|path| format!("`{path}`\n\n"))
                .unwrap_or_default();
            return Ok(json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("{context}```rils\n{detail}\n```")
                }
            }));
        }
        let Some(symbol) = symbol else {
            return Ok(Value::Null);
        };
        let manifest_path = host_path_at(document, offset);
        let is_explicit_manifest_symbol = manifest_path.as_deref().is_some_and(|path| {
            self.host_contract.host_type(path).is_some()
                || path.rsplit_once("::").is_some_and(|(owner, variant)| {
                    self.host_contract
                        .host_type(owner)
                        .and_then(|declaration| declaration.enum_definition.as_ref())
                        .is_some_and(|definition| definition.variants.contains_key(variant))
                })
        });
        let host_detail = (is_explicit_manifest_symbol
            || (!symbol.is_definition
                && (symbol
                    .detail
                    .as_deref()
                    .is_some_and(|detail| detail.starts_with("host "))
                    || (symbol.definition_span.is_none()
                        && symbol.symbol_id.is_none()
                        && symbol.definition_id.is_none()))))
        .then(|| self.host_symbol_detail(document, offset, &symbol.name))
        .flatten();
        let is_host_symbol = host_detail.is_some();
        let detail = if let Some(host_detail) = host_detail {
            host_detail
        } else {
            match (&symbol.detail, &symbol.inferred_type) {
                (Some(detail), _) if symbol.kind == SymbolKind::Field => {
                    detail.strip_prefix("field ").unwrap_or(detail).to_owned()
                }
                (Some(detail), _) => detail.clone(),
                (_, Some(inferred)) if symbol.kind == SymbolKind::Field => {
                    format!("{}: {inferred}", symbol.name)
                }
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
            }
        };
        let context = if is_host_symbol {
            self.host_type_hover_path(document, offset)
                .map(|path| format!("`{path}`\n\n"))
                .unwrap_or_default()
        } else {
            self.hover_path(&uri, symbol)
                .map(|path| format!("`{path}`\n\n"))
                .unwrap_or_default()
        };
        Ok(json!({
            "contents": {
                "kind": "markdown",
                "value": format!("{context}```rils\n{detail}\n```")
            },
            "range": range(&document.text, symbol.span)
        }))
    }

    /// Host manifest references do not have a source span to point at, but
    /// they should still provide the same useful declaration text as native
    /// symbols. Resolve an unannotated type occurrence against the manifest
    /// and keep overload information for functions.
    fn host_symbol_detail(&self, document: &Document, offset: usize, name: &str) -> Option<String> {
        let resolved = host_path_at(document, offset);
        if let Some(resolved) = resolved.as_deref()
            && let Some((owner, variant)) = resolved.rsplit_once("::")
            && let Some(declaration) = self.host_contract.host_type(owner)
            && let Some(enum_definition) = &declaration.enum_definition
            && let Some(raw) = enum_definition.variants.get(variant)
        {
            let owner = owner.rsplit("::").next().unwrap_or(owner);
            return Some(format!("variant {owner}::{variant} = 0x{raw:x}"));
        }
        let mut types = self
            .host_contract
            .types()
            .filter(|declaration| {
                resolved.as_ref().map_or_else(
                    || declaration.name.rsplit("::").next() == Some(name),
                    |resolved| declaration.name == *resolved,
                )
            })
            .collect::<Vec<_>>();
        if !types.is_empty() {
            types.sort_by(|left, right| left.name.cmp(&right.name));
            types.dedup_by(|left, right| left.name == right.name);
            if resolved.is_none() && types.len() != 1 {
                return None;
            }
            let declaration = types[0];
            let type_name = declaration
                .name
                .rsplit("::")
                .next()
                .unwrap_or(&declaration.name);
            if let Some(enum_definition) = &declaration.enum_definition {
                let flags = if enum_definition.flags {
                    ", implements BitFlags"
                } else {
                    ""
                };
                let variants = enum_definition
                    .variants
                    .keys()
                    .map(|variant| format!("    {variant},"))
                    .collect::<Vec<_>>()
                    .join("\n");
                return Some(format!(
                    "// host enum: {}{flags}\nenum {} {{\n{variants}\n}}",
                    enum_definition.underlying_type, type_name
                ));
            }
            // HostHandle values are opaque in Rils even when their managed
            // implementation inherits UnityEngine.Object. Keep the hover in
            // Rils terms and hide the managed class hierarchy.
            return Some(format!("struct {type_name}"));
        }
        let functions = self
            .host_contract
            .functions()
            .filter(|function| {
                resolved.as_ref().map_or_else(
                    || function.name.rsplit("::").next() == Some(name),
                    |resolved| function.name == *resolved,
                )
            })
            .collect::<Vec<_>>();
        if functions.is_empty() {
            return None;
        }
        if resolved.is_none()
            && functions
                .iter()
                .map(|function| function.name.as_str())
                .collect::<HashSet<_>>()
                .len()
                != 1
        {
            return None;
        }
        let mut declarations = functions
            .iter()
            .map(|function| {
                let parameters = function
                    .signature
                    .parameters
                    .as_ref()
                    .map(|parameters| {
                        parameters
                            .iter()
                            .map(ToString::to_string)
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_else(|| "...".into());
                format!(
                    "fn {}({parameters}) -> {}",
                    function.name, function.signature.return_type
                )
            })
            .collect::<Vec<_>>();
        declarations.sort();
        declarations.dedup();
        Some(declarations.join("\n"))
    }

    fn host_type_hover_path(&self, document: &Document, offset: usize) -> Option<String> {
        let resolved = host_path_at(document, offset)?;
        if self.host_contract.host_type(&resolved).is_some() {
            return resolved
                .rsplit_once("::")
                .map(|(module, _)| module.to_owned());
        }
        let (owner, variant) = resolved.rsplit_once("::")?;
        self.host_contract
            .host_type(owner)
            .and_then(|declaration| declaration.enum_definition.as_ref())
            .filter(|definition| definition.variants.contains_key(variant))
            .map(|_| owner.to_owned())
    }

    fn hover_path(&self, uri: &str, symbol: &SymbolOccurrence) -> Option<String> {
        let definition_uri = symbol
            .definition_span
            .and_then(|span| {
                self.documents
                    .iter()
                    .find(|(_, document)| document.source_id == span.source)
                    .map(|(uri, _)| uri.as_str())
            })
            .unwrap_or(uri);
        let path = file_uri_to_path(definition_uri)?;
        let project = self
            .projects
            .iter()
            .find(|project| project.module_for_file(&path).is_some());
        let Some(project) = project else {
            return match &symbol.container {
                Some(SymbolContainer::Module(module)) => Some(module.clone()),
                Some(SymbolContainer::Type(owner)) => Some(owner.clone()),
                None => None,
            };
        };
        let file = project.module_for_file(&path)?;
        let module = match &symbol.container {
            Some(SymbolContainer::Module(module)) => {
                if module == "crate" {
                    String::new()
                } else {
                    module.clone()
                }
            }
            Some(SymbolContainer::Type(owner)) => {
                if file.module_path.is_empty() {
                    owner.clone()
                } else {
                    format!("{}::{owner}", file.module_path)
                }
            }
            None => return None,
        };
        Some(if module.is_empty() || module == "crate" {
            "crate".to_owned()
        } else {
            format!("crate::{module}")
        })
    }
}

fn imported_path_at(document: &Document, offset: usize) -> Option<String> {
    let identifier = identifier_at(&document.text, offset)?;
    let program = parse(lex_with_source_id(&document.text, document.source_id).ok()?).ok()?;
    for statement in &program.statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        let Stmt::Use { imports, .. } = statement else {
            continue;
        };
        for import in imports {
            if import
                .path_spans
                .iter()
                .any(|span| span.start <= offset && offset <= span.end)
            {
                return Some(import.path.join("::"));
            }
            if import.binding_name() == Some(identifier) {
                return Some(import.path.join("::"));
            }
            if import.kind == rils_frontend::ast::UseImportKind::Glob {
                return Some(format!("{}::{identifier}", import.path.join("::")));
            }
        }
    }
    None
}

fn host_path_at(document: &Document, offset: usize) -> Option<String> {
    qualified_path_through_identifier_at(&document.text, offset)
        .map(|path| resolve_path_alias(&document.text, &path))
        .or_else(|| imported_path_at(document, offset))
}

/// Return only the qualified path through the identifier under the cursor.
/// For `CameraType::Game`, hovering `CameraType` must not include `::Game`,
/// while hovering `Game` must see the complete variant path.
fn qualified_path_through_identifier_at(text: &str, offset: usize) -> Option<String> {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let path_character =
        |character: char| character == ':' || character == '_' || character.is_alphanumeric();
    let identifier_character = |character: char| character == '_' || character.is_alphanumeric();
    let mut start = offset;
    for (index, character) in text[..offset].char_indices().rev() {
        if !path_character(character) {
            break;
        }
        start = index;
    }
    let mut end = offset;
    for (relative, character) in text[offset..].char_indices() {
        if !identifier_character(character) {
            break;
        }
        end = offset + relative + character.len_utf8();
    }
    let path = text[start..end].trim_matches(':');
    path.contains("::").then(|| path.to_owned())
}

fn identifier_at(text: &str, offset: usize) -> Option<&str> {
    let offset = floor_char_boundary(text, offset.min(text.len()));
    let allowed = |character: char| character == '_' || character.is_alphanumeric();
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
    (start < end).then(|| &text[start..end])
}
