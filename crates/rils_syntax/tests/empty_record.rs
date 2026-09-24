use rils_syntax::{
    ast::{Expr, Stmt},
    lex, parse,
};

#[test]
fn parenthesized_empty_record_constructor_parses() {
    let program = parse(lex("struct Marker; let marker = (Marker {});").unwrap()).unwrap();
    assert!(matches!(
        &program.statements[1],
        Stmt::Let {
            initializer: Expr::RecordLiteral { fields, .. },
            ..
        } if fields.is_empty()
    ));
}

#[test]
fn empty_condition_blocks_still_parse_as_blocks() {
    parse(lex("let condition = true; if condition {}").unwrap()).unwrap();
    parse(lex("let condition = true; (if condition {})").unwrap()).unwrap();
}
