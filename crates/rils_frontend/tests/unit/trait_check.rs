use crate::{analysis::analyze_program, lexer::lex, parser::parse};

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
