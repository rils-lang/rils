use super::*;

#[test]
fn numeric_resolution_distinguishes_literals_with_the_same_span() {
    let tokens =
        crate::lexer::lex("let small: u8 = 1; let wide: u16 = 2;").expect("lex numeric bindings");
    let mut program = crate::parser::parse(tokens).expect("parse numeric bindings");
    let first_span = match &program.statements[0] {
        Stmt::Let { initializer, .. } => initializer.span(),
        _ => panic!("expected first binding"),
    };
    match &mut program.statements[1] {
        Stmt::Let {
            initializer: Expr::Literal { span, .. },
            ..
        } => *span = first_span,
        _ => panic!("expected second literal binding"),
    }

    resolve_numeric_literals(&mut program).expect("resolve literals by identity");

    let literals = program
        .statements
        .iter()
        .map(|statement| match statement {
            Stmt::Let {
                initializer: Expr::Literal { value, .. },
                ..
            } => value,
            _ => panic!("expected resolved literal binding"),
        })
        .collect::<Vec<_>>();
    assert!(matches!(literals[0], Literal::U8(1)));
    assert!(matches!(literals[1], Literal::U16(2)));
}
