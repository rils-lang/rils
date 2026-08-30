use std::collections::{HashMap, HashSet};

use super::*;
use crate::{SourceId, Type, semantic::TypeIdentityMap};

#[test]
fn resolves_types_expressions_and_patterns_without_modifying_syntax() {
    let source = SourceId::new(21);
    let tokens = crate::lexer::lex_with_source_id(
        "use unity_engine::GameObject as Go; \
         fn inspect(value: Go) { \
             let nested: Option<Go> = None; \
             Go::create(); \
             match Go::Kind { Go::Kind => () } \
         }",
        source,
    )
    .expect("lex host paths");
    let program = crate::parser::parse(tokens).expect("parse host paths");
    let host_types = HashSet::from(["unity_engine::GameObject".to_owned()]);
    let results = resolve_host_types(&program, SourceId::UNKNOWN, &host_types);
    let type_ids = TypeIdentityMap::allocate(&program, SourceId::UNKNOWN);
    let expression_ids =
        crate::semantic::ExpressionIdentityMap::allocate(&program, SourceId::UNKNOWN);
    let pattern_ids = crate::semantic::PatternIdentityMap::allocate(&program, SourceId::UNKNOWN);

    let crate::ast::Stmt::Function {
        parameters, body, ..
    } = &program.statements[1]
    else {
        panic!("expected function");
    };
    let parameter_type = parameters[0]
        .type_annotation
        .as_ref()
        .expect("parameter type");
    let crate::ast::Stmt::Let {
        type_annotation: Some(Type::Option(nested_type)),
        ..
    } = &body.statements[0]
    else {
        panic!("expected nested type");
    };
    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Call { callee, .. },
        ..
    } = &body.statements[1]
    else {
        panic!("expected associated call");
    };
    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Match { arms, .. },
        ..
    } = &body.statements[2]
    else {
        panic!("expected match");
    };

    let parameter_id = type_ids.get(parameter_type).expect("parameter type id");
    let nested_id = type_ids.get(nested_type).expect("nested type id");
    let callee_id = expression_ids.get(callee).expect("callee expression id");
    let pattern_id = pattern_ids
        .get(&arms[0].pattern)
        .expect("variant pattern id");

    assert_eq!(
        results.type_name(parameter_id),
        Some("unity_engine::GameObject")
    );
    assert_eq!(
        results.type_name(nested_id),
        Some("unity_engine::GameObject")
    );
    assert_eq!(
        results
            .expression_path(callee_id)
            .map(|path| path.join("::")),
        Some("unity_engine::GameObject::create".to_owned())
    );
    assert_eq!(
        results.pattern_path(pattern_id).map(|path| path.join("::")),
        Some("unity_engine::GameObject::Kind".to_owned())
    );
    assert!(results.errors().is_empty());

    assert!(matches!(parameter_type, Type::Named { name, .. } if name == "Go"));
    assert!(matches!(nested_type.as_ref(), Type::Named { name, .. } if name == "Go"));
    assert!(
        matches!(callee.as_ref(), crate::ast::Expr::Path { segments, .. } if segments == &["Go", "create"])
    );
    assert!(
        matches!(&arms[0].pattern, crate::ast::Pattern::Path { path, .. } if path == &["Go", "Kind"])
    );
}

#[test]
fn reports_glob_ambiguity_and_respects_local_type_shadowing() {
    let host_types = HashSet::from(["alpha::Object".to_owned(), "beta::Object".to_owned()]);
    let ambiguous = crate::parser::parse(
        crate::lexer::lex("use alpha::*; use beta::*; fn inspect(value: Object) {}")
            .expect("lex ambiguous type"),
    )
    .expect("parse ambiguous type");
    let results = resolve_host_types(&ambiguous, SourceId::UNKNOWN, &host_types);

    assert_eq!(results.errors().len(), 1);
    assert!(
        results.errors()[0]
            .message
            .contains("host type `Object` is ambiguous")
    );
    assert!(results.errors()[0].message.contains("alpha::Object"));
    assert!(results.errors()[0].message.contains("beta::Object"));

    let shadowed = crate::parser::parse(
        crate::lexer::lex("use alpha::*; struct Object {} fn inspect(value: Object) {}")
            .expect("lex shadowed type"),
    )
    .expect("parse shadowed type");
    let results = resolve_host_types(&shadowed, SourceId::UNKNOWN, &host_types);
    let type_ids = TypeIdentityMap::allocate(&shadowed, SourceId::UNKNOWN);
    let crate::ast::Stmt::Function { parameters, .. } = &shadowed.statements[2] else {
        panic!("expected function");
    };
    let parameter = parameters[0]
        .type_annotation
        .as_ref()
        .expect("parameter type");

    assert!(results.errors().is_empty());
    assert_eq!(results.type_name(type_ids.get(parameter).unwrap()), None);
}

