use std::collections::{HashMap, HashSet};

use crate::{
    FrontendError,
    ast::{
        AssociatedType, Block, EnumVariant, Expr, GenericParameter, ImplMethod, NamedField,
        Parameter, Pattern, Program, RecordField, Stmt, TraitMethod,
    },
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
    pub inferred_type: Option<Type>,
    pub detail: Option<String>,
    pub module_path: String,
    pub fields: Vec<ExternalTypeField>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalTypeField {
    pub name: String,
    pub span: Span,
    pub ty: Type,
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
    pub container: Option<SymbolContainer>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SymbolContainer {
    Module(String),
    Type(String),
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
    container: Option<SymbolContainer>,
}

#[derive(Clone)]
struct TypeAliasDefinition {
    parameters: Vec<String>,
    target: Type,
}

#[derive(Clone)]
struct InherentMethod {
    owner: String,
    span: Span,
    detail: String,
}

#[derive(Clone)]
struct EnumVariantSymbol {
    span: Span,
    detail: String,
    owner: String,
}

#[derive(Clone)]
struct StructFieldSymbol {
    span: Span,
    ty: Type,
    detail: String,
    owner: String,
}

fn callable_detail(name: &str, ty: &Type) -> String {
    let Type::Function {
        parameters: Some(parameters),
        return_type,
    } = ty
    else {
        return format!("fn {name}: {ty}");
    };
    let parameters = parameters
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(", ");
    format!("fn {name}({parameters}) -> {return_type}")
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
    analyze_program_with_host_declarations(program, host_functions, &HashSet::new())
}

#[doc(hidden)]
pub fn analyze_program_with_host_declarations(
    program: &Program,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
) -> DocumentAnalysis {
    let mut program = program.clone();
    let resolution_errors = crate::resolve_host_type_names(&mut program, host_types);
    let mut analysis = Analyzer::new(
        SourceId::UNKNOWN,
        host_functions,
        host_types,
        &program,
        &HashMap::new(),
    )
    .analyze(&program);
    append_host_type_resolution_errors(&mut analysis, resolution_errors);
    analysis
}

pub fn analyze(source: &str) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_host_functions(source, &HashMap::new())
}

#[doc(hidden)]
pub fn analyze_with_host_functions(
    source: &str,
    host_functions: &HashMap<String, FunctionSignature>,
) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_host_declarations(source, host_functions, &HashSet::new())
}

#[doc(hidden)]
pub fn analyze_with_host_declarations(
    source: &str,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
) -> Result<DocumentAnalysis, FrontendError> {
    analyze_with_source_id_and_external_exports_and_host_types(
        source,
        SourceId::UNKNOWN,
        host_functions,
        host_types,
        &HashMap::new(),
    )
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
    analyze_with_source_id_and_external_exports_and_host_types(
        source,
        source_id,
        host_functions,
        &HashSet::new(),
        external_exports,
    )
}

