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
        parse(lex("pub trait Describe { fn describe(self) -> string; }").unwrap()).unwrap();
    let implementation_program = parse(
        lex(
            "use crate::api::Describe; struct State; impl Describe for State { fn describe(self) -> string { \"state\" } }",
        )
        .unwrap(),
    )
    .unwrap();
    let api = vec!["api".to_owned()];
    let app = vec!["app".to_owned()];
    let result = trait_check::analyze_project(&[
        (api.as_slice(), &trait_program),
        (app.as_slice(), &implementation_program),
    ]);

    assert!(result.diagnostics.is_empty());
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
