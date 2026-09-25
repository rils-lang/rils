use rils_bytecode::{BytecodeModule, compile};
use rils_runtime::eval;

#[test]
fn exported_free_functions_accept_stateful_and_multi_argument_callbacks() {
    let source = include_str!("fixtures/exported_callback_functions.rils");
    let expected = eval(source).expect("interpreter calls exported Rust function");
    let module = compile(source).expect("VM compiles exported Rust function");
    assert_eq!(module.execute().unwrap(), expected);
    let restored = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(restored.execute().unwrap(), expected);
}

#[test]
fn exported_free_function_callback_errors_keep_their_source_span() {
    let source = "fn fail(value: i32) -> i32 { let absent: Option<i32> = None; absent.unwrap() } core::ops::apply_twice(1, fail)";
    let interpreted = eval(source).unwrap_err();
    let compiled = compile(source).unwrap().execute().unwrap_err();
    assert!(interpreted.to_string().contains("unwrap"));
    assert!(compiled.message.contains("unwrap"));
    assert_eq!(compiled.span, interpreted.span());
}

#[test]
fn exported_free_function_callbacks_count_toward_vm_budget() {
    let source = "fn spin(value: i32) -> i32 { loop {} } core::ops::apply_twice(1, spin)";
    let error = compile(source).unwrap().execute_with_limit(64).unwrap_err();
    assert!(error.message.contains("step limit"));
}