pub fn analyze_with_source_id_and_external_exports_and_host_types(
    source: &str,
    source_id: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> Result<DocumentAnalysis, FrontendError> {
    let tokens = crate::lexer::lex_with_source_id(source, source_id).map_err(FrontendError::Lex)?;
    let program = crate::parser::parse(tokens).map_err(FrontendError::Parse)?;
    Ok(
        analyze_program_with_source_id_and_external_exports_and_host_types(
            &program,
            source_id,
            host_functions,
            host_types,
            external_exports,
        ),
    )
}

#[doc(hidden)]
pub fn analyze_program_with_source_id_and_external_exports_and_host_types(
    program: &Program,
    source_id: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
) -> DocumentAnalysis {
    let mut program = program.clone();
    let resolution_errors = crate::resolve_host_type_names(&mut program, host_types);
    let mut analysis = Analyzer::new(
        source_id,
        host_functions,
        host_types,
        &program,
        external_exports,
    )
    .analyze(&program);
    append_host_type_resolution_errors(&mut analysis, resolution_errors);
    analysis
}

fn append_host_type_resolution_errors(
    analysis: &mut DocumentAnalysis,
    errors: Vec<crate::HostTypeResolutionError>,
) {
    analysis.diagnostics.extend(
        errors
            .into_iter()
            .map(|error| AnalysisDiagnostic::error(error.message, error.span)),
    );
    analysis
        .diagnostics
        .sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
    analysis
        .diagnostics
        .dedup_by(|left, right| left.span == right.span && left.message == right.message);
}

struct Analyzer {
    source_id: SourceId,
    next_symbol: HashMap<SourceId, u32>,
    scopes: Vec<HashMap<String, Definition>>,
    glob_imports: Vec<bool>,
    module_path: Vec<String>,
    definition_modules: HashMap<Span, String>,
    module_exports: HashMap<String, Vec<ModuleExport>>,
    trait_members: HashMap<(String, String), Span>,
    inherent_methods: HashMap<String, Vec<InherentMethod>>,
    enum_variants: HashMap<(String, String), EnumVariantSymbol>,
    struct_fields: HashMap<String, Vec<HashMap<String, StructFieldSymbol>>>,
    member_receivers: HashMap<Span, Span>,
    self_types: Vec<Option<String>>,
    self_type_references: HashMap<Span, String>,
    type_aliases: HashMap<String, TypeAliasDefinition>,
    host_functions: HashMap<String, FunctionSignature>,
    host_types: HashSet<String>,
    host_type_segments: HashSet<String>,
    result: DocumentAnalysis,
}

impl Analyzer {
    fn new(
        source_id: SourceId,
        host_functions: &HashMap<String, FunctionSignature>,
        host_types: &HashSet<String>,
        program: &Program,
        external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
    ) -> Self {
        let definition_modules = external_exports
            .values()
            .flatten()
            .filter(|export| export.span.source == source_id)
            .map(|export| (export.span, export.module_path.clone()))
            .collect();
        let mut module_exports = collect_module_exports(&program.statements);
        for (module, exports) in external_exports {
            module_exports.entry(module.clone()).or_insert_with(|| {
                exports
                    .iter()
                    .map(|export| ModuleExport {
                        name: export.name.clone(),
                        span: export.span,
                        kind: export.kind,
                        inferred_type: export.inferred_type.clone(),
                        detail: export.detail.clone(),
                        module_path: export.module_path.clone(),
                    })
                    .collect()
            });
        }
        let mut globals = HashMap::new();
        let mut struct_fields = HashMap::new();
        for exports in external_exports.values() {
            for export in exports {
                if export.fields.is_empty() || export.span.source == source_id {
                    continue;
                }
                struct_fields
                    .entry(export.name.clone())
                    .or_insert_with(Vec::new)
                    .push(
                        export
                            .fields
                            .iter()
                            .map(|field| {
                                (
                                    field.name.clone(),
                                    StructFieldSymbol {
                                        span: field.span,
                                        ty: field.ty.clone(),
                                        detail: format!("field {}: {}", field.name, field.ty),
                                        owner: export.name.clone(),
                                    },
                                )
                            })
                            .collect(),
                    );
            }
        }
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
                    container: None,
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
                    container: None,
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
                    container: None,
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
                    container: None,
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
                    container: None,
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
                container: None,
            });
        }
        let host_type_segments = host_functions
            .values()
            .flat_map(|signature| {
                signature
                    .parameters
                    .iter()
                    .flatten()
                    .chain(std::iter::once(&signature.return_type))
            })
            .filter_map(|ty| match ty {
                Type::Named { name, arguments } if arguments.is_empty() => Some(name.as_str()),
                _ => None,
            })
            .flat_map(|name| name.split("::").map(str::to_owned))
            .chain(
                host_types
                    .iter()
                    .flat_map(|name| name.split("::").map(str::to_owned)),
            )
            .collect();
        Self {
            source_id,
            next_symbol: HashMap::new(),
            scopes: vec![globals],
            glob_imports: vec![false],
            module_path: Vec::new(),
            definition_modules,
            module_exports,
            trait_members: HashMap::new(),
            inherent_methods: HashMap::new(),
            enum_variants: HashMap::new(),
            struct_fields,
            member_receivers: HashMap::new(),
            self_types: vec![None],
            self_type_references: collect_self_type_references(program),
            type_aliases: HashMap::new(),
            host_functions: host_functions.clone(),
            host_types: host_types.clone(),
            host_type_segments,
            result: DocumentAnalysis::default(),
        }
    }

    fn analyze(mut self, program: &Program) -> DocumentAnalysis {
        self.collect_trait_members(&program.statements);
        self.collect_inherent_methods(&program.statements);
        self.collect_enum_variants(&program.statements);
        self.collect_struct_fields(&program.statements);
        self.collect_type_aliases(&program.statements);
        self.macros(program);
        self.statements(&program.statements);
        self.type_references(program);
        let mut inference_functions = self.host_functions.clone();
        for (module, exports) in &self.module_exports {
            for export in exports {
                let Some(Type::Function {
                    parameters,
                    return_type,
                }) = &export.inferred_type
                else {
                    continue;
                };
                let signature = FunctionSignature {
                    parameters: parameters.clone(),
                    return_type: (**return_type).clone(),
                };
                for path in [
                    format!("{module}::{}", export.name),
                    format!("crate::{module}::{}", export.name),
                ] {
                    inference_functions
                        .entry(path)
                        .or_insert_with(|| signature.clone());
                }
            }
        }
        let inference = type_inference::infer_with_host_functions(program, &inference_functions);
        self.enrich_member_symbols(&inference.expression_types);
        self.result.diagnostics.extend(crate::control_flow::analyze(
            program,
            &inference.expression_types,
        ));
        self.result.diagnostics.extend(crate::ownership::analyze(
            program,
            &inference.binding_types,
            &inference.expression_types,
            &self.host_types,
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
        self.result.diagnostics.extend(crate::format_check::analyze(
            program,
            &inference.expression_types,
            &self.host_types,
        ));
        self.result
            .diagnostics
            .sort_by_key(|diagnostic| (diagnostic.span.start, diagnostic.span.end));
        self.result
            .diagnostics
            .dedup_by(|left, right| left.span == right.span && left.message == right.message);
        let definition_ids = self
            .result
            .symbols
            .iter()
            .filter_map(|symbol| {
                symbol
                    .is_definition
                    .then_some((symbol.span, symbol.symbol_id?))
            })
            .collect::<HashMap<_, _>>();
        for symbol in &mut self.result.symbols {
            if symbol.is_definition {
                if let Some(inferred_type) = inference.binding_types.get(&symbol.span) {
                    symbol.inferred_type = Some(inferred_type.clone());
                }
            }
            if let Some(definition_span) = symbol.definition_span {
                if symbol.definition_id.is_none() {
                    symbol.definition_id = definition_ids.get(&definition_span).copied();
                }
                if let Some(inferred_type) = inference.binding_types.get(&definition_span) {
                    symbol.inferred_type = Some(inferred_type.clone());
                }
            }
        }
        let definition_details = self
            .result
            .symbols
            .iter()
            .filter_map(|symbol| {
                symbol
                    .is_definition
                    .then_some((symbol.symbol_id?, symbol.detail.clone()?))
            })
            .collect::<HashMap<_, _>>();
        for symbol in &mut self.result.symbols {
            if !symbol.is_definition && symbol.detail.is_none() {
                symbol.detail = symbol
                    .definition_id
                    .and_then(|id| definition_details.get(&id).cloned());
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

    fn collect_struct_fields(&mut self, statements: &[Stmt]) {
        fn visit(
            statements: &[Stmt],
            output: &mut HashMap<String, Vec<HashMap<String, StructFieldSymbol>>>,
        ) {
            for statement in statements {
                let statement = match statement {
                    Stmt::Public { statement, .. } => statement.as_ref(),
                    statement => statement,
                };
                match statement {
                    Stmt::Struct { name, fields, .. } => {
                        output.entry(name.clone()).or_default().push(
                            fields
                                .iter()
                                .map(|field| {
                                    (
                                        field.name.clone(),
                                        StructFieldSymbol {
                                            span: field.span,
                                            ty: field.type_annotation.clone(),
                                            detail: format!(
                                                "field {}: {}",
                                                field.name, field.type_annotation
                                            ),
                                            owner: name.clone(),
                                        },
                                    )
                                })
                                .collect(),
                        );
                    }
                    Stmt::Module {
                        statements: Some(children),
                        ..
                    } => visit(children, output),
                    _ => {}
                }
            }
        }

        visit(statements, &mut self.struct_fields);
    }

    fn enrich_member_symbols(&mut self, expression_types: &HashMap<Span, Type>) {
        let mut updates = Vec::new();
        for (index, symbol) in self.result.symbols.iter().enumerate() {
            if symbol.is_definition {
                continue;
            }
            let Some(receiver_span) = self.member_receivers.get(&symbol.span) else {
                continue;
            };
            let Some(receiver_type) = expression_types.get(receiver_span) else {
                continue;
            };
            let receiver_type = match receiver_type {
                Type::Reference { inner, .. } => inner.as_ref(),
                receiver_type => receiver_type,
            };
            if symbol.kind == SymbolKind::Method
                && let Some(method_type) =
                    crate::standard_library::builtin_member_type(receiver_type, &symbol.name)
            {
                updates.push((
                    index,
                    None,
                    None,
                    Some(method_type.clone()),
                    Some(callable_detail(&symbol.name, &method_type)),
                    None,
                ));
                continue;
            }
            if symbol.kind != SymbolKind::Field {
                continue;
            }
            let Type::Named { name, .. } = receiver_type else {
                continue;
            };
            let Some(definitions) = self.struct_fields.get(name) else {
                continue;
            };
            let candidates = definitions
                .iter()
                .filter_map(|fields| fields.get(&symbol.name))
                .collect::<Vec<_>>();
            let [field] = candidates.as_slice() else {
                continue;
            };
            let definition_id = self
                .result
                .symbols
                .iter()
                .find(|candidate| {
                    candidate.is_definition
                        && candidate.kind == SymbolKind::Field
                        && candidate.span == field.span
                })
                .and_then(|candidate| candidate.symbol_id);
            updates.push((
                index,
                Some(field.span),
                definition_id,
                Some(field.ty.clone()),
                Some(field.detail.clone()),
                Some(SymbolContainer::Type(field.owner.clone())),
            ));
        }
        for (index, definition_span, definition_id, inferred_type, detail, container) in updates {
            let symbol = &mut self.result.symbols[index];
            symbol.definition_span = definition_span;
            symbol.definition_id = definition_id;
            symbol.inferred_type = inferred_type;
            symbol.detail = detail;
            if container.is_some() {
                symbol.container = container;
            }
        }
    }

    fn record_field_symbol(&mut self, type_name: Option<&str>, field: &RecordField) {
        let definition = type_name.and_then(|type_name| {
            let definitions = self.struct_fields.get(type_name)?;
            let candidates = definitions
                .iter()
                .filter_map(|fields| fields.get(&field.name))
                .collect::<Vec<_>>();
            let [field] = candidates.as_slice() else {
                return None;
            };
            Some((*field).clone())
        });
        let definition_id = definition.as_ref().and_then(|field| {
            self.result
                .symbols
                .iter()
                .find(|candidate| {
                    candidate.is_definition
                        && candidate.kind == SymbolKind::Field
                        && candidate.span == field.span
                })
                .and_then(|candidate| candidate.symbol_id)
        });
        self.result.symbols.push(SymbolOccurrence {
            name: field.name.clone(),
            span: field.name_span,
            definition_span: definition.as_ref().map(|field| field.span),
            symbol_id: None,
            definition_id,
            kind: SymbolKind::Field,
            is_definition: false,
            inferred_type: definition.as_ref().map(|field| field.ty.clone()),
            detail: definition.as_ref().map(|field| field.detail.clone()),
            container: definition.map(|field| SymbolContainer::Type(field.owner)),
        });
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

    fn collect_inherent_methods(&mut self, statements: &[Stmt]) {
        for statement in statements {
            if let Stmt::Public { statement, .. } = statement {
                self.collect_inherent_methods(std::slice::from_ref(statement));
                continue;
            }
            if let Stmt::Module {
                statements: Some(statements),
                ..
            } = statement
            {
                self.collect_inherent_methods(statements);
                continue;
            }
            let Stmt::Impl {
                trait_name: None,
                target: Type::Named { name: owner, .. },
                methods,
                ..
            } = statement
            else {
                continue;
            };
            for method in methods {
                self.inherent_methods
                    .entry(method.name.clone())
                    .or_default()
                    .push(InherentMethod {
                        owner: owner.clone(),
                        span: method.name_span,
                        detail: impl_method_detail(method),
                    });
            }
        }
    }

    fn collect_enum_variants(&mut self, statements: &[Stmt]) {
        for statement in statements {
            if let Stmt::Public { statement, .. } = statement {
                self.collect_enum_variants(std::slice::from_ref(statement));
                continue;
            }
            if let Stmt::Module {
                statements: Some(statements),
                ..
            } = statement
            {
                self.collect_enum_variants(statements);
                continue;
            }
            let Stmt::Enum { name, variants, .. } = statement else {
                continue;
            };
            for variant in variants {
                let (variant_name, span) = enum_variant_name_and_span(variant);
                self.enum_variants.insert(
                    (name.clone(), variant_name.into()),
                    EnumVariantSymbol {
                        span,
                        detail: enum_variant_declaration(name, variant),
                        owner: name.clone(),
                    },
                );
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
                    container: None,
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
            Stmt::Public { statement, .. } => {
                let first_symbol = self.result.symbols.len();
                self.statement(statement);
                if let Some(symbol) = self.result.symbols.get_mut(first_symbol)
                    && symbol.is_definition
                {
                    if let Some(detail) = &mut symbol.detail {
                        *detail = format!("pub {detail}");
                    } else if symbol.kind == SymbolKind::Module {
                        symbol.detail = Some(format!("pub mod {}", symbol.name));
                    }
                }
            }
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
                return_type,
                body,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Function);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(function_detail(
                    name,
                    generic_parameters,
                    parameters,
                    return_type.as_ref(),
                ));
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
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(struct_detail(name, generic_parameters, fields));
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for field in fields {
                    self.definition_only(&field.name, field.span, SymbolKind::Field);
                    self.set_last_detail(format!(
                        "field {}: {}",
                        field.name, field.type_annotation
                    ));
                    self.set_last_container(SymbolContainer::Type(name.clone()));
                    if let Some(symbol) = self.result.symbols.last_mut() {
                        symbol.inferred_type = Some(field.type_annotation.clone());
                    }
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
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(enum_detail(name, generic_parameters, variants));
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for variant in variants {
                    let (variant_name, span) = enum_variant_name_and_span(variant);
                    self.definition_only(variant_name, span, SymbolKind::Variant);
                    self.set_last_detail(enum_variant_declaration(name, variant));
                    self.set_last_container(SymbolContainer::Type(name.clone()));
                    if let EnumVariant::Record { fields, .. } = variant {
                        for field in fields {
                            self.definition_only(&field.name, field.span, SymbolKind::Field);
                            self.set_last_detail(format!(
                                "field {}: {}",
                                field.name, field.type_annotation
                            ));
                            self.set_last_container(SymbolContainer::Type(format!(
                                "{name}::{variant_name}"
                            )));
                        }
                    }
                }
            }
            Stmt::Trait {
                name,
                name_span,
                bounds,
                associated_types,
                methods,
                ..
            } => {
                self.define(name, *name_span, SymbolKind::Trait);
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
                self.set_last_detail(trait_detail(name, bounds, associated_types, methods));
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    self.set_last_detail(associated_type_detail(associated));
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    self.set_last_detail(trait_method_detail(method));
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
                self.set_last_container(SymbolContainer::Module(
                    self.module_path_for_definition(*name_span),
                ));
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
                target,
                associated_types,
                methods,
                ..
            } => {
                let self_type = match target {
                    Type::Named { name, .. } => Some(name.clone()),
                    _ => None,
                };
                for parameter in generic_parameters {
                    self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                }
                for associated in associated_types {
                    self.definition_only(&associated.name, associated.name_span, SymbolKind::Type);
                    self.set_last_detail(associated_type_detail(associated));
                    for parameter in &associated.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                }
                for method in methods {
                    self.definition_only(&method.name, method.name_span, SymbolKind::Method);
                    self.set_last_detail(impl_method_detail(method));
                    if let Some(owner) = &self_type {
                        self.set_last_container(SymbolContainer::Type(owner.clone()));
                    }
                    for parameter in &method.generic_parameters {
                        self.definition_only(&parameter.name, parameter.span, SymbolKind::Type);
                    }
                    self.with_scope(|analyzer| {
                        analyzer.self_types.push(self_type.clone());
                        if let Some(self_type) = &self_type
                            && let Some(definition) = analyzer.lookup(self_type).cloned()
                        {
                            analyzer
                                .scopes
                                .last_mut()
                                .expect("scope exists")
                                .insert("Self".into(), definition);
                        }
                        for parameter in &method.parameters {
                            analyzer.define(&parameter.name, parameter.span, SymbolKind::Parameter);
                        }
                        analyzer.block_contents(&method.body);
                        analyzer.self_types.pop();
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
                // Host type resolution canonicalizes imported paths (for
                // example `Color::new` becomes
                // `unity_engine::Color::new`). Record the type segment at its
                // actual source position so hover does not select the module
                // segment for an imported host type.
                if segments.len() > 1 {
                    for end in (1..segments.len()).rev() {
                        let candidate = segments[..=end].join("::");
                        if self.host_types.contains(&candidate) {
                            let start = span.start
                                + segments[..end]
                                    .iter()
                                    .map(|segment| segment.len() + 2)
                                    .sum::<usize>();
                            let type_name = segments[end].clone();
                            self.result.symbols.push(SymbolOccurrence {
                                name: type_name.clone(),
                                span: Span::new(start, start + type_name.len()),
                                definition_span: None,
                                symbol_id: None,
                                definition_id: None,
                                kind: SymbolKind::Type,
                                is_definition: false,
                                inferred_type: Some(Type::named(candidate)),
                                detail: None,
                                container: None,
                            });
                            break;
                        }
                    }
                }
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
                        container: None,
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
                            container: None,
                        });
                    }
                }
                if let [type_name, member] = segments.as_slice() {
                    let owner = if type_name == "Self" {
                        self.self_types.last().and_then(Clone::clone)
                    } else {
                        Some(type_name.clone())
                    };
                    if let Some(method) = owner.and_then(|owner| {
                        self.inherent_methods
                            .get(member)
                            .and_then(|methods| methods.iter().find(|method| method.owner == owner))
                            .cloned()
                    }) {
                        self.result.symbols.push(SymbolOccurrence {
                            name: member.clone(),
                            span: member_name_span(*span, member),
                            definition_span: Some(method.span),
                            symbol_id: None,
                            definition_id: None,
                            kind: SymbolKind::Method,
                            is_definition: false,
                            inferred_type: None,
                            detail: Some(method.detail),
                            container: Some(SymbolContainer::Type(method.owner)),
                        });
                    }
                }
                if !segments.is_empty() {
                    self.variant_symbol_for_path(segments, *span);
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
                container: None,
            }),
            Expr::Member { object, name, span } => {
                self.expression(object);
                self.member_receivers
                    .insert(member_name_span(*span, name), object.span());
                self.member_symbol(name, *span, SymbolKind::Field);
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
                self.variant_symbol_for_path(path, *span);
                for field in fields {
                    self.record_field_symbol(path.last().map(String::as_str), field);
                    self.expression(&field.value);
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
                    self.member_receivers
                        .insert(member_name_span(*span, name), object.span());
                    self.member_symbol(name, *span, SymbolKind::Method);
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
                                container: None,
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
            Pattern::Wildcard { .. } | Pattern::Literal { .. } | Pattern::None { .. } => {}
            Pattern::Path { path, span } => self.pattern_variant_symbols(path, *span),
            Pattern::Binding { name, span } => {
                self.define(name, *span, SymbolKind::Variable);
            }
            Pattern::Some { inner, .. } => self.pattern(inner),
            Pattern::Ok { inner, .. } | Pattern::Err { inner, .. } => self.pattern(inner),
            Pattern::TupleVariant { path, fields, span } => {
                self.pattern_variant_symbols(path, *span);
                for field in fields {
                    self.pattern(field);
                }
            }
            Pattern::Record { path, fields, span } => {
                self.pattern_variant_symbols(path, *span);
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
        let merges_host_module = kind == SymbolKind::Module
            && self
                .scopes
                .last()
                .and_then(|scope| scope.get(name))
                .is_some_and(|definition| {
                    definition.kind == SymbolKind::Module && definition.span.is_none()
                });
        if !merges_host_module
            && self
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
                container: None,
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
            container: None,
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
                container: definition.container,
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
                container: None,
            });
        }
    }

    fn member_symbol(&mut self, name: &str, span: Span, fallback_kind: SymbolKind) {
        let method = self
            .inherent_methods
            .get(name)
            .and_then(|methods| (methods.len() == 1).then(|| methods[0].clone()));
        self.result.symbols.push(SymbolOccurrence {
            name: name.into(),
            span: member_name_span(span, name),
            definition_span: method.as_ref().map(|method| method.span),
            symbol_id: None,
            definition_id: None,
            kind: method
                .as_ref()
                .map(|_| SymbolKind::Method)
                .unwrap_or(fallback_kind),
            is_definition: false,
            inferred_type: None,
            detail: method.as_ref().map(|method| method.detail.clone()),
            container: method.map(|method| SymbolContainer::Type(method.owner)),
        });
    }

    fn variant_symbol_for_path(&mut self, path: &[String], symbol_span: Span) {
        let [enum_name, variant_name] = path else {
            return;
        };
        let Some(variant) = self
            .enum_variants
            .get(&(enum_name.clone(), variant_name.clone()))
            .cloned()
        else {
            return;
        };
        let variant_start = symbol_span.start + enum_name.len() + 2;
        let variant_span = Span::in_source(
            symbol_span.source,
            variant_start,
            variant_start + variant_name.len(),
        );
        self.result.symbols.push(SymbolOccurrence {
            name: variant_name.clone(),
            span: variant_span,
            definition_span: Some(variant.span),
            symbol_id: None,
            definition_id: None,
            kind: SymbolKind::Variant,
            is_definition: false,
            inferred_type: None,
            detail: Some(variant.detail),
            container: Some(SymbolContainer::Type(variant.owner)),
        });
    }

    fn pattern_variant_symbols(&mut self, path: &[String], symbol_span: Span) {
        if let [enum_name, ..] = path {
            self.reference(
                enum_name,
                Span::in_source(
                    symbol_span.source,
                    symbol_span.start,
                    symbol_span.start + enum_name.len(),
                ),
                SymbolKind::Type,
            );
        }
        self.variant_symbol_for_path(path, symbol_span);
    }

    fn type_references(&mut self, program: &Program) {
        for reference in &program.type_references {
            let resolved_name = self
                .self_type_references
                .get(&reference.span)
                .map_or(reference.name.as_str(), String::as_str);
            let resolved = reference.definition_span.or_else(|| {
                self.result
                    .symbols
                    .iter()
                    .find(|symbol| {
                        symbol.is_definition
                            && symbol.name == resolved_name
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
                        && symbol.name == resolved_name
                        && matches!(symbol.kind, SymbolKind::Type | SymbolKind::Trait)
                })
                .and_then(|symbol| symbol.symbol_id);
            if resolved.is_none()
                && !reference.is_builtin
                && !self.host_type_segments.contains(&reference.name)
            {
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
                container: self
                    .lookup(resolved_name)
                    .and_then(|definition| definition.container.clone()),
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

    fn set_last_detail(&mut self, detail: String) {
        self.result
            .symbols
            .last_mut()
            .expect("definition symbol")
            .detail = Some(detail);
    }

    fn set_last_container(&mut self, container: SymbolContainer) {
        let symbol = self.result.symbols.last_mut().expect("definition symbol");
        symbol.container = Some(container.clone());
        if symbol.is_definition
            && let Some(definition) = self
                .scopes
                .last_mut()
                .and_then(|scope| scope.get_mut(&symbol.name))
        {
            definition.container = Some(container);
        }
    }

    fn module_path_for_definition(&self, span: Span) -> String {
        if let Some(module) = self.definition_modules.get(&span) {
            return module.clone();
        }
        if self.module_path.is_empty() {
            "crate".into()
        } else {
            self.module_path.join("::")
        }
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

fn generic_parameters_detail(parameters: &[GenericParameter]) -> String {
    if parameters.is_empty() {
        return String::new();
    }
    let parameters = parameters
        .iter()
        .map(|parameter| {
            if parameter.bounds.is_empty() {
                parameter.name.clone()
            } else {
                format!("{}: {}", parameter.name, parameter.bounds.join(" + "))
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("<{parameters}>")
}

fn parameter_detail(parameter: &Parameter) -> String {
    if parameter.name == "self"
        && let Some(Type::Reference { mutable, .. }) = &parameter.type_annotation
    {
        return if *mutable {
            "&mut self".into()
        } else {
            "&self".into()
        };
    }
    let name = if parameter.mutable {
        format!("mut {}", parameter.name)
    } else {
        parameter.name.clone()
    };
    parameter
        .type_annotation
        .as_ref()
        .map(|ty| format!("{name}: {ty}"))
        .unwrap_or(name)
}

fn function_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    parameters: &[Parameter],
    return_type: Option<&Type>,
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    let parameters = parameters
        .iter()
        .map(parameter_detail)
        .collect::<Vec<_>>()
        .join(", ");
    let return_type = return_type
        .map(|ty| format!(" -> {ty}"))
        .unwrap_or_default();
    format!("fn {name}{generic_parameters}({parameters}){return_type}")
}

const MAX_HOVER_MEMBERS: usize = 8;

fn hover_member_lines<T>(
    members: &[T],
    member_name: &str,
    render: impl Fn(&T) -> String,
) -> String {
    let mut lines = members
        .iter()
        .take(MAX_HOVER_MEMBERS)
        .map(|member| format!("    {},", render(member)))
        .collect::<Vec<_>>();
    let omitted = members.len().saturating_sub(MAX_HOVER_MEMBERS);
    if omitted > 0 {
        lines.push(format!("    // ... {omitted} more {member_name}"));
    }
    lines.join("\n")
}

fn struct_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    fields: &[NamedField],
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    if fields.is_empty() {
        return format!("struct {name}{generic_parameters}");
    }
    let fields = hover_member_lines(fields, "fields", |field| {
        format!("{}: {}", field.name, field.type_annotation)
    });
    format!("struct {name}{generic_parameters} {{\n{fields}\n}}")
}

fn enum_variant_detail(variant: &EnumVariant) -> String {
    match variant {
        EnumVariant::Unit { name, .. } => name.clone(),
        EnumVariant::Tuple { name, fields, .. } => format!(
            "{name}({})",
            fields
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        EnumVariant::Record { name, fields, .. } => format!(
            "{name} {{ {} }}",
            fields
                .iter()
                .map(|field| format!("{}: {}", field.name, field.type_annotation))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn enum_variant_name_and_span(variant: &EnumVariant) -> (&str, Span) {
    match variant {
        EnumVariant::Unit { name, span }
        | EnumVariant::Tuple { name, span, .. }
        | EnumVariant::Record { name, span, .. } => {
            (name, Span::new(span.start, span.start + name.len()))
        }
    }
}

fn enum_variant_declaration(enum_name: &str, variant: &EnumVariant) -> String {
    format!("{enum_name}::{}", enum_variant_detail(variant))
}

fn enum_detail(
    name: &str,
    generic_parameters: &[GenericParameter],
    variants: &[EnumVariant],
) -> String {
    let generic_parameters = generic_parameters_detail(generic_parameters);
    if variants.is_empty() {
        return format!("enum {name}{generic_parameters}");
    }
    let variants = hover_member_lines(variants, "variants", enum_variant_detail);
    format!("enum {name}{generic_parameters} {{\n{variants}\n}}")
}

fn associated_type_detail(associated: &AssociatedType) -> String {
    let generic_parameters = generic_parameters_detail(&associated.generic_parameters);
    let value = associated
        .value
        .as_ref()
        .map(|value| format!(" = {value}"))
        .unwrap_or_default();
    format!("type {}{generic_parameters}{value}", associated.name)
}

fn trait_method_detail(method: &TraitMethod) -> String {
    function_detail(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
    )
}

fn impl_method_detail(method: &ImplMethod) -> String {
    function_detail(
        &method.name,
        &method.generic_parameters,
        &method.parameters,
        method.return_type.as_ref(),
    )
}

fn trait_detail(
    name: &str,
    bounds: &[String],
    associated_types: &[AssociatedType],
    methods: &[TraitMethod],
) -> String {
    let bounds = if bounds.is_empty() {
        String::new()
    } else {
        format!(": {}", bounds.join(" + "))
    };
    let members = associated_types
        .iter()
        .map(|associated| format!("    {};", associated_type_detail(associated)))
        .chain(
            methods
                .iter()
                .map(|method| format!("    {};", trait_method_detail(method))),
        )
        .collect::<Vec<_>>();
    if members.is_empty() {
        format!("trait {name}{bounds}")
    } else {
        format!("trait {name}{bounds} {{\n{}\n}}", members.join("\n"))
    }
}

fn member_name_span(span: Span, name: &str) -> Span {
    Span::new(span.end.saturating_sub(name.len()), span.end)
}

fn collect_self_type_references(program: &Program) -> HashMap<Span, String> {
    fn visit(
        statements: &[Stmt],
        references: &[crate::ast::TypeReference],
        output: &mut HashMap<Span, String>,
    ) {
        for statement in statements {
            let statement = match statement {
                Stmt::Public { statement, .. } => statement.as_ref(),
                statement => statement,
            };
            match statement {
                Stmt::Module {
                    statements: Some(statements),
                    ..
                } => visit(statements, references, output),
                Stmt::Impl {
                    target: Type::Named { name, .. },
                    span,
                    ..
                } => {
                    for reference in references.iter().filter(|reference| {
                        reference.name == "Self"
                            && reference.span.source == span.source
                            && span.start <= reference.span.start
                            && reference.span.end <= span.end
                    }) {
                        output.insert(reference.span, name.clone());
                    }
                }
                _ => {}
            }
        }
    }

    let mut output = HashMap::new();
    visit(&program.statements, &program.type_references, &mut output);
    output
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
