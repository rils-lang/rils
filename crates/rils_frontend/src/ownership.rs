use std::collections::{HashMap, HashSet};

mod declarations;
mod expressions;
mod regions;
mod scopes;
use regions::{RegionId, Regions};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt, UnaryOp},
    semantic::ExpressionTypes,
    source::Span,
    types::Type,
};

pub(crate) fn analyze(
    program: &Program,
    binding_types: &HashMap<Span, Type>,
    expression_types: ExpressionTypes<'_>,
    host_types: &HashSet<String>,
    declarations: (&crate::semantic::DeclarationTypeResolver, &[String]),
) -> Vec<AnalysisDiagnostic> {
    Checker::new(
        program,
        binding_types,
        expression_types,
        host_types,
        declarations,
    )
    .run(program)
}

#[derive(Clone)]
struct Binding {
    mutable: bool,
    ty: Type,
    moved: bool,
    moved_places: HashSet<String>,
    reference_region: Option<RegionId>,
}

#[derive(Clone)]
struct Scope {
    region: RegionId,
    bindings: HashMap<String, Binding>,
    retained_borrows: Vec<Borrow>,
}

#[derive(Clone, Debug)]
struct Borrow {
    root: String,
    interior: bool,
    region: RegionId,
}

#[derive(Default)]
struct ExpressionValue {
    reference_region: Option<RegionId>,
    borrows: Vec<Borrow>,
}

impl ExpressionValue {
    fn contains_reference(&self) -> bool {
        self.reference_region.is_some()
    }
}

type Snapshot = (Vec<Scope>, HashMap<String, (usize, usize)>);

#[derive(Clone, Copy)]
enum ReceiverMode {
    Owned,
    Borrowed { mutable: bool },
}

struct Checker<'a> {
    binding_types: &'a HashMap<Span, Type>,
    expression_types: ExpressionTypes<'a>,
    host_types: &'a HashSet<String>,
    declaration_types: &'a crate::semantic::DeclarationTypeResolver,
    module_path: &'a [String],
    receivers: HashMap<(String, String), ReceiverMode>,
    scopes: Vec<Scope>,
    regions: Regions,
    active_borrows: HashMap<String, (usize, usize)>,
    break_states: Vec<Vec<Snapshot>>,
    diagnostics: Vec<AnalysisDiagnostic>,
    return_destinations: Vec<RegionId>,
}

impl<'a> Checker<'a> {
    fn new(
        program: &Program,
        binding_types: &'a HashMap<Span, Type>,
        expression_types: ExpressionTypes<'a>,
        host_types: &'a HashSet<String>,
        declarations: (&'a crate::semantic::DeclarationTypeResolver, &'a [String]),
    ) -> Self {
        let mut checker = Self {
            binding_types,
            expression_types,
            host_types,
            declaration_types: declarations.0,
            module_path: declarations.1,
            receivers: HashMap::new(),
            scopes: vec![Scope {
                region: Regions::ROOT,
                bindings: HashMap::new(),
                retained_borrows: Vec::new(),
            }],
            regions: Regions::new(),
            active_borrows: HashMap::new(),
            break_states: Vec::new(),
            diagnostics: Vec::new(),
            return_destinations: Vec::new(),
        };
        checker.collect_nominals(&program.statements);
        checker
    }

