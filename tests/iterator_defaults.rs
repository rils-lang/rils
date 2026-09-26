use rils::{Value, compile, eval};

#[test]
fn overridden_iterator_default_matches_interpreter_and_vm() {
    let source = include_str!("fixtures/iterator_default_override.rils");
    let expected = Value::Usize(42);
    assert_eq!(eval(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute().unwrap(), expected);
    let mismatched = source.replace("fn count(self) -> usize", "fn count(self) -> i32");
    assert!(compile(&mismatched).is_err());
}

#[test]
fn iterator_default_bodies_match_interpreter_and_vm() {
    let source = include_str!("fixtures/iterator_default_methods.rils");
    let expected = Value::I32(42);
    assert_eq!(eval(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute().unwrap(), expected);
}

#[test]
fn user_trait_default_bodies_apply_to_multiple_implementations() {
    let source = include_str!("fixtures/trait_default_methods.rils");
    let expected = Value::I32(16);
    assert_eq!(eval(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute().unwrap(), expected);
}
