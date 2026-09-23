//! User-source parsing with derive handlers registered by the Rust standard library.

pub use rils_syntax::parser::{ParseCapabilities, ParseError, parse_builtin_declarations};
use rils_syntax::{ast::Program, macros::NativeMacroDefinition, token::Token};

pub fn parse(tokens: Vec<Token>) -> Result<Program, ParseError> {
    parse_with_capabilities(
        tokens,
        rils_syntax::macros::STANDARD_NATIVE_MACROS,
        ParseCapabilities::USER,
    )
}

pub fn parse_with_native_macros(
    tokens: Vec<Token>,
    native_macros: &[NativeMacroDefinition],
) -> Result<Program, ParseError> {
    parse_with_capabilities(tokens, native_macros, ParseCapabilities::USER)
}

pub fn parse_with_capabilities(
    tokens: Vec<Token>,
    native_macros: &[NativeMacroDefinition],
    capabilities: ParseCapabilities,
) -> Result<Program, ParseError> {
    rils_syntax::parser::parse_with_native_macros_and_derives(
        tokens,
        native_macros,
        rils_builtins::NATIVE_DERIVES,
        capabilities,
    )
}