    fn run(mut self, program: &Program) -> Vec<AnalysisDiagnostic> {
        self.statements(&program.statements);
        self.diagnostics
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Module {
                statements: Some(statements),
                ..
            } => self.statements(statements),
            Stmt::Module {
                name, name_span, ..
            } => self.define(name, *name_span, false),
            Stmt::Use { imports, .. } => {
                for import in imports {
                    if let Some(name) = import.binding_name() {
                        self.define(name, import.alias_span.unwrap_or(import.name_span), false);
                    }
                }
            }
            Stmt::Let {
                name,
                name_span,
                mutable,
                type_annotation: _,
                initializer,
                span,
            } => {
                let value = self.expression(initializer);
                if self.scopes.len() == 1 && value.contains_reference() {
                    self.diagnostic("references cannot be stored in global bindings", *span);
                }
                if value.contains_reference()
                    && self.scopes.last().is_some_and(|scope| {
                        scope
                            .bindings
                            .values()
                            .any(|binding| matches!(binding.ty, Type::Function { .. }))
                    })
                {
                    self.diagnostic(
                        "a reference cannot be introduced after a closure in the same scope",
                        *span,
                    );
                }
                self.define_with_region(name, *name_span, *mutable, value.reference_region);
                self.retain(value.borrows);
            }
            Stmt::Function {
                attributes,
                name,
                name_span,
                parameters,
                body,
                ..
            } => {
                self.define(name, *name_span, false);
                if crate::ast::has_compiler_internal_attribute(attributes) {
                    return;
                }
                if !self.active_borrows.is_empty()
                    || self.scopes.iter().any(|scope| {
                        scope
                            .bindings
                            .values()
                            .any(|binding| matches!(binding.ty, Type::Reference { .. }))
                    })
                {
                    self.diagnostic("functions cannot capture local references", body.span);
                }
                self.function(
                    parameters
                        .iter()
                        .map(|parameter| (&parameter.name, parameter.span, parameter.mutable)),
                    body,
                );
            }
            Stmt::Struct { fields, .. } => {
                for field in fields {
                    if field.type_annotation.contains_reference() {
                        self.diagnostic(
                            "struct fields cannot contain local references",
                            field.span,
                        );
                    }
                }
            }
            Stmt::Enum { variants, .. } => {
                for variant in variants {
                    match variant {
                        EnumVariant::Unit { .. } => {}
                        EnumVariant::Tuple { fields, span, .. } => {
                            if fields.iter().any(Type::contains_reference) {
                                self.diagnostic(
                                    "enum fields cannot contain local references",
                                    *span,
                                );
                            }
                        }
                        EnumVariant::Record { fields, .. } => {
                            for field in fields {
                                if field.type_annotation.contains_reference() {
                                    self.diagnostic(
                                        "enum fields cannot contain local references",
                                        field.span,
                                    );
                                }
                            }
                        }
                    }
                }
            }
            Stmt::Impl { methods, .. } => {
                for method in methods {
                    if crate::ast::has_compiler_internal_attribute(&method.attributes) {
                        continue;
                    }
                    self.function(
                        method
                            .parameters
                            .iter()
                            .map(|parameter| (&parameter.name, parameter.span, parameter.mutable)),
                        &method.body,
                    );
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                let condition = self.expression(condition);
                self.discard(condition);
                let snapshot = self.snapshot();
                self.break_states.push(Vec::new());
                self.block(body);
                self.break_states.pop();
                self.restore(snapshot);
            }
            Stmt::Loop { body, .. } => {
                let snapshot = self.snapshot();
                self.break_states.push(Vec::new());
                self.block(body);
                let breaks = self.break_states.pop().expect("loop state exists");
                self.restore(snapshot);
                self.merge_moved(&breaks);
            }
            Stmt::For {
                binding,
                binding_span,
                iterable,
                body,
                ..
            } => {
                let iterable = self.expression(iterable);
                self.discard(iterable);
                let snapshot = self.snapshot();
                self.break_states.push(Vec::new());
                self.push_scope();
                self.define(binding, *binding_span, false);
                self.statements(&body.statements);
                self.pop_scope();
                self.break_states.pop();
                self.restore(snapshot);
            }
            Stmt::Return { value, span } => {
                if let Some(value) = value {
                    let result = self.expression(value);
                    self.check_return(&result, *span);
                    self.discard(result);
                }
            }
            Stmt::Break { value, span } => {
                if let Some(value) = value {
                    let result = self.expression(value);
                    if result.contains_reference() {
                        self.diagnostic("references cannot escape a loop through `break`", *span);
                    }
                    self.discard(result);
                }
                let state = self.snapshot();
                if let Some(states) = self.break_states.last_mut() {
                    states.push(state);
                }
            }
            Stmt::Expr { expression, .. } => {
                let value = self.expression(expression);
                self.discard(value);
            }
            Stmt::Continue { .. } | Stmt::TypeAlias { .. } | Stmt::Trait { .. } => {}
        }
    }

