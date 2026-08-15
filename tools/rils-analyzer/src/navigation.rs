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
