use std::collections::{HashMap, HashSet};

use crate::{
    FrontendError,
    ast::{Block, EnumVariant, Expr, Pattern, Program, Stmt},
    source::{SourceId, Span, SymbolId},
    type_inference,
    types::{FunctionSignature, Type},
};

#[path = "analysis/imports.rs"]
mod imports;

use imports::{ModuleExport, collect_module_exports};

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

/// A public declaration exported by a module outside the document currently
/// being analyzed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalModuleExport {
    pub name: String,
    pub span: Span,
    pub kind: SymbolKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub name: String,
    pub span: Span,
    pub definition_span: Option<Span>,
    pub symbol_id: Option<SymbolId>,
    pub definition_id: Option<SymbolId>,
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
    pub expression_types: HashMap<Span, Type>,
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
    id: Option<SymbolId>,
    kind: SymbolKind,
}

#[derive(Clone)]
struct TypeAliasDefinition {
    parameters: Vec<String>,
    target: Type,
}

#[doc(hidden)]
pub fn analyze_program(program: &Program) -> DocumentAnalysis {
    analyze_program_with_host_functions(program, &HashMap::new())
}

#[doc(hidden)]
pub fn analyze_program_with_host_functions(
    program: &Program,
    host_functions: &HashMap<String, FunctionSignature>,
) -> DocumentAnalysis {
    Analyzer::new(SourceId::UNKNOWN, host_functions, program, &HashMap::new()).analyze(program)
}

pub fn analyze(source: &str) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_host_functions(source, &HashMap::new())
}

#[doc(hidden)]
pub fn analyze_with_host_functions(
    source: &str,
    host_functions: &HashMap<String, FunctionSignature>,
) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_source_id(source, SourceId::UNKNOWN, host_functions)
}

pub fn analyze_with_source_id(
    source: &str,
    source_id: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_source_id_and_external_exports(source, source_id, host_functions, &HashMap::new())
}

/// Analyze a document with public declarations supplied by other project files.
pub fn analyze_with_source_id_and_external_exports(
    source: &str,
    source_id: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex_with_source_id(source, source_id).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    Ok(Analyzer::new(source_id, host_functions, &program, external_exports).analyze(&program))
}

struct Analyzer {
    source_id: SourceId,
    next_symbol: HashMap<SourceId, u32>,
    scopes: Vec<HashMap<String, Definition>>,
    glob_imports: Vec<bool>,
    module_path: Vec<String>,
    module_exports: HashMap<String, Vec<ModuleExport>>,
    trait_members: HashMap<(String, String), Span>,
    type_aliases: HashMap<String, TypeAliasDefinition>,
    host_functions: HashMap<String, FunctionSignature>,
    result: DocumentAnalysis,
}

