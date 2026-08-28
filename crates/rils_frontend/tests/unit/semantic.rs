use super::*;

#[test]
fn type_and_pattern_ids_cover_nested_expression_syntax() {
    let source = SourceId::new(11);
    let tokens = crate::lexer::lex_with_source_id(
        "enum Kind { Ready } \
         fn convert(value: Source) -> Option<Vec<Target>> { \
             let casted = value as i64; \
             <Target as Factory>::make; \
             match Kind::Ready { Kind::Ready => casted } \
         }",
        source,
    )
    .expect("lex type and pattern identities");
    let program = crate::parser::parse(tokens).expect("parse type and pattern identities");
    let type_ids = TypeIdentityMap::allocate(&program, SourceId::UNKNOWN);
    let pattern_ids = PatternIdentityMap::allocate(&program, SourceId::UNKNOWN);

    let crate::ast::Stmt::Function {
        parameters,
        return_type: Some(return_type),
        body,
        ..
    } = &program.statements[1]
    else {
        panic!("expected function declaration");
    };
    let parameter_type = parameters[0]
        .type_annotation
        .as_ref()
        .expect("parameter type");
    let crate::ast::Stmt::Let {
        initializer:
            crate::ast::Expr::Cast {
                target: cast,
                span: cast_span,
                ..
            },
        ..
    } = &body.statements[0]
    else {
        panic!("expected cast initializer");
    };
    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::QualifiedPath { target, .. },
        ..
    } = &body.statements[1]
    else {
        panic!("expected qualified path");
    };
    let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Match { arms, .. },
        ..
    } = &body.statements[2]
    else {
        panic!("expected match expression");
    };

    let parameter = type_ids.get(parameter_type).expect("parameter type id");
    let returned = type_ids.get(return_type).expect("return type id");
    let cast = type_ids.get(cast).expect("cast type id");
    let qualified = type_ids.get(target).expect("qualified path type id");
    let pattern = pattern_ids.get(&arms[0].pattern).expect("pattern id");

    assert_eq!(parameter.source, source);
    assert_eq!(parameter.local, 0);
    assert_eq!(returned.local, 1);
    assert_eq!(cast.local, 4);
    assert_eq!(qualified.local, 5);
    assert_eq!(pattern.source, source);
    assert_eq!(pattern.local, 0);
    assert_eq!(type_ids.span(cast), Some(*cast_span));
    assert_eq!(pattern_ids.span(pattern), Some(arms[0].pattern.span()));
}

#[test]
fn type_and_pattern_ids_preserve_same_span_nodes() {
    let source = SourceId::new(12);
    let tokens = crate::lexer::lex_with_source_id(
        "let first: Left = value; let second: Right = value; \
         match value { Left => 1, Right => 2 }",
        source,
    )
    .expect("lex shared spans");
    let mut program = crate::parser::parse(tokens).expect("parse shared spans");
    let first_type_span = match &program.statements[0] {
        crate::ast::Stmt::Let { span, .. } => *span,
        _ => panic!("expected first typed binding"),
    };
    let first_pattern_span = match &program.statements[2] {
        crate::ast::Stmt::Expr {
            expression: crate::ast::Expr::Match { arms, .. },
            ..
        } => arms[0].pattern.span(),
        _ => panic!("expected match expression"),
    };
    if let crate::ast::Stmt::Let { span, .. } = &mut program.statements[1] {
        *span = first_type_span;
    }
    if let crate::ast::Stmt::Expr {
        expression: crate::ast::Expr::Match { arms, .. },
        ..
    } = &mut program.statements[2]
    {
        match &mut arms[1].pattern {
            crate::ast::Pattern::Binding { span, .. } | crate::ast::Pattern::Path { span, .. } => {
                *span = first_pattern_span;
            }
            _ => panic!("expected path-like pattern"),
        }
    }

    let type_ids = TypeIdentityMap::allocate(&program, SourceId::UNKNOWN);
    let pattern_ids = PatternIdentityMap::allocate(&program, SourceId::UNKNOWN);

    assert_eq!(type_ids.ids_at(first_type_span).len(), 2);
    assert_ne!(
        type_ids.ids_at(first_type_span)[0],
        type_ids.ids_at(first_type_span)[1]
    );
    assert_eq!(pattern_ids.ids_at(first_pattern_span).len(), 2);
    assert_ne!(
        pattern_ids.ids_at(first_pattern_span)[0],
        pattern_ids.ids_at(first_pattern_span)[1]
    );
}

