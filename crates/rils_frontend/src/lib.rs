pub mod analysis;
mod control_flow;
pub mod database;
mod error;
mod format_check;
mod host_type_resolution;
mod ownership;
mod resolution;
pub mod semantic;
pub mod standard_library;
mod static_type_check;
mod trait_check;
mod type_inference;

pub use rils_syntax::{ast, default, format, lexer, macros, parser, source, token, types};

pub use database::{ModuleData, ModuleGraph, ProjectSemanticIndex, SourceDatabase};
pub use error::FrontendError;
pub use host_type_resolution::{HostTypeResolutionError, resolve_host_type_names};
pub use resolution::{
    NumericResolutionError, resolve_numeric_literals, resolve_numeric_literals_with_host_functions,
};
pub use rils_builtins::{
    BuiltinId, FLOAT_INTRINSICS, INTEGER_INTRINSICS, IntrinsicDeclaration, IntrinsicKind,
    TypePattern, float_constant, float_method, integer_associated_function, integer_constant,
    integer_method,
};
pub use rils_syntax::{
    BodyId, DefId, ExprId, FloatType, FunctionSignature, ImplId, IntegerType, ModuleId,
    RuntimeValue, SourceFile, SourceId, Span, SymbolId, Type,
};
pub use rils_syntax::{LexError, ParseError, lex, lex_with_source_id, parse};
pub use semantic::{
    BuiltinCallKind, DefMap, DefinitionData, ResolvedCall, SymbolContainer, SymbolKind,
    TypeckResults,
};