impl Analyzer {
    fn new(
        source_id: SourceId,
        host_functions: &HashMap<String, FunctionSignature>,
        program: &Program,
        external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
    ) -> Self {
        let mut module_exports = collect_module_exports(&program.statements);
        for (module, exports) in external_exports {
            module_exports.entry(module.clone()).or_insert_with(|| {
                exports
                    .iter()
                    .map(|export| ModuleExport {
                        name: export.name.clone(),
                        span: export.span,
                        kind: export.kind,
                    })
                    .collect()
            });
        }
        let mut globals = HashMap::new();
        for name in [
            "#rils_native_print",
            "#rils_native_println",
            "type_of",
            "#rils_native_assert",
            "None",
        ] {
            globals.insert(
                name.into(),
                Definition {
                    span: None,
                    id: None,
                    kind: SymbolKind::Function,
                },
            );
        }
        for builtin in rils_builtins::BUILTINS {
            if builtin.path.contains("::") {
                continue;
            }
            let kind = match builtin.kind {
                rils_builtins::BuiltinKind::Module => SymbolKind::Module,
                rils_builtins::BuiltinKind::Trait => SymbolKind::Trait,
                rils_builtins::BuiltinKind::Primitive
                | rils_builtins::BuiltinKind::Struct
                | rils_builtins::BuiltinKind::Enum => SymbolKind::Type,
                rils_builtins::BuiltinKind::Function => SymbolKind::Function,
            };
            globals.insert(
                builtin.path.into(),
                Definition {
                    span: None,
                    id: None,
                    kind,
                },
            );
        }
        for integer in crate::types::IntegerType::ALL {
            globals.insert(
                integer.name().into(),
                Definition {
                    span: None,
                    id: None,
                    kind: SymbolKind::Type,
                },
            );
        }
        for float in [crate::types::FloatType::F32, crate::types::FloatType::F64] {
            globals.insert(
                float.name().into(),
                Definition {
                    span: None,
                    id: None,
                    kind: SymbolKind::Type,
                },
            );
        }
        for name in ["core", "std", "prelude", "crate", "self", "super"] {
            globals.insert(
                name.into(),
                Definition {
                    span: None,
                    id: None,
                    kind: SymbolKind::Module,
                },
            );
        }
        for name in host_functions.keys() {
            let Some(root) = name.split("::").next() else {
                continue;
            };
            globals.entry(root.into()).or_insert(Definition {
                span: None,
                id: None,
                kind: if name.contains("::") {
                    SymbolKind::Module
                } else {
                    SymbolKind::Function
                },
            });
        }
        Self {
            source_id,
            next_symbol: HashMap::new(),
            scopes: vec![globals],
            glob_imports: vec![false],
            module_path: Vec::new(),
            module_exports,
            trait_members: HashMap::new(),
            type_aliases: HashMap::new(),
            host_functions: host_functions.clone(),
            result: DocumentAnalysis::default(),
        }
    }

    fn analyze(mut self, program: &Program) -> DocumentAnalysis {
        self.collect_trait_members(&program.statements);
        self.collect_type_aliases(&program.statements);
        self.macros(program);
        self.statements(&program.statements);
        self.type_references(program);
        let inference = type_inference::infer_with_host_functions(program, &self.host_functions);
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
            .extend(crate::trait_check::analyze(program));
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
        self.result.expression_types = inference.expression_types;
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
            let definition_id =
                self.definition_only(&definition.name, definition.name_span, SymbolKind::Macro);
            for span in &definition.references {
                self.result.symbols.push(SymbolOccurrence {
                    name: definition.name.clone(),
                    span: *span,
                    definition_span: Some(definition.name_span),
                    symbol_id: None,
                    definition_id: Some(definition_id),
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
                    self.module_path.push(name.clone());
                    self.with_scope(|analyzer| analyzer.statements(statements));
                    self.module_path.pop();
                }
            }
            Stmt::Use { imports, .. } => imports::analyze(self, imports),
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
                let qualified_name = segments.join("::");
                if let (Some(member), Some(signature)) =
                    (segments.last(), self.host_functions.get(&qualified_name))
                {
                    let parameters = signature
                        .parameters
                        .as_ref()
                        .map(|parameters| {
                            parameters
                                .iter()
                                .map(ToString::to_string)
                                .collect::<Vec<_>>()
                                .join(", ")
                        })
                        .unwrap_or_else(|| "...".into());
                    self.result.symbols.push(SymbolOccurrence {
                        name: member.clone(),
                        span: member_name_span(*span, member),
                        definition_span: None,
                        symbol_id: None,
                        definition_id: None,
                        kind: SymbolKind::Function,
                        is_definition: false,
                        inferred_type: Some(signature.as_type()),
                        detail: Some(format!(
                            "host fn {qualified_name}({parameters}) -> {}",
                            signature.return_type
                        )),
                    });
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
                            symbol_id: None,
                            definition_id: None,
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
                symbol_id: None,
                definition_id: None,
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
                    symbol_id: None,
                    definition_id: None,
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
            Expr::Cast { operand, .. } => self.expression(operand),
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
                        symbol_id: None,
                        definition_id: None,
                        kind: SymbolKind::Method,
                        is_definition: false,
                        inferred_type: None,
                        detail: None,
                    });
                } else {
                    self.expression(callee);
                    if let Expr::Path { segments, span } = callee.as_ref()
                        && segments.len() > 1
                        && let Some(member) = segments.last()
                    {
                        let member_span = member_name_span(*span, member);
                        if !self
                            .result
                            .symbols
                            .iter()
                            .any(|symbol| symbol.span == member_span && symbol.name == *member)
                        {
                            self.result.symbols.push(SymbolOccurrence {
                                name: member.clone(),
                                span: member_span,
                                definition_span: None,
                                symbol_id: None,
                                definition_id: None,
                                kind: SymbolKind::Function,
                                is_definition: false,
                                inferred_type: None,
                                detail: None,
                            });
                        }
                    }
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
        let id = self.definition_only(name, span, kind);
        self.scopes.last_mut().expect("scope exists").insert(
            name.into(),
            Definition {
                span: Some(span),
                id: Some(id),
                kind,
            },
        );
    }

