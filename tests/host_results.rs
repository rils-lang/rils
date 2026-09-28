use rils::{Engine, compile, eval};

#[test]
fn interpreter_and_bytecode_return_typed_handles() {
    assert_eq!(eval("40 + 2").unwrap().get_cloned::<i32>().unwrap(), 42);
    let compiled = compile("40 + 2").unwrap();
    assert_eq!(
        compiled.execute().unwrap().into_owned::<i32>().ok(),
        Some(42)
    );
}

#[test]
fn result_handle_borrows_or_moves_string_explicitly() {
    let result = eval("\"hello\"").unwrap();
    assert_eq!(result.with_ref::<String, _>(|text| text.len()).unwrap(), 5);
    assert_eq!(result.get_cloned::<String>().unwrap(), "hello");
    assert_eq!(result.into_owned::<String>().ok().as_deref(), Some("hello"));
}

#[test]
fn returned_script_reference_cannot_be_moved() {
    let mut engine = Engine::new();
    engine.eval("let value = 7;").unwrap();
    let result = engine.eval("&value").unwrap();
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 7);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
    assert!(result.into_owned::<i32>().is_err());
}
