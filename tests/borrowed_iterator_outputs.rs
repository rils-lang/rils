use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn native_borrowed_outputs_match_interpreter_vm_and_loaded_bytecode() {
    let source = include_str!("fixtures/borrowed_iterator_outputs.rils");
    assert_eq!(
        eval_value(source).unwrap(),
        Value::from(rils::value::native_bool(true))
    );
    let module = compile(source).unwrap();
    assert_eq!(
        module.execute_value().unwrap(),
        Value::from(rils::value::native_bool(true))
    );
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute_value().unwrap(),
        Value::from(rils::value::native_bool(true))
    );
}
