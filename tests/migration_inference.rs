use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_backends(source: &str) {
    let expected = Value::from(rils::value::native_bool(true));
    assert_eq!(eval_value(source).unwrap(), expected);
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), expected);
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), expected);
}

#[test]
fn nested_reference_methods() {
    assert_backends(include_str!("fixtures/borrowed_native_members.rils"));
}

#[test]
fn inferred_range_clone() {
    assert_backends(include_str!("fixtures/inferred_range_clone.rils"));
}

#[test]
fn inferred_shared_handles() {
    assert_backends(include_str!("fixtures/inferred_shared_handles.rils"));
}
