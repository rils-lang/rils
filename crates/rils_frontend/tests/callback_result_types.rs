use rils_frontend::{Type, analysis::analyze_program, ast::Stmt, lex, parse};

#[test]
fn callback_signatures_determine_numeric_inputs_and_generic_results() {
    let program = parse(lex(include_str!("fixtures/callback_result_types.rils")).unwrap()).unwrap();
    let analysis = analyze_program(&program);
    assert!(
        analysis.diagnostics.is_empty(),
        "{:?}",
        analysis.diagnostics
    );
    for statement in &program.statements {
        let (expression, expected) = match statement {
            Stmt::Let {
                name, initializer, ..
            } if name == "combined_result" => (initializer, Type::I32),
            Stmt::Let {
                name, initializer, ..
            } if name == "numeric_result" => (initializer, Type::Option(Box::new(Type::USIZE))),
            Stmt::Let {
                name, initializer, ..
            } if name == "floating_result" => (initializer, Type::Option(Box::new(Type::I32))),
            Stmt::Let {
                name, initializer, ..
            } if name == "generic_result" => (
                initializer,
                Type::Result(Box::new(Type::I32), Box::new(Type::String)),
            ),
            Stmt::Function { name, body, .. } if name == "map_error" => {
                let Some(Stmt::Expr { expression, .. }) = body.statements.last() else {
                    panic!("function tail")
                };
                (
                    expression,
                    Type::Result(
                        Box::new(Type::Variable("T".into())),
                        Box::new(Type::Variable("E".into())),
                    ),
                )
            }
            _ => continue,
        };
        assert!(
            analysis
                .typeck_results
                .expression_ids_at(expression.span())
                .iter()
                .any(|id| analysis.typeck_results.expression_type(*id) == Some(&expected)),
            "expected {expected} at {:?}",
            expression.span()
        );
    }
}