#[test]
fn expression_ids_are_deterministic_and_source_scoped() {
    let source = SourceId::new(1);
    let tokens = crate::lexer::lex_with_source_id("let first = 1; let second = true;", source)
        .expect("lex expressions");
    let program = crate::parser::parse(tokens).expect("parse expressions");
    let crate::ast::Stmt::Let {
        initializer: first, ..
    } = &program.statements[0]
    else {
        panic!("expected first binding");
    };
    let crate::ast::Stmt::Let {
        initializer: second,
        ..
    } = &program.statements[1]
    else {
        panic!("expected second binding");
    };
    let first = first.span();
    let second = second.span();
    let analysis = crate::analysis::analyze_program(&program);
    let results = &analysis.typeck_results;
    let first = results.expression_ids_at(first)[0];
    let second = results.expression_ids_at(second)[0];
    assert_eq!(first.local, 0);
    assert_eq!(second.local, 1);
    assert_eq!(results.expression_type(second), Some(&Type::Bool));
}

#[test]
fn expressions_with_the_same_span_keep_distinct_identities() {
    let source = SourceId::new(7);
    let span = Span::in_source(source, 4, 5);
    let first = crate::ast::Expr::Literal {
        value: crate::ast::Literal::Integer(1),
        span,
    };
    let second = crate::ast::Expr::Literal {
        value: crate::ast::Literal::Bool(true),
        span,
    };
    let program = crate::ast::Program {
        statements: vec![
            crate::ast::Stmt::Expr {
                expression: first,
                terminated: true,
            },
            crate::ast::Stmt::Expr {
                expression: second,
                terminated: true,
            },
        ],
        type_references: Vec::new(),
        macros: Vec::new(),
    };
    let analysis = crate::analysis::analyze_program(&program);
    let results = &analysis.typeck_results;
    let ids = results.expression_ids_at(span);

    assert_eq!(ids.len(), 2);
    assert_ne!(ids[0], ids[1]);
    assert_eq!(ids[0].local, 0);
    assert_eq!(ids[1].local, 1);
    assert_eq!(results.expression_span(ids[0]), Some(span));
    assert_eq!(results.expression_type(ids[0]), Some(&Type::I32));
    assert_eq!(results.expression_type(ids[1]), Some(&Type::Bool));
}

#[test]
fn calls_with_the_same_span_resolve_by_expression_identity() {
    let tokens =
        crate::lexer::lex("let value: Option<i32> = Some(1); value.is_some();").expect("lex call");
    let mut program = crate::parser::parse(tokens).expect("parse call");
    program.statements.push(program.statements[1].clone());
    let crate::ast::Stmt::Expr { expression, .. } = &program.statements[1] else {
        panic!("expected method call");
    };
    let span = expression.span();

    let analysis = crate::analysis::analyze_program(&program);
    let ids = analysis.typeck_results.expression_ids_at(span);

    assert_eq!(ids.len(), 2);
    for id in ids {
        assert!(matches!(
            analysis.typeck_results.resolved_call(*id),
            Some(ResolvedCall::Builtin {
                id: rils_builtins::BuiltinId::OptionIsSome,
                ..
            })
        ));
    }
}

#[test]
fn static_checkers_read_types_by_expression_identity() {
    let tokens = crate::lexer::lex("let number: i32 = 1; let flag: bool = true;")
        .expect("lex typed bindings");
    let mut program = crate::parser::parse(tokens).expect("parse typed bindings");
    let first_span = match &program.statements[0] {
        crate::ast::Stmt::Let { initializer, .. } => initializer.span(),
        _ => panic!("expected first binding"),
    };
    match &mut program.statements[1] {
        crate::ast::Stmt::Let {
            initializer: crate::ast::Expr::Literal { span, .. },
            ..
        } => *span = first_span,
        _ => panic!("expected second literal binding"),
    }

    let analysis = crate::analysis::analyze_program(&program);

    assert!(
        analysis.diagnostics.is_empty(),
        "identity-aware checks should preserve distinct types: {:?}",
        analysis.diagnostics
    );
}

