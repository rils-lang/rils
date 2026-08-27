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
fn validates_standard_format_macros() {
    let tokens = lexer::lex("println!(\"value = {} {:#?}\", value, state)").unwrap();
    expand(tokens, STANDARD_NATIVE_MACROS).expect("valid Rust-style format invocation");

    for source in [
        "println!(value)",
        "println!(\"{}\")",
        "println!(\"plain\", value)",
        "println!(\"{\")",
    ] {
        let error = expand(lexer::lex(source).unwrap(), STANDARD_NATIVE_MACROS).unwrap_err();
        assert!(!error.message.is_empty(), "{source}");
    }
}

#[test]
fn forwards_index_comparisons_without_recursive_matching() {
    let tokens = lexer::lex("assert!(values[0usize] == \"expected\")").unwrap();
    let expanded = expand(tokens, STANDARD_NATIVE_MACROS).unwrap();
    assert!(matches!(
        &expanded.tokens[0].kind,
        TokenKind::Identifier(name) if name == "#rils_native_assert"
    ));
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
