use std::collections::HashMap;

use crate::{ExprId, SourceId, Span, ast::Program};

use super::visit::visit_statements;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct ExpressionIds {
    by_span: HashMap<Span, Vec<ExprId>>,
    spans: HashMap<ExprId, Span>,
    visit_order: Vec<ExprId>,
}

impl ExpressionIds {
    pub(super) fn allocate(program: &Program, fallback_source: SourceId) -> Self {
        let mut ids = Self::default();
        let mut next_by_source = HashMap::<SourceId, u32>::new();
        visit_statements(
            &program.statements,
            &mut Vec::new(),
            None,
            &mut |expression, _, _| {
                let span = expression.span();
                let source = if span.source == SourceId::UNKNOWN {
                    fallback_source
                } else {
                    span.source
                };
                let next = next_by_source.entry(source).or_insert(0);
                let id = ExprId {
                    source,
                    local: *next,
                };
                *next = next.checked_add(1).expect("expression id overflow");
                ids.by_span.entry(span).or_default().push(id);
                ids.spans.insert(id, span);
                ids.visit_order.push(id);
            },
        );
        ids
    }

    pub(super) fn at(&self, span: Span) -> &[ExprId] {
        self.by_span.get(&span).map_or(&[], Vec::as_slice)
    }

    pub(super) fn span(&self, id: ExprId) -> Option<Span> {
        self.spans.get(&id).copied()
    }

    pub(super) fn visit_order(&self) -> &[ExprId] {
        &self.visit_order
    }

    pub(super) fn extend(&mut self, other: Self) {
        for (span, ids) in other.by_span {
            self.by_span.entry(span).or_default().extend(ids);
        }
        self.spans.extend(other.spans);
        self.visit_order.extend(other.visit_order);
    }
}
