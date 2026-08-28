use std::collections::HashMap;

use crate::{
    ExprId, SourceId, Span, Type,
    ast::{Expr, Program},
};

use super::visit::visit_statements;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ExpressionIds {
    by_span: HashMap<Span, Vec<ExprId>>,
    spans: HashMap<ExprId, Span>,
}

impl ExpressionIds {
    pub(super) fn at(&self, span: Span) -> &[ExprId] {
        self.by_span.get(&span).map_or(&[], Vec::as_slice)
    }

    pub(super) fn span(&self, id: ExprId) -> Option<Span> {
        self.spans.get(&id).copied()
    }

    pub(super) fn extend(&mut self, other: Self) {
        for (span, ids) in other.by_span {
            self.by_span.entry(span).or_default().extend(ids);
        }
        self.spans.extend(other.spans);
    }
}

#[derive(Default)]
/// Maps expression nodes in one immutable `Program` to their semantic IDs.
///
/// This index is valid while the program used to construct it remains alive
/// and is intended for compiler passes that visit the immutable AST.
#[doc(hidden)]
pub struct ExpressionIdentityMap {
    ids: ExpressionIds,
    by_node: HashMap<*const Expr, ExprId>,
}

impl ExpressionIdentityMap {
    pub fn allocate(program: &Program, fallback_source: SourceId) -> Self {
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
                ids.ids.by_span.entry(span).or_default().push(id);
                ids.ids.spans.insert(id, span);
                ids.by_node.insert(expression as *const Expr, id);
            },
        );
        ids
    }

    pub fn get(&self, expression: &Expr) -> Option<ExprId> {
        self.by_node.get(&(expression as *const Expr)).copied()
    }

    pub fn extend(&mut self, other: Self) {
        self.ids.extend(other.ids);
        self.by_node.extend(other.by_node);
    }

    pub(crate) fn id(&self, expression: &Expr) -> ExprId {
        self.get(expression)
            .expect("expression must belong to the indexed program")
    }

    pub(crate) fn into_ids(self) -> ExpressionIds {
        self.ids
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ExpressionTypes<'a> {
    identities: &'a ExpressionIdentityMap,
    types: &'a HashMap<ExprId, Type>,
}

impl<'a> ExpressionTypes<'a> {
    pub(crate) fn new(
        identities: &'a ExpressionIdentityMap,
        types: &'a HashMap<ExprId, Type>,
    ) -> Self {
        Self { identities, types }
    }

    pub(crate) fn get(self, expression: &Expr) -> Option<&'a Type> {
        self.identities
            .get(expression)
            .and_then(|id| self.types.get(&id))
    }
}
