//! Lexical name queries for editor completion. Uses the resolver's actual scopes.
use super::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleName {
    pub name: String,
    pub kind: SymbolKind,
    pub definition_span: Option<Span>,
    pub definition_id: Option<SymbolId>,
    pub inferred_type: Option<Type>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct LexicalScope {
    span: Span,
    depth: usize,
    names: Vec<VisibleName>,
}

impl Analyzer {
    pub(super) fn record_scope(&mut self, span: Span) {
        let names = self
            .scopes
            .last()
            .into_iter()
            .flatten()
            .map(|(name, definition)| {
                let export = self
                    .module_exports
                    .values()
                    .flatten()
                    .find(|export| Some(export.span) == definition.span);
                VisibleName {
                    name: name.clone(),
                    kind: definition.kind,
                    definition_span: definition.span,
                    definition_id: definition.id,
                    inferred_type: export.and_then(|export| export.inferred_type.clone()),
                    detail: export.and_then(|export| export.detail.clone()),
                }
            })
            .collect();
        self.result.lexical_scopes.push(LexicalScope {
            span,
            depth: self.scopes.len(),
            names,
        });
    }
}

impl DocumentAnalysis {
    /// Visible bindings, with inner scopes taking precedence and later locals excluded.
    /// Definition identities are retained through aliases and glob imports.
    pub fn visible_names(&self, source: SourceId, offset: usize) -> Vec<VisibleName> {
        let mut scopes = self
            .lexical_scopes
            .iter()
            .filter(|scope| {
                scope.span.source == source && scope.span.start <= offset && offset < scope.span.end
            })
            .collect::<Vec<_>>();
        scopes.sort_by_key(|scope| std::cmp::Reverse(scope.depth));
        let mut seen = HashSet::new();
        let mut result = Vec::new();
        for scope in scopes {
            for name in &scope.names {
                if matches!(name.kind, SymbolKind::Variable | SymbolKind::Parameter)
                    && name
                        .definition_span
                        .is_some_and(|span| span.source == source && span.end > offset)
                {
                    continue;
                }
                if !seen.insert(&name.name) {
                    continue;
                }
                let mut name = name.clone();
                if let Some(symbol) = self.symbols.iter().find(|symbol| {
                    symbol.is_definition
                        && (name.definition_id.is_some() && symbol.symbol_id == name.definition_id
                            || name.definition_span.is_some()
                                && Some(symbol.span) == name.definition_span)
                }) {
                    name.inferred_type = symbol.inferred_type.clone().or(name.inferred_type);
                    name.detail = symbol.detail.clone().or(name.detail);
                }
                result.push(name);
            }
        }
        result.sort_by(|a, b| a.name.cmp(&b.name));
        result
    }
}
