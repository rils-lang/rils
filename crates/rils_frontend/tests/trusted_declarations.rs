use std::collections::{HashMap, HashSet};

use rils_frontend::{
    FunctionSignature, ParseCapabilities, ProjectSyntax, SourceId, Type,
    analysis::{analyze_program, analyze_program_with_host_declarations},
    ast::Program,
    lex_with_source_id,
    macros::STANDARD_NATIVE_MACROS,
    parse_with_capabilities,
};

fn program(source: &str, id: u32, capabilities: ParseCapabilities) -> Program {
    parse_with_capabilities(
        lex_with_source_id(source, SourceId::new(id)).unwrap(),
        STANDARD_NATIVE_MACROS,
        capabilities,
    )
    .unwrap()
}

#[test]
fn body_policy_is_separate_from_package_privileges() {
    let source = include_str!("fixtures/trusted_declarations/bodies.rils");
    for (capabilities, checks_bodies) in [
        (ParseCapabilities::USER, true),
        (ParseCapabilities::STANDARD_LIBRARY, false),
        (
            ParseCapabilities {
                declaration_only_bodies: false,
                ..ParseCapabilities::STANDARD_LIBRARY
            },
            true,
        ),
    ] {
        let analysis = analyze_program(&program(source, 1, capabilities));
        for name in ["missing_body_name", "missing_method_name"] {
            assert_eq!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(name)),
                checks_bodies
            );
        }
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.is_definition && symbol.name == "declared")
        );
        assert!(
            analysis
                .symbols
                .iter()
                .any(|symbol| symbol.is_definition && symbol.name == "value")
        );
        if !checks_bodies {
            assert!(
                analysis.diagnostics.is_empty(),
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[test]
fn trusted_signatures_and_duplicate_declarations_still_report_errors() {
    let analysis = analyze_program(&program(
        include_str!("fixtures/trusted_declarations/signatures.rils"),
        1,
        ParseCapabilities::STANDARD_LIBRARY,
    ));
    for expected in ["MissingType", "MissingResult", "already defined"] {
        assert!(
            analysis
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "{:?}",
            analysis.diagnostics
        );
    }
}

#[test]
fn only_trusted_declarations_can_replace_builtin_globals() {
    for source in [
        include_str!("fixtures/trusted_declarations/builtin.rils"),
        include_str!("fixtures/trusted_declarations/glob.rils"),
    ] {
        for (capabilities, duplicate) in [
            (ParseCapabilities::USER, true),
            (ParseCapabilities::STANDARD_LIBRARY, false),
        ] {
            let analysis = analyze_program(&program(source, 1, capabilities));
            assert_eq!(
                analysis
                    .diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains("already defined")),
                duplicate,
                "{:?}",
                analysis.diagnostics
            );
        }
    }
}

#[test]
fn package_privileges_do_not_replace_host_globals() {
    let analysis = analyze_program_with_host_declarations(
        &program(
            include_str!("fixtures/trusted_declarations/host.rils"),
            1,
            ParseCapabilities::STANDARD_LIBRARY,
        ),
        &HashMap::from([(
            "host_function".into(),
            FunctionSignature::fixed(Vec::new(), Type::Unit),
        )]),
        &HashSet::new(),
    );
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("already defined"))
    );
}

#[test]
fn merging_roots_preserves_trust_per_source() {
    let mut syntax = ProjectSyntax::default();
    syntax.push_root(program(
        include_str!("fixtures/trusted_declarations/host.rils"),
        1,
        ParseCapabilities::STANDARD_LIBRARY,
    ));
    syntax.push_root(program(
        include_str!("fixtures/trusted_declarations/builtin.rils"),
        2,
        ParseCapabilities::USER,
    ));
    let root = syntax.root_program();
    assert!(!root.language_declaration_spans.is_empty());
    let analysis = analyze_program(&root);
    assert!(
        analysis
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.span.source == SourceId::new(2)
                && diagnostic.message.contains("already defined"))
    );
}
