//! document state for the analyzer service.
use super::*;

impl Server {
    pub(super) fn parse_capabilities_for_uri(&self, uri: &str) -> ParseCapabilities {
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

    pub(super) fn project_for_source(&self, source_id: SourceId) -> Option<&Project> {
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

    pub(super) fn update_document(&mut self, uri: String, text: String) -> Result<(), AnyError> {
        let uri = normalize_document_uri(&uri);
        let source_id = self.source_id_for_uri(&uri)?;
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

    pub(super) fn source_id_for_uri(&mut self, uri: &str) -> Result<SourceId, AnyError> {
        if let Some(document) = self.documents.get(uri) {
            return Ok(document.source_id);
        }
        let source_id = SourceId::new(self.next_source_id);
        self.next_source_id = self.next_source_id.checked_add(1).ok_or_else(|| {
            invalid_data("source identity capacity exhausted; restart the analyzer")
        })?;
        Ok(source_id)
    }

    pub(super) fn parsed_document(
        &self,
        document: &Document,
    ) -> Option<rils_frontend::ast::Program> {
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

    pub(super) fn publish_all_diagnostics(&mut self) -> Result<(), AnyError> {
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

    pub(super) fn publish_diagnostics(
        &self,
        uri: &str,
        diagnostics: Vec<Value>,
    ) -> Result<(), AnyError> {
        self.connection
            .sender
            .send(Message::Notification(Notification::new(
                "textDocument/publishDiagnostics".to_owned(),
                json!({ "uri": uri, "diagnostics": diagnostics }),
            )))?;
        Ok(())
    }

    pub(super) fn document<'a>(
        &'a self,
        params: &Value,
    ) -> Result<(String, &'a Document), AnyError> {
        let uri = normalize_document_uri(&string_at(params, &["textDocument", "uri"])?);
        let document = self
            .documents
            .get(&uri)
            .ok_or_else(|| invalid_data(format!("document is not open: {uri}")))?;
        Ok((uri, document))
    }

    pub(super) fn document_and_offset<'a>(
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