#[test]
fn view_resolves_nested_types_and_paths_without_exposing_identity_maps() {
    let source = SourceId::new(22);
    let program = crate::parser::parse(
        crate::lexer::lex_with_source_id(
            "use unity_engine::GameObject as Go; \
             fn inspect(value: Option<Vec<Go>>) { \
                 Go::create(); \
                 match Go::Kind { Go::Kind => () } \
             }",
            source,
        )
        .expect("lex host paths"),
    )
    .expect("parse host paths");
    let host_types = HashSet::from(["unity_engine::GameObject".to_owned()]);
    let results = resolve_host_types(&program, SourceId::UNKNOWN, &host_types);
    let view = HostTypeResolutionView::new(&program, SourceId::UNKNOWN, &results);

    let crate::ast::Stmt::Function {
        parameters, body, ..
    } = &program.statements[1]
    else {
        panic!("expected function");
    };
    let resolved = view.resolved_type(
        parameters[0]
            .type_annotation
            .as_ref()
            .expect("parameter type"),
    );
    assert_eq!(
        resolved,
        Type::Option(Box::new(Type::Named {
            name: "Vec".to_owned(),
            arguments: vec![Type::named("unity_engine::GameObject")],
        }))
    );

    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Call { callee, .. },
        ..
    } = &body.statements[0]
    else {
        panic!("expected associated call");
    };
    assert_eq!(
        view.resolved_expression_path(callee)
            .map(|path| path.join("::")),
        Some("unity_engine::GameObject::create".to_owned())
    );

    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Match { arms, .. },
        ..
    } = &body.statements[1]
    else {
        panic!("expected match");
    };
    assert_eq!(
        view.resolved_pattern_path(&arms[0].pattern)
            .map(|path| path.join("::")),
        Some("unity_engine::GameObject::Kind".to_owned())
    );

    assert!(matches!(
        parameters[0].type_annotation.as_ref(),
        Some(Type::Option(inner))
            if matches!(inner.as_ref(), Type::Named { name, arguments }
                if name == "Vec"
                    && matches!(arguments.as_slice(), [Type::Named { name, .. }] if name == "Go"))
    ));
}

#[test]
fn type_inference_consumes_aliases_from_the_read_only_view() {
    let host_type = "unity_engine::GameObject";
    let host_types = HashSet::from([host_type.to_owned()]);
    let host_functions = HashMap::from([(
        "unity_engine::game_object::create".to_owned(),
        crate::FunctionSignature::fixed(Vec::new(), Type::named(host_type)),
    )]);
    let analysis = crate::analysis::analyze_with_host_declarations(
        "use unity_engine::GameObject as Go; \
         fn pass(value: Option<Go>) -> Option<Go> { value } \
         let created = Go::create();",
        &host_functions,
        &host_types,
    )
    .expect("analyze imported host type");

    assert!(
        analysis.diagnostics.is_empty(),
        "{:#?}",
        analysis.diagnostics
    );
    assert!(
        analysis
            .inlay_hints
            .iter()
            .any(|hint| { hint.label == ": unity_engine::GameObject" })
    );
    let pass = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "pass")
        .and_then(|symbol| symbol.inferred_type.as_ref())
        .expect("pass function type");
    assert_eq!(
        pass,
        &Type::function(
            vec![Type::Option(Box::new(Type::named(host_type)))],
            Type::Option(Box::new(Type::named(host_type))),
        )
    );
}
