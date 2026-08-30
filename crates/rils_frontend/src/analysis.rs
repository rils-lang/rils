mod collector;
mod details;
mod symbols;
mod types;
mod visitor;

use details::*;

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

pub use crate::semantic::{SymbolContainer, SymbolKind};
use imports::{ModuleExport, collect_module_exports};

/// A public declaration exported by a module outside the document currently
/// being analyzed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExternalModuleExport {
    pub name: String,
    pub span: Span,
    pub definition_id: Option<SymbolId>,
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
    pub def_map: crate::semantic::DefMap,
    pub typeck_results: crate::semantic::TypeckResults,
    pub host_type_resolutions: crate::HostTypeResolutionResults,
    pub verified_trait_impls: Vec<crate::ImplId>,
    verified_trait_impl_spans: Vec<Span>,
}

impl DocumentAnalysis {
    /// Returns the first blocking diagnostic in source order.
    pub fn first_error(&self) -> Option<&AnalysisDiagnostic> {
        self.diagnostics
            .iter()
            .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.diagnostics.extend(other.diagnostics);
        self.symbols.extend(other.symbols);
        self.inlay_hints.extend(other.inlay_hints);
        self.def_map.extend(other.def_map);
        self.typeck_results.extend(other.typeck_results);
        self.host_type_resolutions
            .extend(other.host_type_resolutions);
        self.verified_trait_impls.extend(other.verified_trait_impls);
        self.verified_trait_impl_spans
            .extend(other.verified_trait_impl_spans);
    }
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
    span: Option<Span>,
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
    let host_type_resolutions = crate::resolve_host_types(program, SourceId::UNKNOWN, host_types);
    let resolution_errors = host_type_resolutions.errors().to_vec();
    let mut analysis = Analyzer::new(AnalyzerInput {
        source_id: SourceId::UNKNOWN,
        host_functions,
        host_types,
        program,
        external_exports: &HashMap::new(),
        module_path: &[],
        host_type_resolutions: &host_type_resolutions,
        host_contract: None,
    })
    .analyze(program, program, &host_type_resolutions);
    analysis.host_type_resolutions = host_type_resolutions;
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
    analyze_program_in_module_with_external_exports_and_host_types(
        program,
        source_id,
        host_functions,
        host_types,
        external_exports,
        &[],
        None,
    )
}

pub(crate) fn analyze_program_in_module_with_external_exports_and_host_types(
    program: &Program,
    source_id: SourceId,
    host_functions: &HashMap<String, FunctionSignature>,
    host_types: &HashSet<String>,
    external_exports: &HashMap<String, Vec<ExternalModuleExport>>,
    module_path: &[String],
    host_contract: Option<&rils_host::HostContract>,
) -> DocumentAnalysis {
    let host_type_resolutions = crate::resolve_host_types(program, source_id, host_types);
    let resolution_errors = host_type_resolutions.errors().to_vec();
    let mut analysis = Analyzer::new(AnalyzerInput {
        source_id,
        host_functions,
        host_types,
        program,
        external_exports,
        module_path,
        host_type_resolutions: &host_type_resolutions,
        host_contract,
    })
    .analyze(program, program, &host_type_resolutions);
    analysis.host_type_resolutions = host_type_resolutions;
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
    member_receivers: HashMap<Span, crate::ExprId>,
    expression_ids: crate::semantic::ExpressionIdentityMap,
    pattern_ids: crate::semantic::PatternIdentityMap,
    host_type_resolutions: crate::HostTypeResolutionResults,
    host_contract: Option<rils_host::HostContract>,
    self_types: Vec<Option<String>>,
    self_type_references: HashMap<Span, String>,
    type_aliases: HashMap<String, TypeAliasDefinition>,
    host_functions: HashMap<String, FunctionSignature>,
    host_types: HashSet<String>,
    host_type_segments: HashSet<String>,
    result: DocumentAnalysis,
    owner_ids: crate::semantic::SemanticOwnerIds,
}

struct AnalyzerInput<'a> {
    source_id: SourceId,
    host_functions: &'a HashMap<String, FunctionSignature>,
    host_types: &'a HashSet<String>,
    program: &'a Program,
    external_exports: &'a HashMap<String, Vec<ExternalModuleExport>>,
    module_path: &'a [String],
    host_type_resolutions: &'a crate::HostTypeResolutionResults,
    host_contract: Option<&'a rils_host::HostContract>,
}

