use super::*;

impl Analyzer {
    pub(super) fn define(&mut self, name: &str, span: Span, kind: SymbolKind) -> SymbolId {
        let merges_host_module = kind == SymbolKind::Module
            && self
                .scopes
                .last()
                .and_then(|scope| scope.get(name))
                .is_some_and(|definition| {
                    definition.kind == SymbolKind::Module && definition.span.is_none()
                });
        if !merges_host_module
            && self
                .scopes
                .last()
                .is_some_and(|scope| scope.contains_key(name))
        {
            self.result.diagnostics.push(AnalysisDiagnostic::error(
                format!("`{name}` is already defined in this scope"),
                span,
            ));
        }
        let id = self.definition_only(name, span, kind);
        let is_synthetic = span.start == span.end;
        self.scopes.last_mut().expect("scope exists").insert(
            name.into(),
            Definition {
                span: (!is_synthetic).then_some(span),
                id: (!is_synthetic).then_some(id),
                kind,
                container: None,
            },
        );
        id
    }

    pub(super) fn definition_only(&mut self, name: &str, span: Span, kind: SymbolKind) -> SymbolId {
        let source = if span.source == SourceId::UNKNOWN {
            self.source_id
        } else {
            span.source
        };
        let next_symbol = self.next_symbol.entry(source).or_insert(1);
        let id = SymbolId {
            source,
            local: *next_symbol,
        };
        *next_symbol = next_symbol.checked_add(1).expect("symbol id overflow");
        self.result.symbols.push(SymbolOccurrence {
            name: name.into(),
            span,
            definition_span: Some(span),
            symbol_id: Some(id),
            definition_id: Some(id),
            kind,
            is_definition: true,
            inferred_type: None,
            detail: None,
            container: None,
        });
        id
    }

    pub(super) fn reference(&mut self, name: &str, span: Span, fallback_kind: SymbolKind) {
        if let Some(definition) = self.lookup(name).cloned() {
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: definition.span,
                symbol_id: None,
                definition_id: definition.id,
                kind: definition.kind,
                is_definition: false,
                inferred_type: None,
                detail: None,
                container: definition.container,
            });
        } else {
            if !self.glob_imports.iter().rev().any(|has_glob| *has_glob) {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("undefined name `{name}`"),
                    span,
                ));
            }
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: None,
                symbol_id: None,
                definition_id: None,
                kind: fallback_kind,
                is_definition: false,
                inferred_type: None,
                detail: None,
                container: None,
            });
        }
    }

    pub(super) fn member_symbol(&mut self, name: &str, span: Span, fallback_kind: SymbolKind) {
        let method = self
            .inherent_methods
            .get(name)
            .and_then(|methods| (methods.len() == 1).then(|| methods[0].clone()));
        self.result.symbols.push(SymbolOccurrence {
            name: name.into(),
            span: member_name_span(span, name),
            definition_span: method.as_ref().map(|method| method.span),
            symbol_id: None,
            definition_id: None,
            kind: method
                .as_ref()
                .map(|_| SymbolKind::Method)
                .unwrap_or(fallback_kind),
            is_definition: false,
            inferred_type: None,
            detail: method.as_ref().map(|method| method.detail.clone()),
            container: method.map(|method| SymbolContainer::Type(method.owner)),
        });
    }

    pub(super) fn variant_symbol_for_path(
        &mut self,
        path: &[String],
        symbol_span: Span,
        path_ends_at_variant: bool,
    ) {
        let Some((variant_name, owner_segments)) = path.split_last() else {
            return;
        };
        let enum_name = owner_segments.join("::");
        let Some(variant) = self
            .enum_variants
            .get(&(enum_name, variant_name.clone()))
            .cloned()
        else {
            return;
        };
        let variant_span = if path_ends_at_variant {
            Span::in_source(
                symbol_span.source,
                symbol_span.end.saturating_sub(variant_name.len()),
                symbol_span.end,
            )
        } else {
            let start = symbol_span.start
                + path[..path.len() - 1]
                    .iter()
                    .map(|segment| segment.len() + 2)
                    .sum::<usize>();
            Span::in_source(symbol_span.source, start, start + variant_name.len())
        };
        self.result.symbols.push(SymbolOccurrence {
            name: variant_name.clone(),
            span: variant_span,
            definition_span: variant.span,
            symbol_id: None,
            definition_id: None,
            kind: SymbolKind::Variant,
            is_definition: false,
            inferred_type: None,
            detail: Some(variant.detail),
            container: Some(SymbolContainer::Type(variant.owner)),
        });
    }

    pub(super) fn pattern_variant_symbols(
        &mut self,
        path: &[String],
        symbol_span: Span,
        path_ends_at_variant: bool,
    ) {
        if path.len() >= 2 {
            let type_index = path.len() - 2;
            let type_name = &path[type_index];
            let type_span = if path_ends_at_variant {
                source_path_segment_span(path, type_index, symbol_span)
            } else {
                let start = symbol_span.start
                    + path[..type_index]
                        .iter()
                        .map(|segment| segment.len() + 2)
                        .sum::<usize>();
                Span::in_source(symbol_span.source, start, start + type_name.len())
            };
            if path.len() == 2 {
                self.reference(type_name, type_span, SymbolKind::Type);
            } else {
                self.result.symbols.push(SymbolOccurrence {
                    name: type_name.clone(),
                    span: type_span,
                    definition_span: None,
                    symbol_id: None,
                    definition_id: None,
                    kind: SymbolKind::Type,
                    is_definition: false,
                    inferred_type: Some(Type::named(path[..path.len() - 1].join("::"))),
                    detail: None,
                    container: None,
                });
            }
        }
        self.variant_symbol_for_path(path, symbol_span, path_ends_at_variant);
    }
}
