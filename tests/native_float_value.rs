use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn float_values_use_native_storage_in_both_backends() {
    for source in ["1.25f32 + 2.0f32", "(-1.25f32).abs() + 2.0f32"] {
        let interpreted = eval_value(source).unwrap();
        assert!(matches!(&interpreted, Value::Native(_)));
        assert_eq!(interpreted.as_f32(), Some(3.25));
        let compiled = compile(source).unwrap();
        let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            compiled.execute_value().unwrap(),
            restored.execute_value().unwrap(),
        ] {
            assert!(matches!(&value, Value::Native(_)));
            assert_eq!(value.as_f32(), Some(3.25));
        }
    }

    let source = "let value = 1.25f64; value + 2.0f64";
    let interpreted = eval_value(source).unwrap();
    assert!(matches!(&interpreted, Value::Native(_)));
    assert_eq!(interpreted.as_f64(), Some(3.25));
    let compiled = compile(source).unwrap();
    let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        compiled.execute_value().unwrap(),
        restored.execute_value().unwrap(),
    ] {
        assert!(matches!(&value, Value::Native(_)));
        assert_eq!(value.as_f64(), Some(3.25));
    }
}