    fn definition_only(&mut self, name: &str, span: Span, kind: SymbolKind) -> SymbolId {
        let source = if span.source == SourceId::UNKNOWN {
            self.source_id
        } else {
            span.source
        };
        let next_symbol = self.next_symbol.entry(source).or_insert(1);
        let id = SymbolId {
            source,
            local: *next_symbol,
        };
        *next_symbol = next_symbol.checked_add(1).expect("symbol id overflow");
        self.result.symbols.push(SymbolOccurrence {
            name: name.into(),
            span,
            definition_span: Some(span),
            symbol_id: Some(id),
            definition_id: Some(id),
            kind,
            is_definition: true,
            inferred_type: None,
            detail: None,
        });
        id
    }

    fn reference(&mut self, name: &str, span: Span, fallback_kind: SymbolKind) {
        if let Some(definition) = self.lookup(name).cloned() {
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: definition.span,
                symbol_id: None,
                definition_id: definition.id,
                kind: definition.kind,
                is_definition: false,
                inferred_type: None,
                detail: None,
            });
        } else {
            if !self.glob_imports.iter().rev().any(|has_glob| *has_glob) {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("undefined name `{name}`"),
                    span,
                ));
            }
            self.result.symbols.push(SymbolOccurrence {
                name: name.into(),
                span,
                definition_span: None,
                symbol_id: None,
                definition_id: None,
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
            let definition_id = self
                .result
                .symbols
                .iter()
                .find(|symbol| {
                    symbol.is_definition
                        && symbol.name == reference.name
                        && matches!(symbol.kind, SymbolKind::Type | SymbolKind::Trait)
                })
                .and_then(|symbol| symbol.symbol_id);
            if resolved.is_none() && !reference.is_builtin {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("undefined type or trait `{}`", reference.name),
                    reference.span,
                ));
            }
            let key_type = match reference.name.as_str() {
                "HashMap" => reference.arguments.first(),
                "HashSet" => reference.arguments.first(),
                _ => None,
            };
            if let Some(key_type) = key_type
                && !hash_key_type_supported(&self.expand_type(key_type, &mut HashSet::new()))
            {
                self.result.diagnostics.push(AnalysisDiagnostic::error(
                    format!("type `{key_type}` does not implement Eq + Hash"),
                    reference.span,
                ));
            }
            self.result.symbols.push(SymbolOccurrence {
                name: reference.name.clone(),
                span: reference.span,
                definition_span: resolved,
                symbol_id: None,
                definition_id,
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
        self.glob_imports.push(false);
        action(self);
        self.glob_imports.pop();
        self.scopes.pop();
    }
}

fn member_name_span(span: Span, name: &str) -> Span {
    Span::new(span.end.saturating_sub(name.len()), span.end)
}

fn hash_key_type_supported(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Bool
            | Type::Char
            | Type::String
            | Type::Integer(_)
            | Type::IntegerVariable(_)
            | Type::Variable(_)
            | Type::Unknown
    )
}

#[cfg(test)]
#[path = "analysis/tests.rs"]
mod tests;
