use super::*;

#[test]
fn scans_keywords_numbers_and_comments() {
    let tokens = lex("let mut answer = 40 + 2.5; // ok").unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Let));
    assert!(matches!(tokens[1].kind, TokenKind::Mut));
    assert!(matches!(tokens[2].kind, TokenKind::Identifier(_)));
    assert!(matches!(tokens[4].kind, TokenKind::Integer(40)));
    assert!(matches!(tokens[6].kind, TokenKind::Float(value) if value == 2.5));
}

#[test]
fn scans_multi_character_arrows_and_comparisons() {
    let tokens = lex("-> <= => >= == !=").unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Arrow));
    assert!(matches!(tokens[1].kind, TokenKind::LessEqual));
    assert!(matches!(tokens[2].kind, TokenKind::FatArrow));
    assert!(matches!(tokens[3].kind, TokenKind::GreaterEqual));
    assert!(matches!(tokens[4].kind, TokenKind::EqualEqual));
    assert!(matches!(tokens[5].kind, TokenKind::BangEqual));
}

#[test]
fn scans_macro_keyword_and_metavariables() {
    let tokens = lex("macro twice($value) { $value + $value }").unwrap();
    assert!(matches!(tokens[0].kind, TokenKind::Macro));
    assert!(matches!(tokens[3].kind, TokenKind::Dollar));
    assert!(matches!(tokens[4].kind, TokenKind::Identifier(_)));
}