    fn function<'b>(
        &mut self,
        parameters: impl Iterator<Item = (&'b String, Span, bool)>,
        body: &Block,
    ) {
        let snapshot = self.snapshot();
        let destination = self.scopes.last().expect("scope exists").region;
        self.push_scope();
        self.return_destinations.push(destination);
        for (name, span, mutable) in parameters {
            let parameter_region = self
                .binding_types
                .get(&span)
                .filter(|ty| ty.contains_reference())
                .map(|_| Regions::ROOT);
            self.define_with_region(name, span, mutable, parameter_region);
        }
        let last = body.statements.len().saturating_sub(1);
        for (index, statement) in body.statements.iter().enumerate() {
            if index == last
                && let Stmt::Expr {
                    expression,
                    terminated: false,
                } = statement
            {
                let value = self.expression(expression);
                self.check_return(&value, expression.span());
                self.discard(value);
                continue;
            }
            self.statement(statement);
        }
        self.pop_scope();
        self.return_destinations.pop();
        self.restore(snapshot);
    }

    fn block(&mut self, block: &Block) -> ExpressionValue {
        self.push_scope();
        let last = block.statements.len().saturating_sub(1);
        let mut result = ExpressionValue::default();
        for (index, statement) in block.statements.iter().enumerate() {
            if index == last
                && let Stmt::Expr {
                    expression,
                    terminated: false,
                } = statement
            {
                result = self.expression(expression);
                continue;
            }
            self.statement(statement);
        }
        let parent_region = self.scopes.last().expect("scope exists").region;
        let valid = result
            .reference_region
            .is_none_or(|region| self.regions.outlives(region, parent_region));
        if !valid {
            self.diagnostic("reference cannot escape its local block", block.span);
        }
        if !valid {
            self.discard(result);
            result = ExpressionValue::default();
        }
        self.pop_scope();
        result
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Binding { name, span } => self.define(name, *span, false),
            Pattern::Some { inner, .. }
            | Pattern::Ok { inner, .. }
            | Pattern::Err { inner, .. } => self.pattern(inner),
            Pattern::TupleVariant { fields, .. } => {
                for field in fields {
                    self.pattern(field);
                }
            }
            Pattern::Record { fields, .. } => {
                for (_, field) in fields {
                    self.pattern(field);
                }
            }
            _ => {}
        }
    }

    fn take_variable(&mut self, name: &str, expression: &Expr) -> ExpressionValue {
        let span = expression.span();
        let Some(binding) = self.lookup(name).cloned() else {
            return self.typed_value(expression);
        };
        if binding.moved {
            self.diagnostic(format!("use of moved value `{name}`"), span);
        } else if !binding.moved_places.is_empty() {
            self.diagnostic(format!("use of partially moved value `{name}`"), span);
        } else if !self.is_copy(&binding.ty) {
            if self.active_borrows.contains_key(name) {
                self.diagnostic(format!("cannot move `{name}` while it is referenced"), span);
            } else if let Some(binding) = self.lookup_mut(name) {
                binding.moved = true;
            }
        }
        ExpressionValue {
            reference_region: if binding.ty.contains_reference() {
                binding.reference_region
            } else {
                None
            },
            borrows: Vec::new(),
        }
    }

    fn receiver_effect(&mut self, object: &Expr, method: &str) -> Option<ExpressionValue> {
        let mode = self.receiver_mode(self.expression_types.get(object), method);
        match mode {
            Some(ReceiverMode::Owned) => Some(match object {
                Expr::Variable { name, .. } => self.take_variable(name, object),
                _ => self.expression(object),
            }),
            Some(ReceiverMode::Borrowed { mutable }) => Some(if place_root(object).is_some() {
                self.borrow_place(object, mutable, object.span())
            } else {
                self.expression(object)
            }),
            None => {
                let callee = Expr::Member {
                    object: Box::new(object.clone()),
                    name: method.into(),
                    span: object.span(),
                };
                let value = self.expression(&callee);
                self.discard(value);
                None
            }
        }
    }

    fn receiver_mode(&self, ty: Option<&Type>, method: &str) -> Option<ReceiverMode> {
        let ty = match ty? {
            Type::Reference { inner, .. } => inner.as_ref(),
            ty => ty,
        };
        if method == "clone" {
            return Some(ReceiverMode::Borrowed { mutable: false });
        }
        if let Some(mode) = crate::standard_library::builtin_receiver_mode(ty, method) {
            return Some(match mode {
                rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
                rils_builtins::ReceiverMode::Shared => ReceiverMode::Borrowed { mutable: false },
                rils_builtins::ReceiverMode::Mutable => ReceiverMode::Borrowed { mutable: true },
            });
        }
        match ty {
            Type::Named { name, .. } => self.receivers.get(&(name.clone(), method.into())).copied(),
            _ => None,
        }
    }

    fn borrow_place(&mut self, target: &Expr, mutable: bool, span: Span) -> ExpressionValue {
        let Some((root, interior)) = place_root(target) else {
            return self.typed_value(target);
        };
        if let Some(binding) = self.lookup(&root).cloned() {
            if binding.moved {
                self.diagnostic(format!("cannot reference moved value `{root}`"), span);
            }
            if let Some((_, place)) = place_key(target)
                && binding
                    .moved_places
                    .iter()
                    .any(|moved| places_overlap(moved, &place))
            {
                self.diagnostic(
                    format!("cannot reference moved place `{root}{place}`"),
                    span,
                );
            }
            if mutable
                && !binding.mutable
                && !matches!(binding.ty, Type::Reference { mutable: true, .. })
            {
                self.diagnostic(
                    format!("cannot mutably reference immutable variable `{root}`"),
                    span,
                );
            }
        }
        let region = self
            .lookup(&root)
            .and_then(|binding| binding.reference_region)
            .or_else(|| {
                self.binding_scope(&root)
                    .map(|index| self.scopes[index].region)
            })
            .unwrap_or(Regions::ROOT);
        let borrow = Borrow {
            root,
            interior,
            region,
        };
        self.add_borrow(&borrow);
        ExpressionValue {
            reference_region: Some(borrow.region),
            borrows: vec![borrow],
        }
    }

    fn read_place(&mut self, expression: &Expr) {
        if let Some((root, place)) = place_key(expression)
            && let Some(binding) = self.lookup(&root)
        {
            if binding.moved {
                self.diagnostic(format!("use of moved value `{root}`"), expression.span());
            } else if !place.is_empty()
                && binding
                    .moved_places
                    .iter()
                    .any(|moved| places_overlap(moved, &place))
            {
                self.diagnostic(
                    format!("use of moved place `{root}{place}`"),
                    expression.span(),
                );
            }
        }
    }

    fn move_place(&mut self, expression: &Expr) {
        let Some((root, place)) = place_key(expression) else {
            return;
        };
        if place.is_empty() {
            return;
        }
        if self.active_borrows.contains_key(&root) {
            self.diagnostic(
                format!("cannot move `{root}{place}` while it is referenced"),
                expression.span(),
            );
            return;
        }
        if let Some(binding) = self.lookup_mut(&root) {
            binding.moved_places.insert(place);
        }
    }

    fn assign_place(&mut self, target: &Expr, reference_region: Option<RegionId>, span: Span) {
        match target {
            Expr::Variable { name, .. } => {
                let scope_index = self.binding_scope(name);
                if let Some(binding) = self.lookup(name) {
                    if !binding.mutable {
                        self.diagnostic(
                            format!("cannot assign to immutable variable `{name}`"),
                            span,
                        );
                    }
                    if self
                        .active_borrows
                        .get(name)
                        .is_some_and(|(_, interior)| *interior > 0)
                    {
                        self.diagnostic(
                            format!(
                                "cannot replace `{name}` while one of its fields is referenced"
                            ),
                            span,
                        );
                    }
                }
                if reference_region.is_some_and(|source| {
                    scope_index.is_some_and(|index| {
                        !self.regions.outlives(source, self.scopes[index].region)
                    })
                }) {
                    self.diagnostic("reference cannot escape its local scope", span);
                }
                if let Some(binding) = self.lookup_mut(name) {
                    binding.moved = false;
                    binding.moved_places.clear();
                    binding.reference_region = reference_region;
                }
            }
            Expr::Unary {
                operator: UnaryOp::Dereference,
                operand,
                ..
            } => {
                if let Some(Type::Reference { mutable: false, .. }) =
                    self.expression_types.get(operand)
                {
                    self.diagnostic("cannot assign through immutable reference", span);
                }
                let operand = self.expression(operand);
                self.discard(operand);
            }
            _ => {
                if let Some((root, _)) = place_root(target)
                    && self.lookup(&root).is_some_and(|binding| {
                        !binding.mutable
                            && !matches!(binding.ty, Type::Reference { mutable: true, .. })
                    })
                {
                    self.diagnostic(
                        format!("cannot assign through immutable place `{root}`"),
                        span,
                    );
                }
                self.read_assignment_place(target);
                if let Some((root, place)) = place_key(target)
                    && !place.is_empty()
                    && let Some(binding) = self.lookup_mut(&root)
                {
                    binding
                        .moved_places
                        .retain(|moved| !places_overlap(moved, &place));
                }
            }
        }
    }

    fn read_assignment_place(&mut self, expression: &Expr) {
        let Some((root, place)) = place_key(expression) else {
            return;
        };
        let Some(binding) = self.lookup(&root) else {
            return;
        };
        if binding.moved {
            self.diagnostic(format!("use of moved value `{root}`"), expression.span());
            return;
        }
        if binding.moved_places.iter().any(|moved| {
            place
                .strip_prefix(moved)
                .is_some_and(|suffix| suffix.starts_with('.'))
        }) {
            self.diagnostic(
                format!("use of partially moved place `{root}{place}`"),
                expression.span(),
            );
        }
    }

    fn typed_value(&self, _expression: &Expr) -> ExpressionValue {
        ExpressionValue {
            reference_region: None,
            borrows: Vec::new(),
        }
    }

    fn shorter_region(&self, left: RegionId, right: RegionId) -> RegionId {
        if self.regions.outlives(left, right) {
            right
        } else {
            left
        }
    }

    fn check_return(&mut self, value: &ExpressionValue, span: Span) {
        let Some(source) = value.reference_region else {
            return;
        };
        let destination = self
            .return_destinations
            .last()
            .copied()
            .unwrap_or(Regions::ROOT);
        if !self.regions.outlives(source, destination) {
            self.diagnostic("references cannot be returned from a function", span);
        }
    }

    fn define(&mut self, name: &str, span: Span, mutable: bool) {
        self.define_with_region(name, span, mutable, None);
    }

    fn define_with_region(
        &mut self,
        name: &str,
        span: Span,
        mutable: bool,
        reference_region: Option<RegionId>,
    ) {
        let ty = self
            .binding_types
            .get(&span)
            .cloned()
            .unwrap_or(Type::Unknown);
        let has_reference = ty.contains_reference();
        self.scopes
            .last_mut()
            .expect("scope exists")
            .bindings
            .insert(
                name.into(),
                Binding {
                    mutable,
                    ty,
                    moved: false,
                    moved_places: HashSet::new(),
                    reference_region: if has_reference {
                        reference_region
                    } else {
                        None
                    },
                },
            );
    }

    fn is_copy(&self, ty: &Type) -> bool {
        if matches!(
            ty,
            Type::Unknown
                | Type::Variable(_)
                | Type::BoundVariable { .. }
                | Type::Associated { .. }
        ) {
            return true;
        }
        let ty = self.declaration_types.resolve(ty, self.module_path);
        self.declaration_types
            .copy_types()
            .is_copy(&ty, self.host_types)
    }
}

