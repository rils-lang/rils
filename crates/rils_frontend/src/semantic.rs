use std::collections::{HashMap, HashSet};

use crate::{
    BodyId, DefId, ExprId, ImplId, SourceId, Span, Type,
    analysis::SymbolOccurrence,
    ast::{Block, Expr, Program, Stmt},
    types::FunctionSignature,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SymbolKind {
    Variable,
    Parameter,
    Function,
    Macro,
    Type,
    Trait,
    Method,
    Field,
    Variant,
    Module,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolContainer {
    Module(String),
    Type(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefinitionData {
    pub id: DefId,
    pub name: String,
    pub span: Span,
    pub kind: SymbolKind,
    pub container: Option<SymbolContainer>,
    pub inferred_type: Option<Type>,
    pub detail: Option<String>,
}

/// Definitions and resolved source occurrences for one analyzed program.
///
/// Consumers use this table to move from syntax locations to semantic
/// identities and back without repeating textual name lookup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DefMap {
    definitions: HashMap<DefId, DefinitionData>,
    resolutions: HashMap<Span, DefId>,
    bodies: HashMap<Span, BodyId>,
    definition_bodies: HashMap<DefId, BodyId>,
    impls: HashMap<Span, ImplId>,
}

impl DefMap {
    pub(crate) fn from_program_and_symbols(
        program: &Program,
        symbols: &[SymbolOccurrence],
    ) -> Self {
        let mut result = Self::default();
        for symbol in symbols {
            let id = if symbol.is_definition {
                let Some(id) = symbol.symbol_id else {
                    continue;
                };
                result.definitions.insert(
                    id,
                    DefinitionData {
                        id,
                        name: symbol.name.clone(),
                        span: symbol.span,
                        kind: symbol.kind,
                        container: symbol.container.clone(),
                        inferred_type: symbol.inferred_type.clone(),
                        detail: symbol.detail.clone(),
                    },
                );
                id
            } else {
                let Some(id) = symbol.definition_id else {
                    continue;
                };
                id
            };
            result.resolutions.insert(symbol.span, id);
        }
        let mut body_owners = Vec::new();
        let mut impl_spans = Vec::new();
        collect_owner_spans(&program.statements, &mut body_owners, &mut impl_spans);
        for (definition_span, body_span) in body_owners {
            let Some(definition) = result.resolution(definition_span) else {
                continue;
            };
            let body = BodyId(definition);
            result.bodies.insert(body_span, body);
            result.definition_bodies.insert(definition, body);
        }
        impl_spans.sort_by_key(|span| (span.source, span.start, span.end));
        let mut next_by_source = HashMap::<SourceId, u32>::new();
        for span in impl_spans {
            let next = next_by_source.entry(span.source).or_insert(0);
            let id = ImplId {
                source: span.source,
                local: *next,
            };
            *next = next.checked_add(1).expect("impl id overflow");
            result.impls.insert(span, id);
        }
        result
    }

    pub fn definition(&self, id: DefId) -> Option<&DefinitionData> {
        self.definitions.get(&id)
    }

    pub fn resolution(&self, span: Span) -> Option<DefId> {
        self.resolutions.get(&span).copied()
    }

    pub fn definition_at(&self, span: Span) -> Option<&DefinitionData> {
        self.resolution(span).and_then(|id| self.definition(id))
    }

    pub fn definitions(&self) -> impl Iterator<Item = &DefinitionData> {
        self.definitions.values()
    }

    pub fn body(&self, definition: DefId) -> Option<BodyId> {
        self.definition_bodies.get(&definition).copied()
    }

    pub fn body_at(&self, span: Span) -> Option<BodyId> {
        self.bodies.get(&span).copied()
    }

    pub fn impl_at(&self, span: Span) -> Option<ImplId> {
        self.impls.get(&span).copied()
    }
}

fn collect_owner_spans(statements: &[Stmt], bodies: &mut Vec<(Span, Span)>, impls: &mut Vec<Span>) {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
            Stmt::Module {
                statements: Some(statements),
                ..
            } => collect_owner_spans(statements, bodies, impls),
            Stmt::Function {
                name_span, body, ..
            } => {
                bodies.push((*name_span, body.span));
                collect_owner_spans(&body.statements, bodies, impls);
            }
            Stmt::Impl { methods, span, .. } => {
                impls.push(*span);
                for method in methods {
                    bodies.push((method.name_span, method.body.span));
                    collect_owner_spans(&method.body.statements, bodies, impls);
                }
            }
            Stmt::While { body, .. } | Stmt::Loop { body, .. } | Stmt::For { body, .. } => {
                collect_owner_spans(&body.statements, bodies, impls);
            }
            Stmt::Let { .. }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Return { .. }
            | Stmt::Break { .. }
            | Stmt::Continue { .. }
            | Stmt::Expr { .. }
            | Stmt::Module {
                statements: None, ..
            } => {}
            Stmt::Public { .. } => unreachable!("public statements were unwrapped"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinCallKind {
    Runtime,
    Intrinsic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ResolvedCall {
    Definition(DefId),
    Builtin {
        id: rils_builtins::BuiltinId,
        kind: BuiltinCallKind,
        receiver: Option<rils_builtins::ReceiverMode>,
    },
    Host {
        path: String,
    },
}

/// Semantic side tables produced by frontend analysis.
///
/// Syntax remains immutable. Later stages use expression identities to query
/// inferred types and resolved callees instead of repeating name lookup.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeckResults {
    expression_ids: HashMap<Span, ExprId>,
    expression_types: HashMap<ExprId, Type>,
    resolved_calls: HashMap<ExprId, ResolvedCall>,
}

impl TypeckResults {
    pub(crate) fn from_expression_types(expression_types: &HashMap<Span, Type>) -> Self {
        let mut spans = expression_types.keys().copied().collect::<Vec<_>>();
        spans.sort_by_key(|span| (span.source, span.start, span.end));

        let mut next_by_source = HashMap::<SourceId, u32>::new();
        let mut expression_ids = HashMap::with_capacity(spans.len());
        let mut types_by_id = HashMap::with_capacity(spans.len());
        for span in spans {
            let next = next_by_source.entry(span.source).or_insert(0);
            let id = ExprId {
                source: span.source,
                local: *next,
            };
            *next = next.checked_add(1).expect("expression id overflow");
            expression_ids.insert(span, id);
            types_by_id.insert(id, expression_types[&span].clone());
        }
        Self {
            expression_ids,
            expression_types: types_by_id,
            resolved_calls: HashMap::new(),
        }
    }

    pub fn expression_id(&self, span: Span) -> Option<ExprId> {
        self.expression_ids.get(&span).copied()
    }

    pub fn expression_type(&self, id: ExprId) -> Option<&Type> {
        self.expression_types.get(&id)
    }

    pub fn expression_type_at(&self, span: Span) -> Option<&Type> {
        self.expression_id(span)
            .and_then(|id| self.expression_type(id))
    }

    pub fn expression_type_ending_at(
        &self,
        source: SourceId,
        end: usize,
    ) -> Option<(ExprId, &Type)> {
        self.expression_ids
            .iter()
            .filter(|(span, _)| span.source == source && span.end == end)
            .max_by_key(|(span, _)| span.start)
            .and_then(|(_, id)| self.expression_type(*id).map(|ty| (*id, ty)))
    }

    pub fn resolved_call(&self, id: ExprId) -> Option<&ResolvedCall> {
        self.resolved_calls.get(&id)
    }

    pub fn resolved_call_at(&self, span: Span) -> Option<&ResolvedCall> {
        self.expression_id(span)
            .and_then(|id| self.resolved_call(id))
    }

    pub fn resolved_call_containing(
        &self,
        source: SourceId,
        offset: usize,
    ) -> Option<(ExprId, &ResolvedCall)> {
        self.expression_ids
            .iter()
            .filter(|(span, id)| {
                span.source == source
                    && span.start <= offset
                    && offset <= span.end
                    && self.resolved_calls.contains_key(id)
            })
            .min_by_key(|(span, _)| span.end.saturating_sub(span.start))
            .and_then(|(_, id)| self.resolved_call(*id).map(|call| (*id, call)))
    }

    pub(crate) fn resolve_call(&mut self, span: Span, call: ResolvedCall) {
        if let Some(id) = self.expression_id(span) {
            self.resolved_calls.insert(id, call);
        }
    }
}

pub(crate) fn resolve_program_calls(
    program: &Program,
    definitions: &DefMap,
    host_functions: &HashMap<String, FunctionSignature>,
    results: &mut TypeckResults,
) {
    let mut iterator_types = HashSet::new();
    collect_trait_implementations(
        &program.statements,
        &mut Vec::new(),
        "Iterator",
        &mut iterator_types,
    );
    visit_statements(&program.statements, &mut |expression| {
        let Expr::Call { callee, span, .. } = expression else {
            return;
        };
        if let Some(call) = resolve_callee(
            callee,
            definitions,
            host_functions,
            &iterator_types,
            results,
        ) {
            results.resolve_call(*span, call);
        }
    });
}

fn resolve_callee(
    callee: &Expr,
    definitions: &DefMap,
    host_functions: &HashMap<String, FunctionSignature>,
    iterator_types: &HashSet<String>,
    results: &TypeckResults,
) -> Option<ResolvedCall> {
    if let Some(definition) = callee_definition(callee, definitions) {
        return Some(ResolvedCall::Definition(definition));
    }
    match callee {
        Expr::Member { object, name, .. } => {
            let receiver = results.expression_type_at(object.span())?;
            let receiver = match receiver {
                Type::Reference { inner, .. } => inner.as_ref(),
                receiver => receiver,
            };
            let intrinsic = match receiver {
                Type::Integer(_) | Type::IntegerVariable(_) => crate::integer_method(name),
                Type::Float(_) | Type::FloatVariable(_) => crate::float_method(name),
                _ => None,
            };
            if let Some(intrinsic) = intrinsic {
                return Some(ResolvedCall::Builtin {
                    id: intrinsic.id,
                    kind: BuiltinCallKind::Intrinsic,
                    receiver: Some(rils_builtins::ReceiverMode::Owned),
                });
            }
            let iterator_member = match receiver {
                Type::Named { name: owner, .. } if iterator_types.contains(owner) => {
                    rils_builtins::builtin_member("Iterator", name)
                }
                _ => None,
            };
            let member = crate::standard_library::builtin_owner_name(receiver)
                .and_then(|owner| rils_builtins::builtin_member(owner, name))
                .or(iterator_member)
                .or_else(|| unqualified_builtin_member(name))?;
            Some(ResolvedCall::Builtin {
                id: member.builtin_id?,
                kind: BuiltinCallKind::Runtime,
                receiver: member.receiver,
            })
        }
        Expr::Path { segments, .. } => {
            let path = segments.join("::");
            host_functions
                .contains_key(&path)
                .then_some(ResolvedCall::Host { path })
        }
        Expr::Variable { name, .. } => host_functions
            .contains_key(name)
            .then(|| ResolvedCall::Host { path: name.clone() }),
        _ => None,
    }
}

fn collect_trait_implementations(
    statements: &[Stmt],
    prefix: &mut Vec<String>,
    trait_name: &str,
    output: &mut HashSet<String>,
) {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
            Stmt::Module {
                name,
                statements: Some(statements),
                ..
            } => {
                prefix.push(name.clone());
                collect_trait_implementations(statements, prefix, trait_name, output);
                prefix.pop();
            }
            Stmt::Impl {
                trait_name: Some(implemented),
                target: Type::Named { name, .. },
                ..
            } if implemented == trait_name => {
                output.insert(name.clone());
                if !prefix.is_empty() && !name.contains("::") {
                    output.insert(format!("{}::{name}", prefix.join("::")));
                }
            }
            _ => {}
        }
    }
}

fn unqualified_builtin_member(name: &str) -> Option<&'static rils_builtins::BuiltinMember> {
    let mut candidates = rils_builtins::BUILTINS
        .iter()
        .flat_map(|declaration| declaration.members)
        .filter(|member| member.name == name && member.builtin_id.is_some());
    let first = candidates.next()?;
    let first_id = first.builtin_id?;
    (!candidates.any(|candidate| {
        candidate.receiver != first.receiver
            || candidate.builtin_id.is_none_or(|candidate_id| {
                !first_id.shares_direct_runtime_implementation(candidate_id)
            })
    }))
    .then_some(first)
}

fn callee_definition(callee: &Expr, definitions: &DefMap) -> Option<DefId> {
    let span = match callee {
        Expr::Variable { span, .. } => *span,
        Expr::Path { segments, span } => member_span(*span, segments.last()?),
        Expr::QualifiedPath { member, span, .. }
        | Expr::Member {
            name: member, span, ..
        } => member_span(*span, member),
        _ => return None,
    };
    definitions.resolution(span)
}

fn member_span(span: Span, name: &str) -> Span {
    Span::in_source(span.source, span.end.saturating_sub(name.len()), span.end)
}

fn visit_statements(statements: &[Stmt], visitor: &mut impl FnMut(&Expr)) {
    for statement in statements {
        let statement = match statement {
            Stmt::Public { statement, .. } => statement.as_ref(),
            statement => statement,
        };
        match statement {
            Stmt::Module {
                statements: Some(statements),
                ..
            } => visit_statements(statements, visitor),
            Stmt::Let { initializer, .. } => visit_expression(initializer, visitor),
            Stmt::Function { body, .. } => visit_block(body, visitor),
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    visit_block(&method.body, visitor);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                visit_expression(condition, visitor);
                visit_block(body, visitor);
            }
            Stmt::Loop { body, .. } => visit_block(body, visitor),
            Stmt::For { iterable, body, .. } => {
                visit_expression(iterable, visitor);
                visit_block(body, visitor);
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    visit_expression(value, visitor);
                }
            }
            Stmt::Expr { expression, .. } => visit_expression(expression, visitor),
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Struct { .. }
            | Stmt::Enum { .. }
            | Stmt::TypeAlias { .. }
            | Stmt::Trait { .. }
            | Stmt::Continue { .. } => {}
            Stmt::Public { .. } => unreachable!("public statements were unwrapped"),
        }
    }
}

