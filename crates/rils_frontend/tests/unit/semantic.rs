use super::*;

#[test]
fn expression_ids_are_deterministic_and_source_scoped() {
    let first = Span::in_source(SourceId::new(1), 4, 5);
    let second = Span::in_source(SourceId::new(1), 8, 12);
    let external = Span::in_source(SourceId::new(2), 1, 3);
    let types = HashMap::from([
        (second, Type::Bool),
        (external, Type::String),
        (first, Type::I32),
    ]);

    let results = TypeckResults::from_expression_types(&types);
    assert_eq!(results.expression_id(first).unwrap().local, 0);
    assert_eq!(results.expression_id(second).unwrap().local, 1);
    assert_eq!(results.expression_id(external).unwrap().local, 0);
    assert_eq!(results.expression_type_at(second), Some(&Type::Bool));
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
