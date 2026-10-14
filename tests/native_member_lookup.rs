use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn projected_clone_borrows_its_source_and_preserves_custom_dispatch() {
    let source = include_str!("fixtures/projected_clone.rils");
    let expected = Value::from(rils::value::native_bool(true));
    assert_eq!(eval_value(source).unwrap(), expected);
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute_value().unwrap(), expected);
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), expected);
}

#[test]
fn projected_native_members_match_interpreter_vm_and_loaded_vm() {
    let source = include_str!("fixtures/projected_native_members.rils");
    let expected = Value::from(rils::value::native_bool(true));
    assert_eq!(eval_value(source).unwrap(), expected);
    let compiled = compile(source).unwrap();
    assert_eq!(compiled.execute_value().unwrap(), expected);
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), expected);
}

#[test]
fn nested_reference_members_are_resolved_without_reading_temporary_values() {
    let source = include_str!("fixtures/borrowed_native_members.rils");
    assert_eq!(
        eval_value(source).unwrap(),
        Value::from(rils::value::native_bool(true))
    );
}
