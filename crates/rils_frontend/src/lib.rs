pub mod analysis;
pub mod ast;
mod control_flow;
mod error;
pub mod lexer;
pub mod macros;
mod ownership;
pub mod parser;
mod resolution;
pub mod source;
pub mod standard_library;
mod static_type_check;
pub mod token;
mod type_inference;
pub mod types;

pub use error::FrontendError;
pub use lexer::{LexError, lex, lex_with_source_id};
pub use parser::{ParseError, parse};
pub use resolution::{
    NumericResolutionError, resolve_numeric_literals, resolve_numeric_literals_with_host_functions,
};
pub use rils_builtins::{
    FLOAT_INTRINSICS, INTEGER_INTRINSICS, IntrinsicDeclaration, IntrinsicId, IntrinsicKind,
    TypePattern, float_constant, float_method, integer_associated_function, integer_constant,
    integer_method,
};
pub use source::{SourceFile, SourceId, Span, SymbolId};
pub use types::{FloatType, FunctionSignature, IntegerType, RuntimeValue, Type};
