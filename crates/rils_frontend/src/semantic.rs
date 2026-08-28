use std::collections::{HashMap, HashSet};

use crate::{
    BodyId, DefId, ExprId, ImplId, SourceId, Span, Type,
    analysis::SymbolOccurrence,
    ast::{Expr, Program, Stmt},
    types::FunctionSignature,
};

mod visit;

use visit::visit_statements;

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

    pub(crate) fn extend(&mut self, other: Self) {
        self.definitions.extend(other.definitions);
        self.resolutions.extend(other.resolutions);
        self.bodies.extend(other.bodies);
        self.definition_bodies.extend(other.definition_bodies);
        self.impls.extend(other.impls);
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
    Import {
        name: String,
        signature: FunctionSignature,
        capability: String,
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
    resolved_values: HashMap<ExprId, DefId>,
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
            resolved_values: HashMap::new(),
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

    pub fn resolved_value_at(&self, span: Span) -> Option<DefId> {
        self.expression_id(span)
            .and_then(|id| self.resolved_values.get(&id).copied())
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

    pub(crate) fn extend(&mut self, other: Self) {
        self.expression_ids.extend(other.expression_ids);
        self.expression_types.extend(other.expression_types);
        self.resolved_calls.extend(other.resolved_calls);
        self.resolved_values.extend(other.resolved_values);
    }

    pub(crate) fn resolve_call(&mut self, span: Span, call: ResolvedCall) {
        if let Some(id) = self.expression_id(span) {
            self.resolved_calls.insert(id, call);
        }
    }

    fn resolve_value(&mut self, span: Span, definition: DefId) {
        if let Some(id) = self.expression_id(span) {
            self.resolved_values.insert(id, definition);
        }
    }
}

pub(crate) fn resolve_program_calls(
    program: &Program,
    definitions: &DefMap,
    host_functions: &HashMap<String, FunctionSignature>,
    results: &mut TypeckResults,
    module_path: &[String],
) {
    resolve_project_calls(
        &[(module_path, program)],
        definitions,
        host_functions,
        results,
    );
}

pub(crate) fn resolve_project_calls(
    units: &[(&[String], &Program)],
    definitions: &DefMap,
    host_functions: &HashMap<String, FunctionSignature>,
    results: &mut TypeckResults,
) {
    let mut iterator_types = HashSet::new();
    let mut callables = CallableDefinitions::default();
    for (module_path, program) in units {
        collect_trait_implementations(
            &program.statements,
            &mut module_path.to_vec(),
            "Iterator",
            &mut iterator_types,
        );
        collect_callable_definitions(
            &program.statements,
            &mut module_path.to_vec(),
            definitions,
            &mut callables,
        );
    }
    for (module_path, program) in units {
        collect_callable_aliases(
            &program.statements,
            &mut module_path.to_vec(),
            &mut callables,
        );
        collect_host_aliases(
            &program.statements,
            &mut module_path.to_vec(),
            host_functions,
            &mut callables.host_aliases,
        );
    }
    for (module_path, program) in units {
        visit_statements(
            &program.statements,
            &mut module_path.to_vec(),
            None,
            &mut |expression, namespace, self_type| {
                if let Some(definition) =
                    callables.resolve(expression, results, namespace, self_type)
                {
                    results.resolve_value(expression.span(), definition);
                }
                let Expr::Call { callee, span, .. } = expression else {
                    return;
                };
                let context = CallResolutionContext {
                    definitions,
                    callables: &callables,
                    host_functions,
                    iterator_types: &iterator_types,
                    results,
                    namespace,
                    self_type,
                };
                if let Some(call) = resolve_callee(callee, &context) {
                    results.resolve_call(*span, call);
                }
            },
        );
    }
}

struct CallResolutionContext<'a> {
    definitions: &'a DefMap,
    callables: &'a CallableDefinitions,
    host_functions: &'a HashMap<String, FunctionSignature>,
    iterator_types: &'a HashSet<String>,
    results: &'a TypeckResults,
    namespace: &'a [String],
    self_type: Option<&'a str>,
}

fn resolve_callee(callee: &Expr, context: &CallResolutionContext<'_>) -> Option<ResolvedCall> {
    let CallResolutionContext {
        definitions,
        callables,
        host_functions,
        iterator_types,
        results,
        namespace,
        self_type,
    } = context;
    if let Some(definition) = callables.resolve(callee, results, namespace, *self_type) {
        return Some(ResolvedCall::Definition(definition));
    }
    if let Some(definition) = callables.resolve_untyped_member(callee) {
        return Some(ResolvedCall::Definition(definition));
    }
    if let Some(path) = callables.resolve_host(callee, host_functions, namespace, *self_type) {
        return Some(ResolvedCall::Host { path });
    }
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
                .or_else(|| unqualified_builtin_member(name));
            if let Some(member) = member {
                return Some(ResolvedCall::Builtin {
                    id: member.builtin_id?,
                    kind: BuiltinCallKind::Runtime,
                    receiver: member.receiver,
                });
            }
            let Type::Named { name: owner, .. } = receiver else {
                return None;
            };
            let path = format!("{owner}::{name}");
            host_functions
                .contains_key(&path)
                .then_some(ResolvedCall::Host { path })
        }
        Expr::Path { segments, .. } => {
            let path = segments.join("::");
            if let [type_name, member] = segments.as_slice()
                && crate::IntegerType::from_name(type_name).is_some()
                && let Some(intrinsic) = rils_builtins::integer_associated_function(member)
            {
                return Some(ResolvedCall::Builtin {
                    id: intrinsic.id,
                    kind: BuiltinCallKind::Intrinsic,
                    receiver: None,
                });
            }
            if let Some(import) = builtin_associated_import(&path) {
                return Some(import);
            }
            standard_import(&path).or_else(|| {
                host_functions
                    .contains_key(&path)
                    .then_some(ResolvedCall::Host { path })
            })
        }
        Expr::Variable { name, .. } => native_macro_import(name)
            .or_else(|| standard_import(name))
            .or_else(|| {
                host_functions
                    .contains_key(name)
                    .then(|| ResolvedCall::Host { path: name.clone() })
            }),
        _ => None,
    }
}

