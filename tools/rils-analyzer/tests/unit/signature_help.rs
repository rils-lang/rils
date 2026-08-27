use super::*;

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
