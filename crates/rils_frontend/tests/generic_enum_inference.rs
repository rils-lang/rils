use rils_frontend::{Type, analysis::analyze_program, ast::Stmt, lex, parse};

#[test]
fn enum_constructors_and_patterns_keep_concrete_type_arguments() {
    for (source, expected) in [
        (
            include_str!("fixtures/generic_enum_inference/nested.rils"),
            Type::Named {
                name: "Choice".into(),
                arguments: vec![Type::Option(Box::new(Type::I32))],
            },
        ),
        (
            include_str!("fixtures/generic_enum_inference/explicit.rils"),
            Type::Named {
                name: "Choice".into(),
                arguments: vec![Type::I32],
            },
        ),
        (
            include_str!("fixtures/generic_enum_inference/pattern.rils"),
            Type::I32,
        ),
        (
            include_str!("fixtures/generic_enum_inference/modules.rils"),
            Type::Named {
                name: "left::Choice".into(),
                arguments: vec![Type::Option(Box::new(Type::I32))],
            },
        ),
    ] {
        let program = parse(lex(source).unwrap()).unwrap();
        let analysis = analyze_program(&program);
        let Some(Stmt::Expr { expression, .. }) = program.statements.last() else {
            panic!("fixture expression");
        };
        let ids = analysis.typeck_results.expression_ids_at(expression.span());
        assert!(
            ids.iter()
                .any(|id| analysis.typeck_results.expression_type(*id) == Some(&expected)),
            "{source}"
        );
    }
}