fn builtin_associated_import(path: &str) -> Option<ResolvedCall> {
    let (owner_path, member_name) = path.rsplit_once("::")?;
    let owner = owner_path.rsplit("::").next()?;
    let member = rils_builtins::builtin_member(owner, member_name)?;
    let name = member.runtime_import?;
    let signature =
        crate::standard_library::builtin_associated_function_signature(owner, member_name)?;
    Some(ResolvedCall::Import {
        name: name.into(),
        signature,
        capability: "core".into(),
    })
}

fn standard_import(path: &str) -> Option<ResolvedCall> {
    let declaration = rils_builtins::builtin_function(path)?;
    let signature = crate::standard_library::standard_function_signature(path)?;
    let capability = match declaration.backend {
        rils_builtins::BuiltinBackend::Host(capability) => capability,
        rils_builtins::BuiltinBackend::Runtime => "core",
        rils_builtins::BuiltinBackend::Intrinsic | rils_builtins::BuiltinBackend::Metadata => {
            return None;
        }
    };
    Some(ResolvedCall::Import {
        name: path.into(),
        signature,
        capability: capability.into(),
    })
}

fn native_macro_import(name: &str) -> Option<ResolvedCall> {
    let (path, capability, signature) = match name {
        "#rils_native_print" => (
            "std::io::print",
            "std::io",
            crate::standard_library::standard_function_signature("std::io::print")?,
        ),
        "#rils_native_println" => (
            "std::io::println",
            "std::io",
            crate::standard_library::standard_function_signature("std::io::println")?,
        ),
        "#rils_native_assert" => (
            "core::assert",
            "core",
            FunctionSignature::variadic(Type::Unit),
        ),
        _ => return None,
    };
    Some(ResolvedCall::Import {
        name: path.into(),
        signature,
        capability: capability.into(),
    })
}

#[derive(Default)]
struct CallableDefinitions {
    functions: HashMap<String, DefId>,
    methods: Vec<MethodDefinition>,
    host_aliases: HashMap<String, String>,
    path_aliases: HashMap<String, String>,
}

struct MethodDefinition {
    owner: String,
    trait_name: Option<String>,
    name: String,
    definition: DefId,
}