impl Analyzer {
    fn new(input: AnalyzerInput<'_>) -> Self {
        let AnalyzerInput {
            source_id,
            host_functions,
            host_types,
            program,
            external_exports,
            module_path,
            host_type_resolutions,
            host_contract,
        } = input;
        let definition_modules = external_exports
            .values()
            .flatten()
            .filter(|export| export.span.source == source_id)
            .map(|export| (export.span, export.module_path.clone()))
            .collect();
        let mut module_exports = collect_module_exports(&program.statements, module_path);
        for (module, exports) in external_exports {
            module_exports.entry(module.clone()).or_insert_with(|| {
                exports
                    .iter()
                    .map(|export| ModuleExport {
                        name: export.name.clone(),
                        span: export.span,
                        definition_id: export.definition_id,
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
        if !module_path.is_empty()
            && let Some(exports) = external_exports.get("")
        {
            for export in exports {
                globals.entry(export.name.clone()).or_insert(Definition {
                    span: Some(export.span),
                    id: export.definition_id,
                    kind: export.kind,
                    container: Some(SymbolContainer::Module("crate".into())),
                });
            }
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
        for name in host_types {
            let Some(root) = name.split("::").next() else {
                continue;
            };
            globals.entry(root.into()).or_insert(Definition {
                span: None,
                id: None,
                kind: if name.contains("::") {
                    SymbolKind::Module
                } else {
                    SymbolKind::Type
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
        let mut analyzer = Self {
            source_id,
            next_symbol: HashMap::new(),
            scopes: vec![globals],
            glob_imports: vec![false],
            module_path: module_path.to_vec(),
            definition_modules,
            module_exports,
            trait_members: HashMap::new(),
            inherent_methods: HashMap::new(),
            enum_variants: HashMap::new(),
            struct_fields,
            member_receivers: HashMap::new(),
            expression_ids: crate::semantic::ExpressionIdentityMap::allocate(program, source_id),
            pattern_ids: crate::semantic::PatternIdentityMap::allocate(program, source_id),
            host_type_resolutions: host_type_resolutions.clone(),
            host_contract: host_contract.cloned(),
            self_types: vec![None],
            self_type_references: collect_self_type_references(program),
            type_aliases: HashMap::new(),
            host_functions: host_functions.clone(),
            host_types: host_types.clone(),
            host_type_segments,
            result: DocumentAnalysis::default(),
            owner_ids: crate::semantic::SemanticOwnerIds::default(),
        };
        analyzer.collect_host_enum_variants();
        analyzer
    }

    fn analyze(
        mut self,
        program: &Program,
        inference_program: &Program,
        host_type_resolutions: &crate::HostTypeResolutionResults,
    ) -> DocumentAnalysis {
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
                let paths = if module.is_empty() {
                    vec![export.name.clone(), format!("crate::{}", export.name)]
                } else {
                    vec![
                        format!("{module}::{}", export.name),
                        format!("crate::{module}::{}", export.name),
                    ]
                };
                for path in paths {
                    inference_functions
                        .entry(path)
                        .or_insert_with(|| signature.clone());
                }
            }
        }
        let inference = type_inference::infer_with_host_functions_and_host_types(
            inference_program,
            self.source_id,
            &inference_functions,
            host_type_resolutions,
            self.host_contract.as_ref(),
        );
        // Every checker consumes the same immutable source AST. Canonical Host
        // identities live in side tables and must not be written back into syntax.
        let checker_expression_ids =
            crate::semantic::ExpressionIdentityMap::allocate(program, self.source_id);
        let expression_types = crate::semantic::ExpressionTypes::new(
            &checker_expression_ids,
            &inference.expression_types_by_id,
        );
        self.enrich_member_symbols(&inference.expression_types_by_id);
        self.result.diagnostics.extend(crate::control_flow::analyze(
            program,
            expression_types,
            self.host_contract.as_ref(),
        ));
        self.result.diagnostics.extend(crate::ownership::analyze(
            program,
            &inference.binding_types,
            expression_types,
            &self.host_types,
        ));
        self.result
            .diagnostics
            .extend(crate::static_type_check::analyze(
                program,
                expression_types,
                self.source_id,
                host_type_resolutions,
            ));
        let trait_check = crate::trait_check::analyze_with_host_types(program, &self.host_types);
        self.result.diagnostics.extend(trait_check.diagnostics);
        self.result
            .verified_trait_impl_spans
            .extend(trait_check.verified_impls);
        self.result.diagnostics.extend(crate::format_check::analyze(
            program,
            expression_types,
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
        // Host declarations injected by an embedding compiler have empty
        // synthetic spans. They participate in name and type resolution but
        // must never become editor symbols for the user's source document.
        self.result
            .symbols
            .retain(|symbol| symbol.span.start < symbol.span.end);
        self.result.inlay_hints = inference
            .hints
            .into_iter()
            .map(|hint| InlayTypeHint {
                position: hint.position,
                span: hint.span,
                label: format!("{}{ty}", hint.prefix, ty = hint.ty),
            })
            .collect();
        let def_map = crate::semantic::DefMap::from_symbols_and_owners(
            &self.result.symbols,
            std::mem::take(&mut self.owner_ids),
        );
        self.result.verified_trait_impls =
            std::mem::take(&mut self.result.verified_trait_impl_spans)
                .into_iter()
                .filter_map(|span| def_map.impl_at(span))
                .collect();
        let mut typeck_results = crate::semantic::TypeckResults::from_expression_types(
            inference.expression_ids.into_ids(),
            inference.expression_types_by_id,
        );
        crate::semantic::resolve_program_calls(
            program,
            self.source_id,
            &def_map,
            &self.host_functions,
            &mut typeck_results,
            &self.module_path,
            host_type_resolutions,
        );
        self.result.def_map = def_map;
        self.result.typeck_results = typeck_results;
        self.result
    }
}

#[cfg(test)]
#[path = "../tests/unit/analysis.rs"]
mod tests;
