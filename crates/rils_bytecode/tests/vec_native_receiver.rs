use rils_bytecode::{BytecodeModule, compile};
use rils_runtime::eval_value;

#[test]
fn vec_receiver_proxy_matches_interpreter_vm_and_loaded_bytecode() {
    let source = include_str!("fixtures/vec_native_receiver.rils");
    let interpreted = eval_value(source).expect("interpreter runs native Vec methods");
    assert_eq!(interpreted, rils_execution::Value::from_i32(42));
    let compiled = compile(source).expect("compiler accepts native Vec methods");
    assert_eq!(compiled.execute_value().unwrap(), interpreted);
    let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(restored.execute_value().unwrap(), interpreted);
}
