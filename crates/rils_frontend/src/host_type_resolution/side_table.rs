use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    ExprId, PatternId, SourceId, TypeRefId,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt, UseImport, UseImportKind},
    semantic::{ExpressionIdentityMap, PatternIdentityMap, TypeIdentityMap},
    source::Span,
    types::Type,
};

use super::{HostTypeResolutionError, path_candidates};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HostTypeResolutionResults {
    type_names: HashMap<TypeRefId, String>,
    expression_paths: HashMap<ExprId, Vec<String>>,
    pattern_paths: HashMap<PatternId, Vec<String>>,
    errors: Vec<HostTypeResolutionError>,
}

/// Read-only canonical host names for nodes in one immutable [`Program`].
///
/// The view owns the syntax-to-semantic identity indexes for the program it
/// was constructed from. Callers can therefore consume host resolution
/// without mutating or cloning the AST and without using source spans as
/// semantic keys.
pub struct HostTypeResolutionView<'a> {
    results: &'a HostTypeResolutionResults,
    type_ids: TypeIdentityMap,
    expression_ids: ExpressionIdentityMap,
    pattern_ids: PatternIdentityMap,
}

impl<'a> HostTypeResolutionView<'a> {
    pub fn new(
        program: &Program,
        fallback_source: SourceId,
        results: &'a HostTypeResolutionResults,
    ) -> Self {
        Self {
            results,
            type_ids: TypeIdentityMap::allocate(program, fallback_source),
            expression_ids: ExpressionIdentityMap::allocate(program, fallback_source),
            pattern_ids: PatternIdentityMap::allocate(program, fallback_source),
        }
    }

    /// Returns a recursively canonicalized copy of an AST type.
    pub fn resolved_type(&self, ty: &Type) -> Type {
        match ty {
            Type::Option(inner) => Type::Option(Box::new(self.resolved_type(inner))),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.resolved_type(ok)),
                Box::new(self.resolved_type(error)),
            ),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.resolved_type(element))
                    .collect(),
            ),
            Type::Array { element, length } => Type::Array {
                element: Box::new(self.resolved_type(element)),
                length: *length,
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.resolved_type(inner)),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| self.resolved_type(parameter))
                        .collect()
                }),
                return_type: Box::new(self.resolved_type(return_type)),
            },
            Type::Named { name, arguments } => Type::Named {
                name: self
                    .type_ids
                    .get(ty)
                    .and_then(|id| self.results.type_name(id))
                    .unwrap_or(name)
                    .to_owned(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolved_type(argument))
                    .collect(),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: Box::new(self.resolved_type(base)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.resolved_type(argument))
                    .collect(),
            },
            other => other.clone(),
        }
    }

    pub fn resolved_expression_path(&self, expression: &Expr) -> Option<&[String]> {
        self.expression_ids
            .get(expression)
            .and_then(|id| self.results.expression_path(id))
    }

    pub fn resolved_pattern_path(&self, pattern: &Pattern) -> Option<&[String]> {
        self.pattern_ids
            .get(pattern)
            .and_then(|id| self.results.pattern_path(id))
    }
}

impl HostTypeResolutionResults {
    pub fn type_name(&self, id: TypeRefId) -> Option<&str> {
        self.type_names.get(&id).map(String::as_str)
    }

    pub fn expression_path(&self, id: ExprId) -> Option<&[String]> {
        self.expression_paths.get(&id).map(Vec::as_slice)
    }

    pub fn pattern_path(&self, id: PatternId) -> Option<&[String]> {
        self.pattern_paths.get(&id).map(Vec::as_slice)
    }

    pub fn errors(&self) -> &[HostTypeResolutionError] {
        &self.errors
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.type_names.extend(other.type_names);
        self.expression_paths.extend(other.expression_paths);
        self.pattern_paths.extend(other.pattern_paths);
        self.errors.extend(other.errors);
    }
}

pub fn resolve_host_types(
    program: &Program,
    fallback_source: SourceId,
    host_types: &HashSet<String>,
) -> HostTypeResolutionResults {
    if host_types.is_empty() {
        return HostTypeResolutionResults::default();
    }
    Resolver::new(program, fallback_source, host_types).resolve_program(program)
}

#[derive(Clone, Debug)]
enum TypeBinding {
    Local,
    Explicit(String),
    Glob(BTreeSet<String>),
}

#[derive(Default)]
struct Scope {
    types: HashMap<String, TypeBinding>,
}

struct Resolver<'a> {
    host_types: &'a HashSet<String>,
    type_ids: TypeIdentityMap,
    expression_ids: ExpressionIdentityMap,
    pattern_ids: PatternIdentityMap,
    scopes: Vec<Scope>,
    module_path: Vec<String>,
    results: HostTypeResolutionResults,
}