fn visit_block(block: &Block, visitor: &mut impl FnMut(&Expr)) {
    visit_statements(&block.statements, visitor);
}

fn visit_expression(expression: &Expr, visitor: &mut impl FnMut(&Expr)) {
    visitor(expression);
    match expression {
        Expr::Member { object, .. }
        | Expr::Borrow { target: object, .. }
        | Expr::Unary {
            operand: object, ..
        }
        | Expr::Cast {
            operand: object, ..
        }
        | Expr::Try {
            operand: object, ..
        } => visit_expression(object, visitor),
        Expr::Index { object, index, .. } => {
            visit_expression(object, visitor);
            visit_expression(index, visitor);
        }
        Expr::Tuple { elements, .. } | Expr::Array { elements, .. } => {
            for element in elements {
                visit_expression(element, visitor);
            }
            if let Expr::Array {
                repeat: Some(repeat),
                ..
            } = expression
            {
                visit_expression(repeat, visitor);
            }
        }
        Expr::RecordLiteral { fields, .. } => {
            for field in fields {
                visit_expression(&field.value, visitor);
            }
        }
        Expr::Assign { target, value, .. } => {
            visit_expression(target, visitor);
            visit_expression(value, visitor);
        }
        Expr::Binary { left, right, .. }
        | Expr::Logical { left, right, .. }
        | Expr::Range {
            start: left,
            end: right,
            ..
        } => {
            visit_expression(left, visitor);
            visit_expression(right, visitor);
        }
        Expr::Call {
            callee, arguments, ..
        } => {
            visit_expression(callee, visitor);
            for argument in arguments {
                visit_expression(argument, visitor);
            }
        }
        Expr::If {
            condition,
            then_branch,
            else_branch,
            ..
        } => {
            visit_expression(condition, visitor);
            visit_block(then_branch, visitor);
            if let Some(else_branch) = else_branch {
                visit_expression(else_branch, visitor);
            }
        }
        Expr::Match { value, arms, .. } => {
            visit_expression(value, visitor);
            for arm in arms {
                visit_expression(&arm.expression, visitor);
            }
        }
        Expr::Block(block) => visit_block(block, visitor),
        Expr::Literal { .. }
        | Expr::Variable { .. }
        | Expr::Path { .. }
        | Expr::QualifiedPath { .. } => {}
    }
}

#[cfg(test)]
#[path = "../tests/unit/semantic.rs"]
mod tests;
