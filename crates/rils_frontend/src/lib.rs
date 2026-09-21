pub mod analysis;
mod control_flow;
pub mod database;
mod error;
pub mod exports;
mod format_check;
mod host_analysis;
mod host_type_resolution;
mod numeric_literals;
mod ownership;
mod project_analysis;
mod recursive_types;
pub mod semantic;
pub mod standard_library;
mod static_type_check;
mod trait_check;
mod type_inference;

pub use rils_syntax::{ast, default, format, lexer, macros, parser, source, token, types};

pub use database::{
    CompilationSession, ModuleData, ModuleGraph, ProjectId, ProjectModuleId, ProjectSemanticIndex,
    ProjectSyntax, SourceDatabase,
};
pub use error::FrontendError;
pub use host_analysis::{
    analyze_module_with_host, analyze_program_with_host_and_source_id_and_external_exports,
    analyze_with_host, analyze_with_host_and_source_id_and_external_exports,
};
pub use host_type_resolution::{
    HostTypeResolutionError, HostTypeResolutionResults, HostTypeResolutionView, resolve_host_types,
};
pub use numeric_literals::{NumericLiteralError, concretize_numeric_literal};
pub use project_analysis::{
    analyze_project_with_host, analyze_project_with_host_and_external_exports,
    analyze_project_with_host_declarations,
};
pub use rils_builtins::{
    BuiltinId, FLOAT_INTRINSICS, INTEGER_INTRINSICS, IntrinsicDeclaration, IntrinsicKind,
    TypePattern, float_constant, float_method, integer_associated_function, integer_constant,
    integer_method,
};
pub use rils_syntax::{
    BodyId, DefId, ExprId, FloatType, FunctionSignature, ImplId, IntegerType, ModuleId, PatternId,
    RuntimeValue, SourceFile, SourceId, Span, SymbolId, Type, TypeRefId,
};
pub use rils_syntax::{
    LexError, ParseCapabilities, ParseError, lex, lex_with_source_id, parse,
    parse_with_capabilities,
};
pub use semantic::{
    BuiltinCallKind, DefMap, DefinitionData, ResolvedCall, SymbolContainer, SymbolKind,
    TypeckResults,
};