impl CallableDefinitions {
    fn resolve(
        &self,
        callee: &Expr,
        results: &TypeckResults,
        namespace: &[String],
        self_type: Option<&str>,
    ) -> Option<DefId> {
        match callee {
            Expr::Variable { name, .. } => self
                .functions
                .get(&self.resolve_path(namespace, self_type, name))
                .copied(),
            Expr::Path { segments, .. } => {
                let path = self.resolve_path(namespace, self_type, &segments.join("::"));
                if let Some(definition) = self.functions.get(&path) {
                    return Some(*definition);
                }
                let (owner, name) = path.rsplit_once("::")?;
                unique_method(self.methods.iter().filter(|method| {
                    owner_matches(&method.owner, owner)
                        && method.name == name
                        && method.trait_name.is_none()
                }))
            }
            Expr::QualifiedPath {
                target,
                trait_name,
                member,
                ..
            } => {
                let Type::Named { name: owner, .. } = target else {
                    return None;
                };
                let owner = contextual_name(namespace, self_type, owner);
                let trait_name = contextual_name(namespace, self_type, trait_name);
                unique_method(self.methods.iter().filter(|method| {
                    owner_matches(&method.owner, &owner)
                        && method.trait_name.as_deref() == Some(trait_name.as_str())
                        && method.name == *member
                }))
            }
            Expr::Member { object, name, .. } => {
                let receiver = results.expression_type_at(object.span())?;
                let receiver = match receiver {
                    Type::Reference { inner, .. } => inner.as_ref(),
                    receiver => receiver,
                };
                let Type::Named { name: owner, .. } = receiver else {
                    return None;
                };
                let owner = contextual_name(namespace, self_type, owner);
                let inherent = self.methods.iter().filter(|method| {
                    owner_matches(&method.owner, &owner)
                        && method.name == *name
                        && method.trait_name.is_none()
                });
                unique_method(inherent).or_else(|| {
                    unique_method(self.methods.iter().filter(|method| {
                        owner_matches(&method.owner, &owner)
                            && method.name == *name
                            && method.trait_name.is_some()
                    }))
                })
            }
            _ => None,
        }
    }

    fn resolve_host(
        &self,
        callee: &Expr,
        host_functions: &HashMap<String, FunctionSignature>,
        namespace: &[String],
        self_type: Option<&str>,
    ) -> Option<String> {
        let name = match callee {
            Expr::Variable { name, .. } => contextual_name(namespace, self_type, name),
            Expr::Path { segments, .. } => {
                contextual_name(namespace, self_type, &segments.join("::"))
            }
            _ => return None,
        };
        self.host_aliases
            .get(&name)
            .cloned()
            .or_else(|| host_functions.contains_key(&name).then_some(name))
    }

    fn resolve_untyped_member(&self, callee: &Expr) -> Option<DefId> {
        let Expr::Member { name, .. } = callee else {
            return None;
        };
        unique_method(self.methods.iter().filter(|method| method.name == *name))
    }

    fn resolve_path(&self, namespace: &[String], self_type: Option<&str>, name: &str) -> String {
        let name = contextual_name(namespace, self_type, name);
        let alias = self
            .path_aliases
            .iter()
            .filter(|(alias, _)| name == alias.as_str() || name.starts_with(&format!("{alias}::")))
            .max_by_key(|(alias, _)| alias.len());
        match alias {
            Some((alias, target)) => format!("{target}{}", &name[alias.len()..]),
            None => name,
        }
    }
}

fn owner_matches(candidate: &str, requested: &str) -> bool {
    candidate == requested
        || (!requested.contains("::") && candidate.ends_with(&format!("::{requested}")))
}

fn unique_method<'a>(mut methods: impl Iterator<Item = &'a MethodDefinition>) -> Option<DefId> {
    let first = methods.next()?.definition;
    methods.next().is_none().then_some(first)
}

fn collect_callable_definitions(
    statements: &[Stmt],
    namespace: &mut Vec<String>,
    definitions: &DefMap,
    output: &mut CallableDefinitions,
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
                namespace.push(name.clone());
                collect_callable_definitions(statements, namespace, definitions, output);
                namespace.pop();
            }
            Stmt::Function {
                name, name_span, ..
            } => {
                if let Some(definition) = definitions.resolution(*name_span) {
                    output
                        .functions
                        .insert(qualified_name(namespace, name), definition);
                }
            }
            Stmt::Impl {
                target,
                trait_name,
                methods,
                ..
            } => {
                let Type::Named { name: owner, .. } = target else {
                    continue;
                };
                let owner = qualified_name(namespace, owner);
                let trait_name = trait_name
                    .as_ref()
                    .map(|trait_name| qualified_name(namespace, trait_name));
                output.methods.extend(methods.iter().filter_map(|method| {
                    Some(MethodDefinition {
                        owner: owner.clone(),
                        trait_name: trait_name.clone(),
                        name: method.name.clone(),
                        definition: definitions.resolution(method.name_span)?,
                    })
                }));
            }
            _ => {}
        }
    }
}

