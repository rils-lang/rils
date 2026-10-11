use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn native_arrays_keep_indexing_iteration_and_conversion_across_backends() {
    let source = include_str!("fixtures/native_array_decode.rils");
    assert_eq!(eval_value(source).unwrap(), Value::Bool(true));
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), Value::Bool(true));
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), Value::Bool(true));
}

#[test]
fn native_array_indexing_rejects_noncopy_moves_and_out_of_bounds() {
    for (source, expected) in [
        (
            include_str!("fixtures/native_array_noncopy_index.rils"),
            "non-Copy",
        ),
        (
            include_str!("fixtures/native_array_bounds.rils"),
            "out of bounds",
        ),
    ] {
        let error = eval_value(source).unwrap_err().to_string();
        assert!(error.contains(expected), "{error}");
        match compile(source) {
            Ok(module) => {
                let error = module.execute_value().unwrap_err();
                assert!(error.message.contains(expected), "{error}");
                assert!(error.span.end > error.span.start);
                let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
                assert!(
                    loaded
                        .execute_value()
                        .unwrap_err()
                        .message
                        .contains(expected)
                );
            }
            Err(error) => assert!(error.message.contains(expected), "{error}"),
        }
    }
}