impl<'a> Resolver<'a> {
    fn new(program: &Program, fallback_source: SourceId, host_types: &'a HashSet<String>) -> Self {
        Self {
            host_types,
            type_ids: TypeIdentityMap::allocate(program, fallback_source),
            expression_ids: ExpressionIdentityMap::allocate(program, fallback_source),
            pattern_ids: PatternIdentityMap::allocate(program, fallback_source),
            scopes: vec![Scope::default()],
            module_path: Vec::new(),
            results: HostTypeResolutionResults::default(),
        }
    }

    fn resolve_program(mut self, program: &Program) -> HostTypeResolutionResults {
        self.resolve_scope(&program.statements, false);
        self.results
    }

    fn resolve_scope(&mut self, statements: &[Stmt], nested: bool) {
        if nested {
            self.scopes.push(Scope::default());
        }
        self.collect_local_types(statements);
        self.collect_imports(statements);
        for statement in statements {
            self.resolve_statement(statement);
        }
        if nested {
            self.scopes.pop();
        }
    }

    fn collect_local_types(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let name = match statement {
                Stmt::Struct { name, .. }
                | Stmt::Enum { name, .. }
                | Stmt::TypeAlias { name, .. }
                | Stmt::Trait { name, .. } => Some(name),
                _ => None,
            };
            if let Some(name) = name {
                self.current_scope()
                    .types
                    .insert(name.clone(), TypeBinding::Local);
            }
        }
    }

    fn collect_imports(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let Stmt::Use { imports, .. } = statement else {
                continue;
            };
            for import in imports {
                self.collect_import(import);
            }
        }
    }

    fn collect_import(&mut self, import: &UseImport) {
        let candidates = path_candidates(&self.module_path, &import.path);
        match import.kind {
            UseImportKind::Single => {
                let Some(canonical) = candidates
                    .iter()
                    .find(|candidate| self.host_types.contains(*candidate))
                    .cloned()
                else {
                    return;
                };
                let alias = import.binding_name().expect("single import has a binding");
                self.insert_explicit(alias, canonical);
            }
            UseImportKind::Glob => {
                let imported = candidates
                    .iter()
                    .flat_map(|module| self.immediate_host_types(module))
                    .collect::<Vec<_>>();
                for (alias, canonical) in imported {
                    self.insert_glob(alias, canonical);
                }
            }
        }
    }

    fn immediate_host_types(&self, module: &str) -> Vec<(String, String)> {
        let prefix = format!("{module}::");
        self.host_types
            .iter()
            .filter_map(|canonical| {
                let member = canonical.strip_prefix(&prefix)?;
                (!member.contains("::")).then(|| (member.to_owned(), canonical.clone()))
            })
            .collect()
    }

    fn insert_explicit(&mut self, alias: &str, canonical: String) {
        use std::collections::hash_map::Entry;
        match self.current_scope().types.entry(alias.to_owned()) {
            Entry::Vacant(entry) => {
                entry.insert(TypeBinding::Explicit(canonical));
            }
            Entry::Occupied(mut entry) => match entry.get() {
                TypeBinding::Local => {}
                TypeBinding::Explicit(existing) if existing == &canonical => {}
                TypeBinding::Explicit(existing) => {
                    let mut candidates = BTreeSet::from([existing.clone(), canonical]);
                    entry.insert(TypeBinding::Glob(std::mem::take(&mut candidates)));
                }
                TypeBinding::Glob(_) => {
                    entry.insert(TypeBinding::Explicit(canonical));
                }
            },
        }
    }

    fn insert_glob(&mut self, alias: String, canonical: String) {
        use std::collections::hash_map::Entry;
        match self.current_scope().types.entry(alias) {
            Entry::Vacant(entry) => {
                entry.insert(TypeBinding::Glob(BTreeSet::from([canonical])));
            }
            Entry::Occupied(mut entry) => match entry.get_mut() {
                TypeBinding::Glob(candidates) => {
                    candidates.insert(canonical);
                }
                TypeBinding::Local | TypeBinding::Explicit(_) => {}
            },
        }
    }

    fn resolve_statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Module {
                name,
                statements: Some(statements),
                ..
            } => {
                self.module_path.push(name.clone());
                self.resolve_scope(statements, true);
                self.module_path.pop();
            }
            Stmt::Let {
                type_annotation,
                initializer,
                span,
                ..
            } => {
                self.resolve_optional_type(type_annotation.as_ref(), *span);
                self.resolve_expression(initializer);
            }
            Stmt::Function {
                parameters,
                return_type,
                body,
                span,
                ..
            } => {
                for parameter in parameters {
                    self.resolve_optional_type(parameter.type_annotation.as_ref(), parameter.span);
                }
                self.resolve_optional_type(return_type.as_ref(), *span);
                self.resolve_block(body);
            }
            Stmt::Struct { fields, .. } => {
                for field in fields {
                    self.resolve_type(&field.type_annotation, field.span);
                }
            }
            Stmt::Enum { variants, .. } => {
                for variant in variants {
                    match variant {
                        EnumVariant::Unit { .. } => {}
                        EnumVariant::Tuple { fields, span, .. } => {
                            for field in fields {
                                self.resolve_type(field, *span);
                            }
                        }
                        EnumVariant::Record { fields, .. } => {
                            for field in fields {
                                self.resolve_type(&field.type_annotation, field.span);
                            }
                        }
                    }
                }
            }
            Stmt::TypeAlias { target, span, .. } => self.resolve_type(target, *span),
            Stmt::Impl {
                target,
                associated_types,
                methods,
                span,
                ..
            } => {
                self.resolve_type(target, *span);
                for associated in associated_types {
                    self.resolve_optional_type(associated.value.as_ref(), associated.span);
                }
                for method in methods {
                    for parameter in &method.parameters {
                        self.resolve_optional_type(
                            parameter.type_annotation.as_ref(),
                            parameter.span,
                        );
                    }
                    self.resolve_optional_type(method.return_type.as_ref(), method.span);
                    self.resolve_block(&method.body);
                }
            }
            Stmt::Trait {
                associated_types,
                methods,
                ..
            } => {
                for associated in associated_types {
                    self.resolve_optional_type(associated.value.as_ref(), associated.span);
                }
                for method in methods {
                    for parameter in &method.parameters {
                        self.resolve_optional_type(
                            parameter.type_annotation.as_ref(),
                            parameter.span,
                        );
                    }
                    self.resolve_optional_type(method.return_type.as_ref(), method.span);
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.resolve_expression(condition);
                self.resolve_block(body);
            }
            Stmt::Loop { body, .. } => self.resolve_block(body),
            Stmt::For { iterable, body, .. } => {
                self.resolve_expression(iterable);
                self.resolve_block(body);
            }
            Stmt::Return { value, .. } | Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.resolve_expression(value);
                }
            }
            Stmt::Expr { expression, .. } => self.resolve_expression(expression),
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Continue { .. } => {}
        }
    }

    fn resolve_optional_type(&mut self, ty: Option<&Type>, span: Span) {
        if let Some(ty) = ty {
            self.resolve_type(ty, span);
        }
    }

    fn resolve_type(&mut self, ty: &Type, span: Span) {
        match ty {
            Type::Option(inner) => self.resolve_type(inner, span),
            Type::Result(ok, error) => {
                self.resolve_type(ok, span);
                self.resolve_type(error, span);
            }
            Type::Tuple(elements) => {
                for element in elements {
                    self.resolve_type(element, span);
                }
            }
            Type::Array { element, .. } | Type::Reference { inner: element, .. } => {
                self.resolve_type(element, span)
            }
            Type::Function {
                parameters,
                return_type,
            } => {
                if let Some(parameters) = parameters {
                    for parameter in parameters {
                        self.resolve_type(parameter, span);
                    }
                }
                self.resolve_type(return_type, span);
            }
            Type::Named { name, arguments } => {
                for argument in arguments {
                    self.resolve_type(argument, span);
                }
                if let Some(canonical) = self.resolve_name(name, span) {
                    let id = self
                        .type_ids
                        .get(ty)
                        .expect("visited type must have a semantic identity");
                    self.results.type_names.insert(id, canonical);
                }
            }
            Type::Associated {
                base, arguments, ..
            } => {
                self.resolve_type(base, span);
                for argument in arguments {
                    self.resolve_type(argument, span);
                }
            }
            _ => {}
        }
    }

    fn resolve_expression(&mut self, expression: &Expr) {
        match expression {
            Expr::QualifiedPath { target, span, .. } | Expr::Cast { target, span, .. } => {
                self.resolve_type(target, *span);
                if let Expr::Cast { operand, .. } = expression {
                    self.resolve_expression(operand);
                }
            }
            Expr::Member { object, .. }
            | Expr::Try {
                operand: object, ..
            }
            | Expr::Borrow { target: object, .. }
            | Expr::Unary {
                operand: object, ..
            } => self.resolve_expression(object),
            Expr::Index { object, index, .. }
            | Expr::Assign {
                target: object,
                value: index,
                ..
            }
            | Expr::Binary {
                left: object,
                right: index,
                ..
            }
            | Expr::Logical {
                left: object,
                right: index,
                ..
            }
            | Expr::Range {
                start: object,
                end: index,
                ..
            } => {
                self.resolve_expression(object);
                self.resolve_expression(index);
            }
            Expr::Tuple { elements, .. } => {
                for element in elements {
                    self.resolve_expression(element);
                }
            }
            Expr::Array {
                elements, repeat, ..
            } => {
                for element in elements {
                    self.resolve_expression(element);
                }
                if let Some(repeat) = repeat {
                    self.resolve_expression(repeat);
                }
            }
            Expr::RecordLiteral { fields, .. } => {
                for field in fields {
                    self.resolve_expression(&field.value);
                }
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                self.resolve_expression(callee);
                for argument in arguments {
                    self.resolve_expression(argument);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.resolve_expression(condition);
                self.resolve_block(then_branch);
                if let Some(else_branch) = else_branch {
                    self.resolve_expression(else_branch);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.resolve_expression(value);
                for arm in arms {
                    self.resolve_pattern(&arm.pattern);
                    self.resolve_expression(&arm.expression);
                }
            }
            Expr::Block(block) => self.resolve_block(block),
            Expr::Path { segments, span } => {
                if let Some(path) = self.resolve_path(segments, *span) {
                    let id = self
                        .expression_ids
                        .get(expression)
                        .expect("visited expression must have a semantic identity");
                    self.results.expression_paths.insert(id, path);
                }
            }
            Expr::Literal { .. } | Expr::Variable { .. } => {}
        }
    }

    fn resolve_pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Path { path, span }
            | Pattern::TupleVariant { path, span, .. }
            | Pattern::Record { path, span, .. } => {
                if let Some(path) = self.resolve_path(path, *span) {
                    let id = self
                        .pattern_ids
                        .get(pattern)
                        .expect("visited pattern must have a semantic identity");
                    self.results.pattern_paths.insert(id, path);
                }
            }
            Pattern::Some { inner, .. }
            | Pattern::Ok { inner, .. }
            | Pattern::Err { inner, .. } => self.resolve_pattern(inner),
            Pattern::Wildcard { .. }
            | Pattern::Binding { .. }
            | Pattern::Literal { .. }
            | Pattern::None { .. } => {}
        }
        match pattern {
            Pattern::TupleVariant { fields, .. } => {
                for field in fields {
                    self.resolve_pattern(field);
                }
            }
            Pattern::Record { fields, .. } => {
                for (_, field) in fields {
                    self.resolve_pattern(field);
                }
            }
            _ => {}
        }
    }

    fn resolve_block(&mut self, block: &Block) {
        for statement in &block.statements {
            self.resolve_statement(statement);
        }
    }

    fn resolve_path(&mut self, segments: &[String], span: Span) -> Option<Vec<String>> {
        if segments.len() <= 1 {
            return None;
        }
        let canonical = self.resolve_name(&segments[0], span)?;
        let mut resolved = canonical.split("::").map(str::to_owned).collect::<Vec<_>>();
        resolved.extend(segments.iter().skip(1).cloned());
        Some(resolved)
    }

    fn resolve_name(&mut self, name: &str, span: Span) -> Option<String> {
        if self.host_types.contains(name) {
            return Some(name.to_owned());
        }
        if name.contains("::") {
            return path_candidates(
                &self.module_path,
                &name.split("::").map(str::to_owned).collect::<Vec<_>>(),
            )
            .into_iter()
            .find(|candidate| self.host_types.contains(candidate));
        }
        for scope in self.scopes.iter().rev() {
            let Some(binding) = scope.types.get(name) else {
                continue;
            };
            return match binding {
                TypeBinding::Local => None,
                TypeBinding::Explicit(canonical) => Some(canonical.clone()),
                TypeBinding::Glob(candidates) if candidates.len() == 1 => {
                    candidates.first().cloned()
                }
                TypeBinding::Glob(candidates) => {
                    self.results.errors.push(HostTypeResolutionError {
                        message: format!(
                            "host type `{name}` is ambiguous; candidates: {}",
                            candidates.iter().cloned().collect::<Vec<_>>().join(", ")
                        ),
                        span,
                    });
                    None
                }
            };
        }
        let candidates = self
            .host_types
            .iter()
            .filter(|canonical| {
                canonical
                    .rsplit_once("::")
                    .is_some_and(|(_, item)| item == name)
            })
            .cloned()
            .collect::<BTreeSet<_>>();
        if !candidates.is_empty() {
            self.results.errors.push(HostTypeResolutionError {
                message: format!(
                    "host type `{name}` is not in scope; import one of: {}",
                    candidates.into_iter().collect::<Vec<_>>().join(", ")
                ),
                span,
            });
        }
        None
    }

    fn current_scope(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("host type scope exists")
    }
}
