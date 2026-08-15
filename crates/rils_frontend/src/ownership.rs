use std::collections::{HashMap, HashSet};

use crate::{
    analysis::AnalysisDiagnostic,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt, UnaryOp},
    source::Span,
    types::Type,
};

pub(crate) fn analyze(
    program: &Program,
    binding_types: &HashMap<Span, Type>,
    expression_types: &HashMap<Span, Type>,
) -> Vec<AnalysisDiagnostic> {
    Checker::new(program, binding_types, expression_types).run(program)
}

#[derive(Clone)]
struct Binding {
    mutable: bool,
    ty: Type,
    moved: bool,
    moved_places: HashSet<String>,
}

#[derive(Clone, Default)]
struct Scope {
    bindings: HashMap<String, Binding>,
    retained_borrows: Vec<Borrow>,
}

#[derive(Clone, Debug)]
struct Borrow {
    root: String,
    interior: bool,
}

#[derive(Default)]
struct ExpressionValue {
    contains_reference: bool,
    borrows: Vec<Borrow>,
}

#[derive(Clone)]
struct NominalDefinition {
    parameters: Vec<String>,
    fields: Vec<Type>,
}

type Snapshot = (Vec<Scope>, HashMap<String, (usize, usize)>);

#[derive(Clone, Copy)]
enum ReceiverMode {
    Owned,
    Borrowed { mutable: bool },
}

struct Checker<'a> {
    binding_types: &'a HashMap<Span, Type>,
    expression_types: &'a HashMap<Span, Type>,
    nominals: HashMap<String, NominalDefinition>,
    receivers: HashMap<(String, String), ReceiverMode>,
    scopes: Vec<Scope>,
    active_borrows: HashMap<String, (usize, usize)>,
    break_states: Vec<Vec<Snapshot>>,
    diagnostics: Vec<AnalysisDiagnostic>,
}

impl<'a> Checker<'a> {
    fn new(
        program: &Program,
        binding_types: &'a HashMap<Span, Type>,
        expression_types: &'a HashMap<Span, Type>,
    ) -> Self {
        let mut checker = Self {
            binding_types,
            expression_types,
            nominals: HashMap::new(),
            receivers: HashMap::new(),
            scopes: vec![Scope::default()],
            active_borrows: HashMap::new(),
            break_states: Vec::new(),
            diagnostics: Vec::new(),
        };
        checker.collect_nominals(&program.statements);
        checker
    }

    fn run(mut self, program: &Program) -> Vec<AnalysisDiagnostic> {
        self.statements(&program.statements);
        self.diagnostics
    }

