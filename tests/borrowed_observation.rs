use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn borrowed_literals_and_formatting_agree_in_all_backends() {
    let source = include_str!("fixtures/borrowed_observation.rils");
    let expected = Value::from(rils::value::native_bool(true));
    assert_eq!(eval_value(source).unwrap(), expected);
    let compiled = compile(source).unwrap();
    let mut host = rils::BytecodeHost::standard();
    host.enable_standard_io().unwrap();
    assert_eq!(compiled.execute_value_with_host(&host).unwrap(), expected);
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value_with_host(&host).unwrap(), expected);
}
