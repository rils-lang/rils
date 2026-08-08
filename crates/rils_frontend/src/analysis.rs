use std::collections::{HashMap, HashSet};

use crate::{
    FrontendError,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt},
    source::Span,
    type_inference,
    types::Type,
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
pub struct SymbolOccurrence {
    pub name: String,
    pub span: Span,
    pub definition_span: Option<Span>,
    pub kind: SymbolKind,
    pub is_definition: bool,
    pub inferred_type: Option<Type>,
    pub detail: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisDiagnostic {
    pub message: String,
    pub span: Span,
    pub severity: DiagnosticSeverity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
}

impl AnalysisDiagnostic {
    pub(crate) fn error(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            severity: DiagnosticSeverity::Error,
        }
    }

    pub(crate) fn warning(message: impl Into<String>, span: Span) -> Self {
        Self {
            message: message.into(),
            span,
            severity: DiagnosticSeverity::Warning,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DocumentAnalysis {
    pub diagnostics: Vec<AnalysisDiagnostic>,
    pub symbols: Vec<SymbolOccurrence>,
    pub inlay_hints: Vec<InlayTypeHint>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InlayTypeHint {
    pub position: usize,
    pub span: Span,
    pub label: String,
}

#[derive(Clone)]
struct Definition {
    span: Option<Span>,
    kind: SymbolKind,
}

#[derive(Clone)]
struct TypeAliasDefinition {
    parameters: Vec<String>,
    target: Type,
}

#[doc(hidden)]
pub fn analyze_program(program: &Program) -> DocumentAnalysis {
    Analyzer::new().analyze(program)
}

pub fn analyze(source: &str) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex(source).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    Ok(analyze_program(&program))
}

struct Analyzer {
    scopes: Vec<HashMap<String, Definition>>,
    trait_members: HashMap<(String, String), Span>,
    type_aliases: HashMap<String, TypeAliasDefinition>,
    result: DocumentAnalysis,
}

impl Analyzer {
    fn new() -> Self {
        let mut globals = HashMap::new();
        for name in [
            "#rils_native_print",
            "#rils_native_println",
            "type_of",
            "clone",
            "#rils_native_assert",
            "Some",
            "None",
            "Ok",
            "Err",
            "is_ok",
            "is_err",
            "is_some",
            "is_none",
            "unwrap",
            "unwrap_or",
        ] {
            globals.insert(
                name.into(),
                Definition {
                    span: None,
                    kind: SymbolKind::Function,
                },
            );
        }
        for name in ["Copy", "Clone", "Iterator", "IntoIterator"] {
            globals.insert(
                name.into(),
                Definition {
                    span: None,
                    kind: SymbolKind::Trait,
                },
            );
        }
        globals.insert(
            "Range".into(),
            Definition {
                span: None,
                kind: SymbolKind::Type,
            },
        );
        globals.insert(
            "Vec".into(),
            Definition {
                span: None,
                kind: SymbolKind::Type,
            },
        );
        for name in ["core", "std", "prelude"] {
            globals.insert(
                name.into(),
                Definition {
                    span: None,
                    kind: SymbolKind::Module,
                },
            );
        }
        Self {
            scopes: vec![globals],
            trait_members: HashMap::new(),
            type_aliases: HashMap::new(),
            result: DocumentAnalysis::default(),
        }
    }

    fn analyze(mut self, program: &Program) -> DocumentAnalysis {
        self.collect_trait_members(&program.statements);
        self.collect_type_aliases(&program.statements);
        self.macros(program);
        self.statements(&program.statements);
        self.type_references(program);
        let inference = type_inference::infer(program);
        self.result.diagnostics.extend(crate::control_flow::analyze(
            program,
            &inference.expression_types,
        ));
        self.result.diagnostics.extend(crate::ownership::analyze(
            program,
            &inference.binding_types,
            &inference.expression_types,
        ));
        self.result
            .diagnostics
            .extend(crate::static_type_check::analyze(
                program,
                &inference.expression_types,
            ));
        self.result
            .diagnostics
            .sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
        self.result
            .diagnostics
            .dedup_by(|left, right| left.span == right.span && left.message == right.message);
        for symbol in &mut self.result.symbols {
            if let Some(definition_span) = symbol.definition_span {
                symbol.inferred_type = inference.binding_types.get(&definition_span).cloned();
            }
        }
        self.result.inlay_hints = inference
            .hints
            .into_iter()
            .map(|hint| InlayTypeHint {
                position: hint.position,
                span: hint.span,
                label: format!("{}{ty}", hint.prefix, ty = hint.ty),
            })
            .collect();
        self.result
    }

    fn collect_type_aliases(&mut self, statements: &[Stmt]) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => self.collect_type_aliases(statements),
                Stmt::TypeAlias {
                    name,
                    generic_parameters,
                    target,
                    ..
                } => {
                    self.type_aliases.insert(
                        name.clone(),
                        TypeAliasDefinition {
                            parameters: generic_parameters
                                .iter()
                                .map(|parameter| parameter.name.clone())
                                .collect(),
                            target: target.clone(),
                        },
                    );
                }
                _ => {}
            }
        }
    }

