use rils_bytecode::compile;
use rils_runtime::eval;

#[test]
fn exported_combinators_invoke_rils_functions_and_captured_closures() {
    let source = include_str!("fixtures/native_callbacks.rils");
    let expected = eval(source).expect("interpreter accepts callback fixture");
    let module = compile(source).expect("bytecode accepts callback fixture");
    assert_eq!(module.execute().expect("VM invokes callbacks"), expected);
    let loaded = rils_bytecode::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute().expect("loaded VM invokes callbacks"),
        expected
    );
}

#[test]
fn callback_errors_preserve_their_source_span() {
    let source = "fn fail(value: i32) -> i32 { let absent: Option<i32> = None; absent.unwrap() } Some(1).map(fail)";
    let interpreted = eval(source).unwrap_err();
    let compiled = compile(source).unwrap().execute().unwrap_err();
    assert!(interpreted.to_string().contains("unwrap"));
    assert!(compiled.message.contains("unwrap"));
    assert_eq!(compiled.span, interpreted.span());
}

#[test]
fn callback_execution_counts_toward_the_vm_budget() {
    let source = "fn spin(value: i32) -> i32 { loop {} } Some(1).map(spin)";
    let error = compile(source).unwrap().execute_with_limit(64).unwrap_err();
    assert!(error.message.contains("step limit"));
}
