use crate::{analysis::analyze_program, lexer::lex, parser::parse, trait_check};

#[test]
fn requires_supertraits_for_trait_implementations() {
    let missing = parse(
        lex("trait Behaviour: Default {} struct State; impl Behaviour for State {}").unwrap(),
    )
    .unwrap();
    let diagnostics = analyze_program(&missing).diagnostics;
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("must implement supertrait `Default`")
    }));

    let valid = parse(
        lex("trait Behaviour: Default {} #[derive(Default)] struct State; impl Behaviour for State {}")
            .unwrap(),
    )
    .unwrap();
    assert!(analyze_program(&valid).diagnostics.is_empty());
}

#[test]
fn records_impls_whose_method_contracts_were_checked() {
    let program = parse(
        lex(
            "trait Describe { fn describe(self) -> string; } struct State; impl Describe for State { fn describe(self) -> string { \"state\" } }",
        )
        .unwrap(),
    )
    .unwrap();

    let analysis = analyze_program(&program);
    assert!(analysis.diagnostics.is_empty());
    assert_eq!(analysis.verified_trait_impls.len(), 1);
}

#[test]
fn project_checks_imported_trait_impls() {
    let trait_program =
        parse(lex("pub trait Describe { type Output; fn describe(self) -> string; }").unwrap())
            .unwrap();
    let implementation_program = parse(
        lex(
            "use crate::api::Describe; struct State; impl Describe for State { type Output = string; fn describe(self) -> string { \"state\" } }",
        )
        .unwrap(),
    )
    .unwrap();
    let api = vec!["api".to_owned()];
    let app = vec!["app".to_owned()];
    let result = trait_check::analyze_project(
        &[
            (api.as_slice(), &trait_program),
            (app.as_slice(), &implementation_program),
        ],
        &Default::default(),
        &Default::default(),
    );

    assert!(result.diagnostics.is_empty());
    assert_eq!(result.verified_impls.len(), 1);
}

#[test]
fn validates_associated_type_contracts() {
    let cases = [
        (
            "trait Source { type Item; } struct State; impl Source for State {}",
            "missing associated type `Item`",
        ),
        (
            "trait Source {} struct State; impl Source for State { type Extra = i32; }",
            "associated type `Extra` is not a member of trait `Source`",
        ),
        (
            "trait Source { type Item<T>; } struct State; impl Source for State { type Item = i32; }",
            "associated type `Item` has the wrong number of generic parameters",
        ),
    ];

    for (source, expected) in cases {
        let program = parse(lex(source).unwrap()).unwrap();
        let result = trait_check::analyze(&program);
        assert!(
            result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains(expected)),
            "expected diagnostic containing `{expected}`"
        );
        assert!(result.verified_impls.is_empty());
    }

    let defaulted = parse(
        lex("trait Source { type Item = i32; } struct State; impl Source for State {}").unwrap(),
    )
    .unwrap();
    let result = trait_check::analyze(&defaulted);
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.verified_impls.len(), 1);
}

#[test]
fn rejects_conditional_trait_impl_bounds() {
    let source =
        "trait Tagged {} struct Wrapper<T> { value: T } impl<T: Clone> Tagged for Wrapper<T> {}";
    let conditional = parse(lex(source).unwrap()).unwrap();
    let result = trait_check::analyze(&conditional);
    let diagnostic = result
        .diagnostics
        .iter()
        .find(|diagnostic| {
            diagnostic
                .message
                .contains("conditional trait impl bounds are not supported yet")
        })
        .expect("conditional impl diagnostic");
    let parameter_start = source.find("T: Clone").unwrap();
    assert_eq!(diagnostic.span.start, parameter_start);
    assert_eq!(diagnostic.span.end, parameter_start + 1);
    assert!(result.verified_impls.is_empty());

    let unbounded = parse(
        lex("trait Tagged {} struct Wrapper<T> { value: T } impl<T> Tagged for Wrapper<T> {}")
            .unwrap(),
    )
    .unwrap();
    let result = trait_check::analyze(&unbounded);
    assert!(result.diagnostics.is_empty());
    assert_eq!(result.verified_impls.len(), 1);

    let inherent =
        parse(lex("struct Wrapper<T> { value: T } impl<T: Clone> Wrapper<T> {} ").unwrap())
            .unwrap();
    let result = trait_check::analyze(&inherent);
    assert!(result.diagnostics.is_empty());
}

#[test]
fn enforces_orphan_rules_without_rejecting_a_local_side() {
    let orphan = parse(lex("impl Clone for string {}").unwrap()).unwrap();
    let result = trait_check::analyze(&orphan);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| { diagnostic.message.contains("violates the orphan rule") })
    );

    for source in [
        "struct Local; impl Clone for Local {}",
        "trait LocalTrait {} impl LocalTrait for string {}",
    ] {
        let program = parse(lex(source).unwrap()).unwrap();
        let result = trait_check::analyze(&program);
        assert!(
            !result
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("violates the orphan rule")),
            "unexpected orphan diagnostic for `{source}`"
        );
    }

    let host_target = parse(lex("impl Clone for UnityObject {}").unwrap()).unwrap();
    let host_types = ["UnityObject".to_owned()].into_iter().collect();
    let result = trait_check::analyze_with_host_types(&host_target, &host_types);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("violates the orphan rule"))
    );
}

#[test]
fn rejects_duplicate_trait_implementations_by_identity() {
    let program = parse(
        lex("trait Tagged {} struct State; impl Tagged for State {} impl Tagged for State {}")
            .unwrap(),
    )
    .unwrap();
    let result = trait_check::analyze(&program);

    assert!(result.diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("trait `Tagged` is already implemented for `State`")
    }));
    assert_eq!(result.verified_impls.len(), 1);
}

#[test]
fn validates_trait_method_members_and_signatures() {
    let program = parse(
        lex(
            "trait Convert { fn convert(self, value: i32) -> i32; } struct State; impl Convert for State { fn convert(self, value: string) -> string { value } fn extra(self) {} }",
        )
        .unwrap(),
    )
    .unwrap();
    let diagnostics = analyze_program(&program).diagnostics;
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("method `convert` does not match its trait signature")
    }));
    assert!(diagnostics.iter().any(|diagnostic| {
        diagnostic
            .message
            .contains("method `extra` is not a member of trait `Convert`")
    }));

    let missing = parse(
        lex("trait Convert { fn convert(self, value: i32) -> i32; } struct State; impl Convert for State {}")
            .unwrap(),
    )
    .unwrap();
    assert!(
        analyze_program(&missing)
            .diagnostics
            .iter()
            .any(|diagnostic| {
                diagnostic
                    .message
                    .contains("impl of trait `Convert` is missing method `convert`")
            })
    );
}
