use rils_frontend::{Type, analysis::analyze_program, ast::Stmt, lex, parse};

#[test]
fn later_member_arguments_refine_initializers_without_crossing_shadowed_bindings() {
    let source = include_str!("fixtures/collection_inference.rils");
    let program = parse(lex(source).unwrap()).unwrap();
    let analysis = analyze_program(&program);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    let Stmt::Let { initializer, .. } = &program.statements[0] else {
        panic!("first binding")
    };
    let expected = Type::Named {
        name: "Vec".into(),
        arguments: vec![Type::I32],
    };
    assert!(
        analysis
            .typeck_results
            .expression_ids_at(initializer.span())
            .iter()
            .any(|id| analysis.typeck_results.expression_type(*id) == Some(&expected))
    );
    let conflicting = source.replace("values.push(1i32)", "values.push(true)");
    let program = parse(lex(&conflicting).unwrap()).unwrap();
    assert!(!analyze_program(&program).diagnostics.is_empty());
}