fn place_root(expression: &Expr) -> Option<(String, bool)> {
    match expression {
        Expr::Variable { name, .. } => Some((name.clone(), false)),
        Expr::Member { object, .. } | Expr::Index { object, .. } => {
            place_root(object).map(|(root, _)| (root, true))
        }
        Expr::Unary {
            operator: UnaryOp::Dereference,
            operand,
            ..
        } => place_root(operand),
        _ => None,
    }
}

fn place_key(expression: &Expr) -> Option<(String, String)> {
    match expression {
        Expr::Variable { name, .. } => Some((name.clone(), String::new())),
        Expr::Member { object, name, .. } => {
            place_key(object).map(|(root, path)| (root, format!("{path}.{name}")))
        }
        Expr::Unary {
            operator: UnaryOp::Dereference,
            operand,
            ..
        } => place_key(operand),
        _ => None,
    }
}

fn places_overlap(left: &str, right: &str) -> bool {
    left == right
        || left
            .strip_prefix(right)
            .is_some_and(|suffix| suffix.starts_with('.'))
        || right
            .strip_prefix(left)
            .is_some_and(|suffix| suffix.starts_with('.'))
}

fn callee_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Variable { name, .. } => Some(name),
        Expr::Path { segments, .. } | Expr::GenericPath { segments, .. } => {
            segments.last().map(String::as_str)
        }
        _ => None,
    }
}
