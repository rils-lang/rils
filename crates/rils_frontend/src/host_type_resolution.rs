//! Canonical resolution for named host types imported into Rils source.

use std::collections::{BTreeSet, HashMap, HashSet};

use crate::{
    ast::{
        Block, EnumVariant, Expr, ImplMethod, NamedField, Parameter, Program, Stmt, TraitMethod,
        UseImport, UseImportKind,
    },
    source::Span,
    types::Type,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostTypeResolutionError {
    pub message: String,
    pub span: Span,
}

/// Rewrites imported host type names to their canonical manifest identities.
///
/// This pass deliberately runs before type inference so every downstream consumer sees the same
/// identity regardless of whether source used a full path, a glob import, or an explicit alias.
pub fn resolve_host_type_names(
    program: &mut Program,
    host_types: &HashSet<String>,
) -> Vec<HostTypeResolutionError> {
    if host_types.is_empty() {
        return Vec::new();
    }
    Resolver::new(host_types).resolve_program(program)
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
    scopes: Vec<Scope>,
    module_path: Vec<String>,
    errors: Vec<HostTypeResolutionError>,
}

impl<'a> Resolver<'a> {
    fn new(host_types: &'a HashSet<String>) -> Self {
        Self {
            host_types,
            scopes: vec![Scope::default()],
            module_path: Vec::new(),
            errors: Vec::new(),
        }
    }

    fn resolve_program(mut self, program: &mut Program) -> Vec<HostTypeResolutionError> {
        self.resolve_scope(&mut program.statements, false);
        self.errors
    }

    fn resolve_scope(&mut self, statements: &mut [Stmt], nested: bool) {
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
            let statement = public_inner(statement);
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
            let Stmt::Use { imports, .. } = public_inner(statement) else {
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

    fn resolve_statement(&mut self, statement: &mut Stmt) {
        match statement {
            Stmt::Public { statement, .. } => self.resolve_statement(statement),
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
                self.resolve_optional_type(type_annotation, *span);
                self.resolve_expression(initializer);
            }
            Stmt::Function {
                parameters,
                return_type,
                body,
                span,
                ..
            } => {
                self.resolve_parameters(parameters);
                self.resolve_optional_type(return_type, *span);
                self.resolve_block(body);
            }
            Stmt::Struct { fields, .. } => self.resolve_fields(fields),
            Stmt::Enum { variants, .. } => {
                for variant in variants {
                    match variant {
                        EnumVariant::Unit { .. } => {}
                        EnumVariant::Tuple { fields, span, .. } => {
                            for field in fields {
                                self.resolve_type(field, *span);
                            }
                        }
                        EnumVariant::Record { fields, .. } => self.resolve_fields(fields),
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
                    self.resolve_optional_type(&mut associated.value, associated.span);
                }
                for method in methods {
                    self.resolve_impl_method(method);
                }
            }
            Stmt::Trait {
                associated_types,
                methods,
                ..
            } => {
                for associated in associated_types {
                    self.resolve_optional_type(&mut associated.value, associated.span);
                }
                for method in methods {
                    self.resolve_trait_method(method);
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
            Stmt::Return {
                value: Some(value), ..
            }
            | Stmt::Break {
                value: Some(value), ..
            } => self.resolve_expression(value),
            Stmt::Expr { expression, .. } => self.resolve_expression(expression),
            Stmt::Module {
                statements: None, ..
            }
            | Stmt::Use { .. }
            | Stmt::Continue { .. }
            | Stmt::Return { value: None, .. }
            | Stmt::Break { value: None, .. } => {}
        }
    }

    fn resolve_impl_method(&mut self, method: &mut ImplMethod) {
        self.resolve_parameters(&mut method.parameters);
        self.resolve_optional_type(&mut method.return_type, method.span);
        self.resolve_block(&mut method.body);
    }

    fn resolve_trait_method(&mut self, method: &mut TraitMethod) {
        self.resolve_parameters(&mut method.parameters);
        self.resolve_optional_type(&mut method.return_type, method.span);
    }

    fn resolve_parameters(&mut self, parameters: &mut [Parameter]) {
        for parameter in parameters {
            self.resolve_optional_type(&mut parameter.type_annotation, parameter.span);
        }
    }

    fn resolve_fields(&mut self, fields: &mut [NamedField]) {
        for field in fields {
            self.resolve_type(&mut field.type_annotation, field.span);
        }
    }

    fn resolve_optional_type(&mut self, ty: &mut Option<Type>, span: Span) {
        if let Some(ty) = ty {
            self.resolve_type(ty, span);
        }
    }

    fn resolve_type(&mut self, ty: &mut Type, span: Span) {
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
                self.resolve_type(element, span);
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
                    *name = canonical;
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
                    self.errors.push(HostTypeResolutionError {
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
            self.errors.push(HostTypeResolutionError {
                message: format!(
                    "host type `{name}` is not in scope; import one of: {}",
                    candidates.into_iter().collect::<Vec<_>>().join(", ")
                ),
                span,
            });
        }
        None
    }

    fn resolve_block(&mut self, block: &mut Block) {
        for statement in &mut block.statements {
            self.resolve_statement(statement);
        }
    }

    fn resolve_expression(&mut self, expression: &mut Expr) {
        let span = expression.span();
        match expression {
            Expr::QualifiedPath { target, .. } | Expr::Cast { target, .. } => {
                self.resolve_type(target, span);
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
                    self.resolve_expression(&mut field.value);
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
                if let Some(branch) = else_branch {
                    self.resolve_expression(branch);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.resolve_expression(value);
                for arm in arms {
                    self.resolve_expression(&mut arm.expression);
                }
            }
            Expr::Block(block) => self.resolve_block(block),
            Expr::Path { segments, span } => {
                if segments.len() > 1
                    && let Some(canonical) = self.resolve_name(&segments[0], *span)
                {
                    let mut resolved = canonical.split("::").map(str::to_owned).collect::<Vec<_>>();
                    resolved.extend(segments.iter().skip(1).cloned());
                    *segments = resolved;
                }
            }
            Expr::Literal { .. } | Expr::Variable { .. } => {}
        }
    }

    fn current_scope(&mut self) -> &mut Scope {
        self.scopes.last_mut().expect("host type scope exists")
    }
}

fn public_inner(statement: &Stmt) -> &Stmt {
    match statement {
        Stmt::Public { statement, .. } => statement,
        statement => statement,
    }
}

fn path_candidates(prefix: &[String], path: &[String]) -> Vec<String> {
    let Some(first) = path.first().map(String::as_str) else {
        return Vec::new();
    };
    if matches!(first, "crate" | "self" | "super") {
        let mut output = match first {
            "crate" => Vec::new(),
            "self" => prefix.to_vec(),
            "super" => {
                let mut output = prefix.to_vec();
                output.pop();
                output
            }
            _ => unreachable!(),
        };
        for segment in path.iter().skip(1) {
            match segment.as_str() {
                "crate" => output.clear(),
                "self" => {}
                "super" => {
                    output.pop();
                }
                _ => output.push(segment.clone()),
            }
        }
        return vec![output.join("::")];
    }
    let absolute = path.join("::");
    if prefix.is_empty() {
        vec![absolute]
    } else {
        vec![format!("{}::{absolute}", prefix.join("::")), absolute]
    }
}
