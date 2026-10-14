use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn decoded_native_tuples_preserve_places_across_backends() {
    let source = include_str!("fixtures/native_tuple_decode.rils");
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