#[test]
fn builtin_method_calls_resolve_to_semantic_ids() {
    let source = "fn increment(item: i32) -> i32 { item + 1 } \
         let value: Option<i32> = Some(1); value.is_some(); value.map(increment);";
    let analysis = crate::analysis::analyze(source).expect("analyze Option calls");

    let calls = analysis
        .typeck_results
        .resolved_calls
        .values()
        .collect::<Vec<_>>();
    assert!(calls.iter().any(|call| matches!(
        call,
        ResolvedCall::Builtin {
            id: rils_builtins::BuiltinId::OptionIsSome,
            kind: BuiltinCallKind::Runtime,
            ..
        }
    )));
    assert!(calls.iter().any(|call| matches!(
        call,
        ResolvedCall::Builtin {
            id: rils_builtins::BuiltinId::OptionMap,
            kind: BuiltinCallKind::Runtime,
            ..
        }
    )));
    let open = source.find("is_some(").expect("is_some call") + "is_some".len();
    assert!(matches!(
        analysis
            .typeck_results
            .resolved_call_containing(SourceId::UNKNOWN, open),
        Some((
            _,
            ResolvedCall::Builtin {
                id: rils_builtins::BuiltinId::OptionIsSome,
                ..
            }
        ))
    ));
}

#[test]
fn iterator_trait_methods_resolve_without_compiler_name_lookup() {
    let analysis = crate::analysis::analyze(
        "struct Counter { value: i32 } \
         impl Iterator for Counter { \
             type Item = i32; \
             fn next(&mut self) -> Option<i32> { None } \
         } \
         let counter = Counter { value: 0 }; counter.take(1usize);",
    )
    .expect("analyze custom iterator call");

    assert!(analysis.typeck_results.resolved_calls.values().any(|call| {
        matches!(
            call,
            ResolvedCall::Builtin {
                id: rils_builtins::BuiltinId::IteratorTake,
                kind: BuiltinCallKind::Runtime,
                ..
            }
        )
    }));
}

#[test]
fn def_map_resolves_occurrences_and_definitions_by_identity() {
    let analysis = crate::analysis::analyze("fn answer() -> i32 { 42 } let value = answer();")
        .expect("analyze function call");
    let reference = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.name == "answer" && !symbol.is_definition)
        .expect("function reference");
    let definition_id = analysis
        .def_map
        .resolution(reference.span)
        .expect("resolved definition identity");
    let definition = analysis
        .def_map
        .definition(definition_id)
        .expect("definition data");

    assert_eq!(definition.name, "answer");
    assert_eq!(definition.kind, SymbolKind::Function);
    assert!(matches!(
        definition.inferred_type,
        Some(Type::Function { .. })
    ));
    assert_eq!(
        analysis.def_map.definition_at(definition.span),
        Some(definition)
    );
}

#[test]
fn body_and_impl_ids_are_assigned_from_semantic_owners() {
    let tokens = crate::lexer::lex(
        "struct Counter { value: i32 } impl Counter { fn get(&self) -> i32 { self.value } }",
    )
    .expect("lex declarations");
    let program = crate::parser::parse(tokens).expect("parse declarations");
    let analysis = crate::analysis::analyze_program(&program);
    let method = analysis
        .symbols
        .iter()
        .find(|symbol| symbol.is_definition && symbol.name == "get")
        .expect("method definition");
    let definition = method.symbol_id.expect("method definition id");
    let Stmt::Impl { methods, span, .. } = &program.statements[1] else {
        panic!("expected impl declaration");
    };

    assert_eq!(analysis.def_map.body(definition), Some(BodyId(definition)));
    assert_eq!(
        analysis.def_map.body_at(methods[0].body.span),
        Some(BodyId(definition))
    );
    assert_eq!(
        analysis.def_map.impl_at(*span),
        Some(ImplId {
            source: span.source,
            local: 0,
        })
    );
}

#[test]
fn body_owners_do_not_depend_on_definition_span_lookup() {
    let shared_span = Span::in_source(SourceId::new(7), 10, 14);
    let first = DefId {
        source: shared_span.source,
        local: 1,
    };
    let second = DefId {
        source: shared_span.source,
        local: 2,
    };
    let symbols = [first, second].map(|id| crate::analysis::SymbolOccurrence {
        name: format!("function_{}", id.local),
        span: shared_span,
        definition_span: Some(shared_span),
        symbol_id: Some(id),
        definition_id: Some(id),
        kind: SymbolKind::Function,
        is_definition: true,
        inferred_type: None,
        detail: None,
        container: None,
    });
    let first_body = Span::in_source(shared_span.source, 20, 30);
    let second_body = Span::in_source(shared_span.source, 40, 50);
    let mut owners = SemanticOwnerIds::default();
    owners.record_body(first, first_body);
    owners.record_body(second, second_body);

    let map = DefMap::from_symbols_and_owners(&symbols, owners);

    assert_eq!(map.body(first), Some(BodyId(first)));
    assert_eq!(map.body(second), Some(BodyId(second)));
    assert_eq!(map.body_at(first_body), Some(BodyId(first)));
    assert_eq!(map.body_at(second_body), Some(BodyId(second)));
}
