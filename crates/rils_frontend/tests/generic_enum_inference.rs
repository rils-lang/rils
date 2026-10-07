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

#[test]
fn annotations_reach_nested_constructor_expressions_for_every_consumer() {
    let source = include_str!("fixtures/generic_enum_inference/contextual.rils");
    let program = parse(lex(source).unwrap()).unwrap();
    let analysis = analyze_program(&program);
    for statement in &program.statements {
        let Stmt::Let {
            type_annotation: Some(expected),
            initializer: value,
            ..
        } = statement
        else {
            continue;
        };
        let ids = analysis.typeck_results.expression_ids_at(value.span());
        assert!(
            ids.iter()
                .any(|id| analysis.typeck_results.expression_type(*id) == Some(expected)),
            "{expected}: {source}"
        );
        if let rils_frontend::ast::Expr::RecordLiteral { fields, .. } = value
            && let rils_frontend::ast::Expr::Call { arguments, .. } = &fields[0].value
        {
            let ids = analysis
                .typeck_results
                .expression_ids_at(arguments[0].span());
            let expected = Type::Named {
                name: "Choice".into(),
                arguments: vec![Type::I32],
            };
            assert!(
                ids.iter()
                    .any(|id| analysis.typeck_results.expression_type(*id) == Some(&expected))
            );
        }
    }
}