fn collect_host_aliases(
    statements: &[Stmt],
    namespace: &mut Vec<String>,
    host_functions: &HashMap<String, FunctionSignature>,
    output: &mut HashMap<String, String>,
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
                namespace.push(name.clone());
                collect_host_aliases(statements, namespace, host_functions, output);
                namespace.pop();
            }
            Stmt::Use { imports, .. } => {
                for import in imports {
                    let path = import.path.join("::");
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let prefix = format!("{path}::");
                        for candidate in host_functions.keys() {
                            let Some(member) = candidate.strip_prefix(&prefix) else {
                                continue;
                            };
                            if !member.contains("::") {
                                output.insert(qualified_name(namespace, member), candidate.clone());
                            }
                        }
                    } else if host_functions.contains_key(&path)
                        && let Some(binding) = import.binding_name()
                    {
                        output.insert(qualified_name(namespace, binding), path);
                    }
                }
            }
            _ => {}
        }
    }
}

fn collect_callable_aliases(
    statements: &[Stmt],
    namespace: &mut Vec<String>,
    callables: &mut CallableDefinitions,
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
                namespace.push(name.clone());
                collect_callable_aliases(statements, namespace, callables);
                namespace.pop();
            }
            Stmt::Use { imports, .. } => {
                for import in imports {
                    let path = contextual_name(namespace, None, &import.path.join("::"));
                    if import.kind == crate::ast::UseImportKind::Glob {
                        let prefix = format!("{path}::");
                        let aliases = callables
                            .functions
                            .iter()
                            .filter_map(|(candidate, definition)| {
                                let member = candidate.strip_prefix(&prefix)?;
                                (!member.contains("::")).then_some((member.to_owned(), *definition))
                            })
                            .collect::<Vec<_>>();
                        for (member, definition) in aliases {
                            callables
                                .functions
                                .insert(qualified_name(namespace, &member), definition);
                        }
                        let modules = callables
                            .functions
                            .keys()
                            .filter_map(|candidate| {
                                let remaining = candidate.strip_prefix(&prefix)?;
                                remaining
                                    .split_once("::")
                                    .map(|(module, _)| module.to_owned())
                            })
                            .collect::<HashSet<_>>();
                        for module in modules {
                            callables.path_aliases.insert(
                                qualified_name(namespace, &module),
                                format!("{path}::{module}"),
                            );
                        }
                    } else if let Some(binding) = import.binding_name() {
                        let binding = qualified_name(namespace, binding);
                        if let Some(definition) = callables.functions.get(&path).copied() {
                            callables.functions.insert(binding, definition);
                        } else if callables
                            .functions
                            .keys()
                            .any(|candidate| candidate.starts_with(&format!("{path}::")))
                        {
                            callables.path_aliases.insert(binding, path);
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn qualified_name(namespace: &[String], name: &str) -> String {
    if namespace.is_empty() || name.contains("::") {
        name.to_owned()
    } else {
        format!("{}::{name}", namespace.join("::"))
    }
}

fn contextual_name(namespace: &[String], self_type: Option<&str>, name: &str) -> String {
    if name == "Self" {
        return self_type.unwrap_or(name).to_owned();
    }
    if let Some(suffix) = name.strip_prefix("Self::") {
        return self_type
            .map(|self_type| format!("{self_type}::{suffix}"))
            .unwrap_or_else(|| name.to_owned());
    }
    if let Some(name) = name.strip_prefix("crate::") {
        return name.to_owned();
    }
    if let Some(name) = name.strip_prefix("self::") {
        return qualified_name(namespace, name);
    }
    if name.starts_with("super::") {
        let mut parent = namespace.to_vec();
        let mut remainder = name;
        while let Some(stripped) = remainder.strip_prefix("super::") {
            parent.pop();
            remainder = stripped;
        }
        return qualified_name(&parent, remainder);
    }
    qualified_name(namespace, name)
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

#[cfg(test)]
#[path = "../tests/unit/semantic.rs"]
mod tests;