    fn collect_nominals(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_nominals(statements),
                Stmt::Struct {
                    name,
                    generic_parameters,
                    fields,
                    ..
                } => {
                    self.nominals.insert(
                        name.clone(),
                        NominalDefinition {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            fields: fields
                                .iter()
                                .map(|field| field.type_annotation.clone())
                                .collect(),
                        },
                    );
                }
                Stmt::Enum {
                    name,
                    generic_parameters,
                    variants,
                    ..
                } => {
                    let fields = variants
                        .iter()
                        .flat_map(|variant| match variant {
                            EnumVariant::Unit { .. } => Vec::new(),
                            EnumVariant::Tuple { fields, .. } => fields.clone(),
                            EnumVariant::Record { fields, .. } => fields
                                .iter()
                                .map(|field| field.type_annotation.clone())
                                .collect(),
                        })
                        .collect();
                    self.nominals.insert(
                        name.clone(),
                        NominalDefinition {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            fields,
                        },
                    );
                }
                Stmt::Impl {
                    target,
                    trait_name,
                    methods,
                    ..
                } => {
                    let Type::Named { name, .. } = target else {
                        continue;
                    };
                    if trait_name.as_deref() == Some("Iterator") {
                        for member in rils_builtins::builtin("Iterator")
                            .into_iter()
                            .flat_map(|declaration| declaration.members)
                        {
                            if !rils_builtins::is_iterator_default_method(member.name) {
                                continue;
                            }
                            let Some(receiver) = member.receiver else {
                                continue;
                            };
                            let mode = match receiver {
                                rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
                                rils_builtins::ReceiverMode::Shared => {
                                    ReceiverMode::Borrowed { mutable: false }
                                }
                                rils_builtins::ReceiverMode::Mutable => {
                                    ReceiverMode::Borrowed { mutable: true }
                                }
                            };
                            self.receivers
                                .insert((name.clone(), member.name.into()), mode);
                        }
                    }
                    for method in methods {
                        let Some(receiver) = method
                            .parameters
                            .first()
                            .filter(|parameter| parameter.name == "self")
                        else {
                            continue;
                        };
                        let mode = match &receiver.type_annotation {
                            Some(Type::Reference { mutable, .. }) => {
                                ReceiverMode::Borrowed { mutable: *mutable }
                            }
                            _ => ReceiverMode::Owned,
                        };
                        self.receivers
                            .insert((name.clone(), method.name.clone()), mode);
                    }
                }
                _ => {}
            }
        }
    }

    fn statements(&mut self, statements: &[Stmt]) {
        for statement in statements {
            self.statement(statement);
        }
    }

    fn statement(&mut self, statement: &Stmt) {
        match statement {
            Stmt::Public { statement, .. } => self.statement(statement),
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
                type_annotation,
                initializer,
                span,
            } => {
                let value = self.expression(initializer);
                if self.scopes.len() == 1 && value.contains_reference {
                    self.diagnostic("references cannot be stored in global bindings", *span);
                }
                if type_annotation.as_ref().is_some_and(|ty| {
                    ty.contains_reference() && !matches!(ty, Type::Reference { .. })
                }) {
                    self.diagnostic("references cannot be stored inside owned values", *span);
                }
                if value.contains_reference
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
                self.define(name, *name_span, *mutable);
                self.retain(value.borrows);
            }
            Stmt::Function {
                name,
                name_span,
                parameters,
                body,
                ..
            } => {
                self.define(name, *name_span, false);
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
                    if result.contains_reference {
                        self.diagnostic("references cannot be returned from a function", *span);
                    }
                    self.discard(result);
                }
            }
            Stmt::Break { value, span } => {
                if let Some(value) = value {
                    let result = self.expression(value);
                    if result.contains_reference {
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
        self.push_scope();
        for (name, span, mutable) in parameters {
            self.define(name, span, mutable);
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
                if value.contains_reference {
                    self.diagnostic(
                        "references cannot be returned from a function",
                        expression.span(),
                    );
                }
                self.discard(value);
                continue;
            }
            self.statement(statement);
        }
        self.pop_scope();
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
        let contains_reference = result.contains_reference;
        if contains_reference {
            self.diagnostic("reference cannot escape its local block", block.span);
        }
        self.discard(result);
        self.pop_scope();
        ExpressionValue {
            contains_reference,
            borrows: Vec::new(),
        }
    }

    fn expression(&mut self, expression: &Expr) -> ExpressionValue {
        match expression {
            Expr::Literal { .. } | Expr::Path { .. } | Expr::QualifiedPath { .. } => {
                self.typed_value(expression)
            }
            Expr::Variable { name, span } => self.take_variable(name, *span),
            Expr::Member { object, name, span }
                if matches!(self.expression_types.get(span), Some(Type::Function { .. })) =>
            {
                match self.receiver_mode(self.expression_types.get(&object.span()), name) {
                    Some(ReceiverMode::Owned) => {
                        if self
                            .expression_types
                            .get(&object.span())
                            .is_some_and(|ty| !self.is_copy(ty))
                        {
                            self.diagnostic(
                                "bound method values with an owned receiver require Copy",
                                *span,
                            );
                        }
                        self.expression(object)
                    }
                    Some(ReceiverMode::Borrowed { .. }) => {
                        self.diagnostic(
                            "bound method values cannot capture a local reference",
                            *span,
                        );
                        ExpressionValue::default()
                    }
                    None => self.typed_value(expression),
                }
            }
            Expr::Member { .. } => {
                self.read_place(expression);
                if let Some(ty) = self.expression_types.get(&expression.span())
                    && !self.is_copy(ty)
                {
                    self.move_place(expression);
                }
                self.typed_value(expression)
            }
            Expr::Index { object, index, .. } => {
                self.read_place(object);
                let index = self.expression(index);
                self.discard(index);
                if let Some(ty) = self.expression_types.get(&expression.span())
                    && !self.is_copy(ty)
                {
                    self.diagnostic(
                        "cannot move a non-Copy value out through indexing",
                        expression.span(),
                    );
                }
                self.typed_value(expression)
            }
            Expr::Tuple { elements, span } | Expr::Array { elements, span, .. } => {
                let mut values = elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Vec<_>>();
                if values.iter().any(|value| value.contains_reference) {
                    self.diagnostic("owned collections cannot contain local references", *span);
                }
                if let Expr::Array {
                    repeat: Some(repeat),
                    ..
                } = expression
                {
                    let repeat = self.expression(repeat);
                    self.discard(repeat);
                }
                let borrows = values.drain(..).flat_map(|value| value.borrows).collect();
                ExpressionValue {
                    contains_reference: false,
                    borrows,
                }
            }
            Expr::Try { operand, .. } => {
                let value = self.expression(operand);
                self.discard(value);
                self.typed_value(expression)
            }
            Expr::RecordLiteral { fields, span, .. } => {
                let values = fields
                    .iter()
                    .map(|(_, value)| self.expression(value))
                    .collect::<Vec<_>>();
                if values.iter().any(|value| value.contains_reference) {
                    self.diagnostic(
                        "struct and enum fields cannot contain local references",
                        *span,
                    );
                }
                let borrows = values.into_iter().flat_map(|value| value.borrows).collect();
                ExpressionValue {
                    contains_reference: false,
                    borrows,
                }
            }
            Expr::Assign {
                target,
                value,
                span,
            } => {
                let value = self.expression(value);
                self.assign_place(target, value.contains_reference, *span);
                if value.contains_reference {
                    self.retain(value.borrows);
                } else {
                    self.discard(value);
                }
                ExpressionValue::default()
            }
            Expr::Borrow {
                mutable,
                target,
                span,
            } => self.borrow_place(target, *mutable, *span),
            Expr::Unary {
                operator,
                operand,
                span,
            } => {
                let value = self.expression(operand);
                if *operator == UnaryOp::Dereference
                    && let Some(Type::Reference { inner, .. }) =
                        self.expression_types.get(&operand.span())
                    && !self.is_copy(inner)
                {
                    self.diagnostic("cannot move a non-Copy value out of a reference", *span);
                }
                self.discard(value);
                self.typed_value(expression)
            }
            Expr::Cast { operand, .. } => {
                let value = self.expression(operand);
                self.discard(value);
                self.typed_value(expression)
            }
            Expr::Binary { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
                ..
            } => {
                let left = self.expression(left);
                let right = self.expression(right);
                self.discard(left);
                self.discard(right);
                self.typed_value(expression)
            }
            Expr::Call {
                callee,
                arguments,
                span,
            } => {
                let receiver = if let Expr::Member { object, name, .. } = callee.as_ref() {
                    self.receiver_effect(object, name)
                } else {
                    let callee = self.expression(callee);
                    self.discard(callee);
                    None
                };
                let values = arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Vec<_>>();
                if matches!(
                    callee_name(callee_expression(expression)),
                    Some("Some" | "Ok" | "Err")
                ) && values.iter().any(|value| value.contains_reference)
                {
                    self.diagnostic("references cannot be stored inside owned values", *span);
                }
                for value in values {
                    self.discard(value);
                }
                if let Some(receiver) = receiver {
                    self.discard(receiver);
                }
                self.typed_value(expression)
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                let condition = self.expression(condition);
                self.discard(condition);
                let base = self.snapshot();
                let then_value = self.block(then_branch);
                let then_state = self.snapshot();
                self.restore(base.clone());
                let else_value = else_branch
                    .as_deref()
                    .map(|branch| self.expression(branch))
                    .unwrap_or_default();
                let else_state = self.snapshot();
                self.restore(base);
                self.merge_moved(&[then_state, else_state]);
                ExpressionValue {
                    contains_reference: then_value.contains_reference
                        || else_value.contains_reference,
                    borrows: Vec::new(),
                }
            }
            Expr::Match { value, arms, .. } => {
                let value = self.expression(value);
                self.discard(value);
                let base = self.snapshot();
                let mut contains_reference = false;
                let mut states = Vec::new();
                for arm in arms {
                    self.restore(base.clone());
                    self.push_scope();
                    self.pattern(&arm.pattern);
                    let value = self.expression(&arm.expression);
                    contains_reference |= value.contains_reference;
                    self.discard(value);
                    self.pop_scope();
                    states.push(self.snapshot());
                }
                self.restore(base);
                self.merge_moved(&states);
                ExpressionValue {
                    contains_reference,
                    borrows: Vec::new(),
                }
            }
            Expr::Block(block) => self.block(block),
        }
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

    fn take_variable(&mut self, name: &str, span: Span) -> ExpressionValue {
        let Some(binding) = self.lookup(name).cloned() else {
            return self.typed_value_from_span(span);
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
            contains_reference: binding.ty.contains_reference(),
            borrows: Vec::new(),
        }
    }

    fn receiver_effect(&mut self, object: &Expr, method: &str) -> Option<ExpressionValue> {
        let mode = self.receiver_mode(self.expression_types.get(&object.span()), method);
        match mode {
            Some(ReceiverMode::Owned) => Some(match object {
                Expr::Variable { name, span } => self.take_variable(name, *span),
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
            return self.typed_value_from_span(span);
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
        let borrow = Borrow { root, interior };
        self.add_borrow(&borrow);
        ExpressionValue {
            contains_reference: true,
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

    fn assign_place(&mut self, target: &Expr, contains_reference: bool, span: Span) {
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
                if contains_reference
                    && scope_index.is_some_and(|index| index + 1 < self.scopes.len())
                {
                    self.diagnostic("reference cannot escape its local scope", span);
                }
                if let Some(binding) = self.lookup_mut(name) {
                    binding.moved = false;
                    binding.moved_places.clear();
                }
            }
            Expr::Unary {
                operator: UnaryOp::Dereference,
                operand,
                ..
            } => {
                if let Some(Type::Reference { mutable: false, .. }) =
                    self.expression_types.get(&operand.span())
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

    fn typed_value(&self, expression: &Expr) -> ExpressionValue {
        self.typed_value_from_span(expression.span())
    }

    fn typed_value_from_span(&self, span: Span) -> ExpressionValue {
        ExpressionValue {
            contains_reference: self
                .expression_types
                .get(&span)
                .is_some_and(Type::contains_reference),
            borrows: Vec::new(),
        }
    }

    fn define(&mut self, name: &str, span: Span, mutable: bool) {
        let ty = self
            .binding_types
            .get(&span)
            .cloned()
            .unwrap_or(Type::Unknown);
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
                },
            );
    }

    fn is_copy(&self, ty: &Type) -> bool {
        self.is_copy_inner(ty, &mut HashSet::new())
    }

    fn is_copy_inner(&self, ty: &Type, visiting: &mut HashSet<String>) -> bool {
        match ty {
            Type::Unit
            | Type::Bool
            | Type::Integer(_)
            | Type::Float(_)
            | Type::IntegerVariable(_)
            | Type::FloatVariable(_)
            | Type::Char
            | Type::Reference { .. }
            | Type::Function { .. } => true,
            Type::Option(inner) => self.is_copy_inner(inner, visiting),
            Type::Result(ok, error) => {
                self.is_copy_inner(ok, visiting) && self.is_copy_inner(error, visiting)
            }
            Type::Tuple(elements) => elements.iter().all(|ty| self.is_copy_inner(ty, visiting)),
            Type::Array { element, .. } => self.is_copy_inner(element, visiting),
            Type::Named { name, arguments } => {
                let Some(definition) = self.nominals.get(name) else {
                    return false;
                };
                if !visiting.insert(name.clone()) {
                    return false;
                }
                let substitutions = definition
                    .parameters
                    .iter()
                    .cloned()
                    .zip(arguments.iter().cloned())
                    .collect::<HashMap<_, _>>();
                let copy = definition
                    .fields
                    .iter()
                    .all(|field| self.is_copy_inner(&field.substitute(&substitutions), visiting));
                visiting.remove(name);
                copy
            }
            Type::Unknown | Type::Variable(_) | Type::Associated { .. } => true,
            Type::String => false,
        }
    }

    fn push_scope(&mut self) {
        self.scopes.push(Scope::default());
    }

    fn pop_scope(&mut self) {
        if let Some(scope) = self.scopes.pop() {
            for borrow in scope.retained_borrows {
                self.remove_borrow(&borrow);
            }
        }
    }

    fn retain(&mut self, borrows: Vec<Borrow>) {
        self.scopes
            .last_mut()
            .expect("scope exists")
            .retained_borrows
            .extend(borrows);
    }

    fn discard(&mut self, value: ExpressionValue) {
        for borrow in value.borrows {
            self.remove_borrow(&borrow);
        }
    }

    fn add_borrow(&mut self, borrow: &Borrow) {
        let counts = self.active_borrows.entry(borrow.root.clone()).or_default();
        if borrow.interior {
            counts.1 += 1;
        } else {
            counts.0 += 1;
        }
    }

    fn remove_borrow(&mut self, borrow: &Borrow) {
        let Some(counts) = self.active_borrows.get_mut(&borrow.root) else {
            return;
        };
        if borrow.interior {
            counts.1 = counts.1.saturating_sub(1);
        } else {
            counts.0 = counts.0.saturating_sub(1);
        }
        if *counts == (0, 0) {
            self.active_borrows.remove(&borrow.root);
        }
    }

    fn lookup(&self, name: &str) -> Option<&Binding> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.bindings.get(name))
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Binding> {
        self.scopes
            .iter_mut()
            .rev()
            .find_map(|scope| scope.bindings.get_mut(name))
    }

    fn binding_scope(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .rposition(|scope| scope.bindings.contains_key(name))
    }

    fn diagnostic(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics
            .push(AnalysisDiagnostic::error(message, span));
    }

    fn snapshot(&self) -> Snapshot {
        (self.scopes.clone(), self.active_borrows.clone())
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.scopes = snapshot.0;
        self.active_borrows = snapshot.1;
    }

    fn merge_moved(&mut self, states: &[Snapshot]) {
        if states.is_empty() {
            return;
        }
        for scope_index in 0..self.scopes.len() {
            let names = self.scopes[scope_index]
                .bindings
                .keys()
                .cloned()
                .collect::<Vec<_>>();
            for name in names {
                let moved = states.iter().all(|(scopes, _)| {
                    scopes
                        .get(scope_index)
                        .and_then(|scope| scope.bindings.get(&name))
                        .is_some_and(|binding| binding.moved)
                });
                if moved && let Some(binding) = self.scopes[scope_index].bindings.get_mut(&name) {
                    binding.moved = true;
                }
                let moved_places = states
                    .iter()
                    .filter_map(|(scopes, _)| {
                        scopes
                            .get(scope_index)
                            .and_then(|scope| scope.bindings.get(&name))
                            .map(|binding| binding.moved_places.clone())
                    })
                    .reduce(|left, right| left.intersection(&right).cloned().collect())
                    .unwrap_or_default();
                if let Some(binding) = self.scopes[scope_index].bindings.get_mut(&name) {
                    binding.moved_places.extend(moved_places);
                }
            }
        }
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

fn callee_expression(expression: &Expr) -> &Expr {
    let Expr::Call { callee, .. } = expression else {
        unreachable!("callee_expression requires a call")
    };
    callee
}

fn callee_name(expression: &Expr) -> Option<&str> {
    match expression {
        Expr::Variable { name, .. } => Some(name),
        Expr::Path { segments, .. } => segments.last().map(String::as_str),
        _ => None,
    }
}
