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
    parser::{ParseError, is_expression_fragment},
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
    template: Vec<Token>,
    legacy_arity: Option<usize>,
}

#[derive(Clone, Debug)]
struct MacroTemplate {
    arms: Vec<MacroArm>,
}

struct CollectedMacros {
    tokens: Vec<Token>,
    definitions: HashMap<String, MacroTemplate>,
    symbols: Vec<MacroSymbol>,
}

#[derive(Clone, Debug, Default)]
struct Bindings {
    single: HashMap<String, Vec<Token>>,
    repeated: HashMap<String, Vec<Vec<Token>>>,
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
    let references = invocation_references(&tokens);
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

    let eof = tokens.last().cloned().ok_or_else(|| ParseError {
        message: "missing end-of-file token".into(),
        span: Span::new(0, 0),
    })?;
    let body = &tokens[..tokens.len().saturating_sub(1)];
    let mut stack = Vec::new();
    let mut expanded = expand_sequence(body, &definitions, &mut stack)?;
    expanded.push(eof);
    Ok(MacroExpansion {
        tokens: expanded,
        macros,
    })
}

#[cfg(test)]
mod tests {
    use crate::{lexer, token::TokenKind};

    use super::{STANDARD_NATIVE_MACROS, expand};

    #[test]
    fn keeps_legacy_function_like_macros_compatible() {
        let tokens = lexer::lex("twice!(21) macro twice($x) { $x + $x }").unwrap();
        let expanded = expand(tokens, &[]).unwrap();
        assert!(matches!(expanded.tokens[0].kind, TokenKind::Integer(21)));
        assert!(matches!(expanded.tokens[1].kind, TokenKind::Plus));
        assert!(matches!(expanded.tokens[2].kind, TokenKind::Integer(21)));
    }

    #[test]
    fn forwards_standard_native_macros_to_hidden_functions() {
        let tokens = lexer::lex("assert!(type_of(getter) == \"fn() -> i32\")").unwrap();
        let expanded = expand(tokens, STANDARD_NATIVE_MACROS).unwrap();
        assert!(matches!(
            &expanded.tokens[0].kind,
            TokenKind::Identifier(name) if name == "#rils_native_assert"
        ));
        assert_eq!(
            expanded
                .tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Bang))
                .count(),
            0
        );
    }

    #[test]
    fn selects_fragment_specific_branches() {
        for (invocation, expected) in [
            ("classify!(42)", "literal"),
            ("classify!(answer)", "identifier"),
            ("classify!(1 + 2)", "expression"),
        ] {
            let source = [
                r#"
            macro classify {
                ($value:lit) => { "literal" }
                ($name:ident) => { "identifier" }
                ($value:expr) => { "expression" }
            }
            "#,
                invocation,
            ]
            .concat();
            let expanded = expand(lexer::lex(&source).unwrap(), &[]).unwrap();
            assert!(matches!(
                &expanded.tokens[0].kind,
                TokenKind::String(value) if value == expected
            ));
        }
    }

    #[test]
    fn repeats_matches_and_expansions() {
        let tokens = lexer::lex(
            r#"
            macro emit { ($($value:expr),*) => { $(consume($value);)* } }
            emit!(1, 2, 3)
            "#,
        )
        .unwrap();
        let expanded = expand(tokens, &[]).unwrap();
        let calls = expanded
            .tokens
            .iter()
            .filter(|token| matches!(&token.kind, TokenKind::Identifier(name) if name == "consume"))
            .count();
        assert_eq!(calls, 3);
    }

    #[test]
    fn permits_bounded_recursive_expansion() {
        let tokens = lexer::lex(
            r#"
            macro count {
                () => { 0 }
                ($single:expr) => { 1 }
                ($head:expr, $($tail:expr),+) => { 1 + count!($($tail),+) }
            }
            count!(10, 20, 30)
            "#,
        )
        .unwrap();
        let expanded = expand(tokens, &[]).unwrap();
        assert_eq!(
            expanded
                .tokens
                .iter()
                .filter(|token| matches!(token.kind, TokenKind::Integer(1)))
                .count(),
            3
        );
    }
}
