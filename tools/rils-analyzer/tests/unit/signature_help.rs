use super::*;

#[test]
fn constructor_signature_help_uses_shared_declarations_for_paths_and_aliases() {
    let text = include_str!("../fixtures/sum_constructor_signatures.rils");
    let program = rils_frontend::parse(rils_frontend::lex(text).unwrap()).unwrap();
    let analysis = rils_frontend::analysis::analyze_program(&program);
    for (call, name) in [
        ("core::result::Ok(", "Ok"),
        ("success(", "Ok"),
        ("failure(", "Err"),
        ("present(", "Some"),
    ] {
        let open = text.find(call).unwrap() + call.len() - 1;
        let (actual_name, signature) =
            semantic_signature_at_call(&analysis, rils_frontend::SourceId::UNKNOWN, text, open)
                .unwrap();
        assert_eq!(actual_name, name);
        assert_eq!(
            signature,
            rils_frontend::standard_library::standard_function_signature(name).unwrap()
        );
    }
}

#[test]
fn finds_nested_active_call_and_argument() {
    let text = "outer(1, inner(2, 3";
    assert_eq!(
        call_context(text, text.len()),
        Some(CallContext {
            open: 14,
            argument: 1
        })
    );
}

#[test]
fn ignores_commas_inside_nested_calls() {
    let text = "outer(inner(1, 2), 3";
    assert_eq!(
        call_context(text, text.len()),
        Some(CallContext {
            open: 5,
            argument: 1
        })
    );
}

#[test]
fn ignores_commas_inside_collection_arguments() {
    let text = "outer([1, 2], (3, 4), ";
    assert_eq!(
        call_context(text, text.len()),
        Some(CallContext {
            open: 5,
            argument: 2
        })
    );
}
