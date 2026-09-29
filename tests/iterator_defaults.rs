use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn trait_path_next_matches_interpreter_vm_and_loaded_bytecode() {
    for source in [
        include_str!("fixtures/trait_path_owned_iterator.rils"),
        include_str!("fixtures/trait_path_borrowed_iterator.rils"),
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
        let compiled = compile(source).unwrap();
        assert_eq!(compiled.execute_value().unwrap(), Value::from_i32(42));
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_eq!(loaded.execute_value().unwrap(), Value::from_i32(42));
    }
}

#[test]
fn user_iterator_trait_path_preserves_its_impl() {
    let source = include_str!("fixtures/trait_path_user_iterator.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
}

#[test]
fn same_name_trait_paths_select_the_requested_impl() {
    let source = include_str!("fixtures/trait_path_same_name.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
}

#[test]
fn overridden_iterator_default_matches_interpreter_and_vm() {
    let source = include_str!("fixtures/iterator_default_override.rils");
    let expected = Value::Usize(42);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
    let mismatched = source.replace("fn count(self) -> usize", "fn count(self) -> i32");
    assert!(compile(&mismatched).is_err());
}

#[test]
fn iterator_default_bodies_match_interpreter_and_vm() {
    let source = include_str!("fixtures/iterator_default_methods.rils");
    let expected = Value::from_i32(42);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}

#[test]
fn user_trait_default_bodies_apply_to_multiple_implementations() {
    let source = include_str!("fixtures/trait_default_methods.rils");
    let expected = Value::from_i32(16);
    assert_eq!(eval_value(source).unwrap(), expected);
    assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
}
