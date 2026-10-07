use rils_runtime::{Engine, Value};

#[test]
fn constructors_keep_definition_types_across_eval_calls_and_returned_closures() {
    let mut engine = Engine::new();
    engine
        .eval_value(include_str!(
            "fixtures/native_constructor_context/definitions.rils"
        ))
        .unwrap();
    let result = engine
        .eval_value(include_str!(
            "fixtures/native_constructor_context/invoke.rils"
        ))
        .unwrap();
    assert_eq!(result, Value::from_i32(42));
}

#[test]
fn result_constructors_keep_definition_types_across_eval_calls_and_returned_closures() {
    let mut engine = Engine::new();
    engine
        .eval_value(include_str!(
            "fixtures/native_result_context/definitions.rils"
        ))
        .unwrap();
    let value = engine
        .eval_value(include_str!("fixtures/native_result_context/invoke.rils"))
        .unwrap();
    assert_eq!(value, Value::from_i32(42));
}