    fn collect_trait_members(&mut self, statements: &[Stmt]) {
        for statement in statements {
            if let Stmt::Public { statement, .. } = statement {
                self.collect_trait_members(std::slice::from_ref(statement));
                continue;
            }
            if let Stmt::Module {
                statements: Some(statements),
                ..
            } = statement
            {
                self.collect_trait_members(statements);
                continue;
            }
            if let Stmt::Trait {
                name,
                associated_types,
                methods,
                ..
            } = statement
            {
                for associated in associated_types {
                    self.trait_members.insert(
                        (name.clone(), associated.name.clone()),
                        associated.name_span,
                    );
                }
                for method in methods {
                    self.trait_members
                        .insert((name.clone(), method.name.clone()), method.name_span);
                }
            }
        }
    }

    fn macros(&mut self, program: &Program) {
        for definition in &program.macros {
            self.definition_only(&definition.name, definition.name_span, SymbolKind::Macro);
            for span in &definition.references {
                self.result.symbols.push(SymbolOccurrence {
                    name: definition.name.clone(),
                    span: *span,
                    definition_span: Some(definition.name_span),
                    kind: SymbolKind::Macro,
                    is_definition: false,
                    inferred_type: None,
                    detail: None,
                });
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
                name,
                name_span,
                statements,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Module);
                if let Some(statements) = statements {
                    self.with_scope(|analyzer| analyzer.statements(statements));
                }
            }
            Stmt::Use {
                path,
                alias,
                alias_span,
                span,
            } => {
                if let Some(first) = path.first() {
                    self.reference(
                        first,
                        Span::new(
                            span.start + "use ".len(),
                            span.start + "use ".len() + first.len(),
                        ),
                        SymbolKind::Module,
                    );
                }
                let mut offset = span.start + "use ".len();
                for (index, segment) in path.iter().enumerate().skip(1) {
                    offset += path[index - 1].len() + 2;
                    self.result.symbols.push(SymbolOccurrence {
                        name: segment.clone(),
                        span: Span::new(offset, offset + segment.len()),
                        definition_span: None,
                        kind: if index + 1 == path.len() {
                            SymbolKind::Function
                        } else {
                            SymbolKind::Module
                        },
                        is_definition: false,
                        inferred_type: None,
                        detail: None,
                    });
                }
                let name = alias.as_ref().or_else(|| path.last()).expect("use path");
                let name_span = alias_span
                    .unwrap_or_else(|| Span::new(span.end - 1 - name.len(), span.end - 1));
                let kind = if name.chars().next().is_some_and(char::is_uppercase) {
                    SymbolKind::Type
                } else {
                    SymbolKind::Function
                };
                self.define(name, name_span, kind);
            }
            Stmt::Let {
                name,
                name_span,
                initializer,
                ..
            } => {
                self.expression(initializer);
                self.define(name, *name_span, SymbolKind::Variable);
            }
            Stmt::Function {
                name,
                name_span,
                generic_parameters,
                parameters,
                body,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Function);
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                self.with_scope(|analyzer| {
                    for parameter in parameters {
                        analyzer.define(&parameter.name, parameter.span, SymbolKind::Parameter);
                    }
                    analyzer.block_contents(body);
                });
            }
            Stmt::Struct {
                name,
                name_span,
                generic_parameters,
                fields,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for field in fields {
                    self.definition_only(&field.name, field.span, SymbolKind::Field);
                }
            }
            Stmt::Enum {
                name,
                name_span,
                generic_parameters,
                variants,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for variant in variants {
                    let (variant_name, span) = match variant {
                        EnumVariant::Unit { name, span }
                        | EnumVariant::Tuple { name, span, .. }
                        | EnumVariant::Record { name, span, .. } => {
                            (name, Span::new(span.start, span.start + name.len()))
                        }
                    };
                    self.definition_only(variant_name, span, SymbolKind::Variant);
                    if let EnumVariant::Record { fields, .. } = variant {
                        for field in fields {
                            self.definition_only(&field.name, field.span, SymbolKind::Field);
                        }
                    }
                }
            }
            Stmt::Trait {
                name,
                name_span,
                associated_types,
                methods,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Trait);
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    for parameter in &method.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
            }
            Stmt::TypeAlias {
                name,
                name_span,
                generic_parameters,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Type);
                let arguments = generic_parameters
                    .iter()
                    .map(|parameter| Type::Variable(parameter.name.clone()))
                    .collect::<Vec<_>>();
                let detail = self.type_alias_detail(name, &arguments);
                self.result
                    .symbols
                    .last_mut()
                    .expect("type alias definition symbol")
                    .detail = detail;
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
            }
            Stmt::Impl {
                generic_parameters,
                associated_types,
                methods,
                ..
            } => {
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    for parameter in &method.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                    self.with_scope(|analyzer| {
                        for parameter in &method.parameters {
                            analyzer.define(&parameter.name, parameter.span, SymbolKind::Parameter);
                        }
                        analyzer.block_contents(&method.body);
                    });
                }
            }
            Stmt::While {
                condition, body, ..
            } => {
                self.expression(condition);
                self.block(body);
            }
            Stmt::Loop { body, .. } => self.block(body),
            Stmt::For {
                binding,
                binding_span,
                iterable,
                body,
                ..
            } => {
                self.expression(iterable);
                self.with_scope(|analyzer| {
                    analyzer.define(binding, *binding_span, SymbolKind::Variable);
                    analyzer.block_contents(body);
                });
            }
            Stmt::Return { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Break { value, .. } => {
                if let Some(value) = value {
                    self.expression(value);
                }
            }
            Stmt::Continue { .. } => {}
            Stmt::Expr { expression, .. } => self.expression(expression),
        }
    }

    fn expression(&mut self, expression: &Expr) {
        match expression {
            Expr::Literal { .. } => {}
            Expr::Variable { name, span } => {
                if !name.starts_with("#rils_native_") {
                    self.reference(name, *span, SymbolKind::Variable);
                }
            }
            Expr::Path { segments, span } => {
                if let Some(name) = segments.first() {
                    self.reference(
                        name,
                        Span::new(span.start, span.start + name.len()),
                        SymbolKind::Type,
                    );
                }
                if let [trait_name, member] = segments.as_slice() {
                    let definition_span = self
                        .trait_members
                        .get(&(trait_name.clone(), member.clone()))
                        .copied();
                    if definition_span.is_some()
                        || self
                            .lookup(trait_name)
                            .is_some_and(|definition| definition.kind == SymbolKind::Trait)
                    {
                        self.result.symbols.push(SymbolOccurrence {
                            name: member.clone(),
                            span: member_name_span(*span, member),
                            definition_span,
                            kind: SymbolKind::Method,
                            is_definition: false,
                            inferred_type: None,
                            detail: None,
                        });
                    }
                }
            }
            Expr::QualifiedPath {
                trait_name,
                member,
                span,
                ..
            } => self.result.symbols.push(SymbolOccurrence {
                name: member.clone(),
                span: member_name_span(*span, member),
                definition_span: self
                    .trait_members
                    .get(&(trait_name.clone(), member.clone()))
                    .copied(),
                kind: SymbolKind::Method,
                is_definition: false,
                inferred_type: None,
                detail: None,
            }),
            Expr::Member { object, name, span } => {
                self.expression(object);
                self.result.symbols.push(SymbolOccurrence {
                    name: name.clone(),
                    span: member_name_span(*span, name),
                    definition_span: None,
                    kind: SymbolKind::Field,
                    is_definition: false,
                    inferred_type: None,
                    detail: None,
                });
            }
            Expr::Index { object, index, .. } => {
                self.expression(object);
                self.expression(index);
            }
            Expr::Tuple { elements, .. } => {
                for element in elements {
                    self.expression(element);
                }
            }
            Expr::Array {
                elements, repeat, ..
            } => {
                for element in elements {
                    self.expression(element);
                }
                if let Some(repeat) = repeat {
                    self.expression(repeat);
                }
            }
            Expr::Try { operand, .. } => self.expression(operand),
            Expr::RecordLiteral { path, fields, span } => {
                if let Some(name) = path.first() {
                    self.reference(
                        name,
                        Span::new(span.start, span.start + name.len()),
                        SymbolKind::Type,
                    );
                }
                for (_, expression) in fields {
                    self.expression(expression);
                }
            }
            Expr::Assign { target, value, .. } => {
                self.expression(target);
                self.expression(value);
            }
            Expr::Borrow { target, .. } => self.expression(target),
            Expr::Unary { operand, .. } => self.expression(operand),
            Expr::Binary { left, right, .. }
            | Expr::Logical { left, right, .. }
            | Expr::Range {
                start: left,
                end: right,
                ..
            } => {
                self.expression(left);
                self.expression(right);
            }
            Expr::Call {
                callee, arguments, ..
            } => {
                if let Expr::Member { object, name, span } = callee.as_ref() {
                    self.expression(object);
                    self.result.symbols.push(SymbolOccurrence {
                        name: name.clone(),
                        span: member_name_span(*span, name),
                        definition_span: None,
                        kind: SymbolKind::Method,
                        is_definition: false,
                        inferred_type: None,
                        detail: None,
                    });
                } else {
                    self.expression(callee);
                }
                for argument in arguments {
                    self.expression(argument);
                }
            }
            Expr::If {
                condition,
                then_branch,
                else_branch,
                ..
            } => {
                self.expression(condition);
                self.block(then_branch);
                if let Some(else_branch) = else_branch {
                    self.expression(else_branch);
                }
            }
            Expr::Match { value, arms, .. } => {
                self.expression(value);
                for arm in arms {
                    self.with_scope(|analyzer| {
                        analyzer.pattern(&arm.pattern);
                        analyzer.expression(&arm.expression);
                    });
                }
            }
            Expr::Block(block) => self.block(block),
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        match pattern {
            Pattern::Wildcard { .. }
            | Pattern::Literal { .. }
            | Pattern::None { .. }
            | Pattern::Path { .. } => {}
            Pattern::Binding { name, span } => {
                self.define(name, *span, SymbolKind::Variable);
            }
            Pattern::Some { inner, .. } => self.pattern(inner),
            Pattern::Ok { inner, .. } | Pattern::Err { inner, .. } => self.pattern(inner),
            Pattern::TupleVariant { fields, .. } => {
                for field in fields {
                    self.pattern(field);
                }
            }
            Pattern::Record { fields, .. } => {
                for (_, pattern) in fields {
                    self.pattern(pattern);
                }
            }
        }
    }

    fn block(&mut self, block: &Block) {
        self.with_scope(|analyzer| analyzer.block_contents(block));
    }

    fn block_contents(&mut self, block: &Block) {
        self.statements(&block.statements);
    }

    fn define(&mut self, name: &str, span: Span, kind: SymbolKind) {
        if self
            .scopes
            .last()
            .is_some_and(|scope| scope.contains_key(name))
        {
            self.result.diagnostics.push(AnalysisDiagnostic::error(
                format!("`{name}` is already defined in this scope"),
                span,
            ));
        }
        self.scopes.last_mut().expect("scope exists").insert(
            name.into(),
            Definition {
                span: Some(span),
                kind,
            },
        );
        self.definition_only(name, span, kind);
    }

    fn definition_only(&mut self, name: &str, span: Span, kind: SymbolKind) {
        self.result.symbols.push(SymbolOccurrence {
            name: name.into(),
            span,
            definition_span: Some(span),
            kind,
            is_definition: true,
            inferred_type: None,
            detail: None,
        });
    }

    fn reference(&mut self, name: &str, span: Span, fallback_kind: SymbolKind) {
        if let Some(definition) = self.lookup(name).cloned() {
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: definition.span,
                kind: definition.kind,
                is_definition: false,
                inferred_type: None,
                detail: None,
            });
        } else {
            self.result.diagnostics.push(AnalysisDiagnostic::error(
                format!("undefined name `{name}`"),
                span,
            ));
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: None,
                kind: fallback_kind,
                is_definition: false,
                inferred_type: None,
                detail: None,
            });
        }
    }

    fn type_references(&mut self, program: &Program) {
        for reference in &program.type_references {
            let resolved = reference.definition_span.or_else(|| {
                self.result
                    .symbols
                    .iter()
                    .find(|symbol| {
                        symbol.is_definition
                            && symbol.name == reference.name
                            && matches!(symbol.kind, SymbolKind::Type | SymbolKind::Trait)
                    })
                    .map(|symbol| symbol.span)
            });
            if resolved.is_none() && !reference.is_builtin {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("undefined type or trait `{}`", reference.name),
                    reference.span,
                ));
            }
            self.result.symbols.push(SymbolOccurrence {
                name: reference.name.clone(),
                span: reference.span,
                definition_span: resolved,
                kind: SymbolKind::Type,
                is_definition: false,
                inferred_type: None,
                detail: self.type_alias_detail(&reference.name, &reference.arguments),
            });
        }
    }

    fn type_alias_detail(&self, name: &str, arguments: &[Type]) -> Option<String> {
        let alias = self.type_aliases.get(name)?;
        if alias.parameters.len() != arguments.len() {
            return None;
        }
        let expanded = self.expand_type_alias(name, arguments, &mut HashSet::new())?;
        let arguments = if arguments.is_empty() {
            String::new()
        } else {
            format!(
                "<{}>",
                arguments
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        Some(format!("type {name}{arguments} = {expanded}"))
    }

    fn expand_type_alias(
        &self,
        name: &str,
        arguments: &[Type],
        visiting: &mut HashSet<String>,
    ) -> Option<Type> {
        let alias = self.type_aliases.get(name)?;
        if alias.parameters.len() != arguments.len() || !visiting.insert(name.into()) {
            return None;
        }
        let substitutions = alias
            .parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect::<HashMap<_, _>>();
        let expanded = self.expand_type(&alias.target.substitute(&substitutions), visiting);
        visiting.remove(name);
        Some(expanded)
    }

    fn expand_type(&self, ty: &Type, visiting: &mut HashSet<String>) -> Type {
        match ty {
            Type::Named { name, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| self.expand_type(argument, visiting))
                    .collect::<Vec<_>>();
                self.expand_type_alias(name, &arguments, visiting)
                    .unwrap_or_else(|| Type::Named {
                        name: name.clone(),
                        arguments,
                    })
            }
            Type::Option(inner) => Type::Option(Box::new(self.expand_type(inner, visiting))),
            Type::Result(ok, error) => Type::Result(
                Box::new(self.expand_type(ok, visiting)),
                Box::new(self.expand_type(error, visiting)),
            ),
            Type::Tuple(elements) => Type::Tuple(
                elements
                    .iter()
                    .map(|element| self.expand_type(element, visiting))
                    .collect(),
            ),
            Type::Array { element, length } => Type::Array {
                element: Box::new(self.expand_type(element, visiting)),
                length: *length,
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: Box::new(self.expand_type(inner, visiting)),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|parameters| {
                    parameters
                        .iter()
                        .map(|parameter| self.expand_type(parameter, visiting))
                        .collect()
                }),
                return_type: Box::new(self.expand_type(return_type, visiting)),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: Box::new(self.expand_type(base, visiting)),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: arguments
                    .iter()
                    .map(|argument| self.expand_type(argument, visiting))
                    .collect(),
            },
            other => other.clone(),
        }
    }

    fn lookup(&self, name: &str) -> Option<&Definition> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name))
    }

    fn with_scope(&mut self, action: impl FnOnce(&mut Self)) {
        self.scopes.push(HashMap::new());
        action(self);
        self.scopes.pop();
    }
}

fn member_name_span(span: Span, name: &str) -> Span {
    Span::new(span.end.saturating_sub(name.len()), span.end)
}

#[cfg(test)]
#[path = "analysis/tests.rs"]
mod tests;
