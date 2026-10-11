use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn integer_outputs_work_in_interpreter_vm_and_loaded_bytecode() {
    let source = include_str!("fixtures/native_integer_outputs.rils");
    assert_eq!(eval_value(source).unwrap(), Value::Bool(true));
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), Value::Bool(true));
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), Value::Bool(true));
}
