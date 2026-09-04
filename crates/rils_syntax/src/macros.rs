use std::collections::{HashMap, HashSet};

mod definition;
mod expansion;
mod matcher;
mod support;
mod template;

use definition::*;
use expansion::*;
use matcher::*;
use support::*;
use template::*;

use crate::{
    ast::MacroSymbol,
    cursor::TokenStream,
    parser::{ParseError, is_expression_fragment_stream},
    source::Span,
    token::{Token, TokenKind},
};

const MAX_EXPANSION_DEPTH: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeMacroDefinition {
    pub name: &'static str,
    pub target: &'static str,
}

pub const STANDARD_NATIVE_MACROS: &[NativeMacroDefinition] = &[
    NativeMacroDefinition {
        name: "print",
        target: "#rils_native_print",
    },
    NativeMacroDefinition {
        name: "println",
        target: "#rils_native_println",
    },
    NativeMacroDefinition {
        name: "assert",
        target: "#rils_native_assert",
    },
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FragmentKind {
    Expr,
    Lit,
    Ident,
    Tokens,
}

#[derive(Clone, Debug)]
enum MatcherElement {
    Token(TokenKind),
    Group {
        delimiter: crate::token_tree::Delimiter,
        elements: Vec<MatcherElement>,
    },
    Capture {
        name: String,
        kind: FragmentKind,
    },
    Repeat {
        elements: Vec<MatcherElement>,
        separator: Option<TokenKind>,
        one_or_more: bool,
    },
}

#[derive(Clone, Debug)]
struct MacroArm {
    matcher: Vec<MatcherElement>,
    template: TokenStream,
    legacy_arity: Option<usize>,
}

#[derive(Clone, Debug)]
struct MacroTemplate {
    arms: Vec<MacroArm>,
}

struct CollectedMacros {
    tokens: TokenStream,
    definitions: HashMap<String, MacroTemplate>,
    symbols: Vec<MacroSymbol>,
}

#[derive(Clone, Debug, Default)]
struct Bindings {
    single: HashMap<String, TokenStream>,
    repeated: HashMap<String, Vec<TokenStream>>,
}

#[derive(Debug)]
pub(crate) struct MacroExpansion {
    pub tokens: Vec<Token>,
    pub macros: Vec<MacroSymbol>,
}

pub(crate) fn expand(
    tokens: Vec<Token>,
    native_macros: &[NativeMacroDefinition],
) -> Result<MacroExpansion, ParseError> {
    let input_stream = TokenStream::new(tokens.clone()).map_err(|span| ParseError {
        message: "expected closing delimiter".into(),
        span,
    })?;
    let references = invocation_references(&input_stream);
    let CollectedMacros {
        tokens,
        definitions,
        symbols: mut macros,
    } = collect_definitions(tokens, native_macros)?;
    for definition in &mut macros {
        definition.references = references
            .get(&definition.name)
            .cloned()
            .unwrap_or_default();
    }

    let mut stack = Vec::new();
    let expanded = expand_sequence(&tokens, &definitions, &mut stack)?;
    Ok(MacroExpansion {
        tokens: expanded.to_tokens(),
        macros,
    })
}

#[cfg(test)]
#[path = "../tests/unit/macros.rs"]
mod tests;
