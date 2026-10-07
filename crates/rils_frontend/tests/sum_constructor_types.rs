use rils_frontend::{
    Type,
    analysis::analyze_program,
    ast::{Expr, Stmt},
    lex, parse,
};

#[test]
fn expected_sum_types_are_recorded_on_both_outer_and_inner_constructors() {
    let source = include_str!("fixtures/sum_constructor_types.rils");
    let program = parse(lex(source).unwrap()).unwrap();
    let analysis = analyze_program(&program);
    for statement in &program.statements {
        let Stmt::Let {
            initializer,
            type_annotation: Some(expected),
            ..
        } = statement
        else {
            panic!("fixture contains annotated bindings");
        };
        let Expr::Call { arguments, .. } = initializer else {
            panic!("Some constructor")
        };
        let Type::Option(inner) = expected else {
            panic!("Option declaration")
        };
        for (expression, expected) in [(initializer, expected), (&arguments[0], inner.as_ref())] {
            assert!(
                analysis
                    .typeck_results
                    .expression_ids_at(expression.span())
                    .iter()
                    .any(|id| analysis.typeck_results.expression_type(*id) == Some(expected)),
                "constructor at {:?} must retain {expected}",
                expression.span(),
            );
        }
    }
}
