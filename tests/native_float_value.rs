use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn float_values_use_native_storage_in_both_backends() {
    for source in ["1.25f32 + 2.0f32", "(-1.25f32).abs() + 2.0f32"] {
        let interpreted = eval_value(source).unwrap();
        assert!(matches!(&interpreted, Value::Native(_)));
        assert_eq!(interpreted.get_cloned::<f32>().ok(), Some(3.25));
        let compiled = compile(source).unwrap();
        let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            compiled.execute_value().unwrap(),
            restored.execute_value().unwrap(),
        ] {
            assert!(matches!(&value, Value::Native(_)));
            assert_eq!(value.get_cloned::<f32>().ok(), Some(3.25));
        }
    }

    let source = "let value = 1.25f64; value + 2.0f64";
    let interpreted = eval_value(source).unwrap();
    assert!(matches!(&interpreted, Value::Native(_)));
    assert_eq!(interpreted.get_cloned::<f64>().ok(), Some(3.25));
    let compiled = compile(source).unwrap();
    let restored = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        compiled.execute_value().unwrap(),
        restored.execute_value().unwrap(),
    ] {
        assert!(matches!(&value, Value::Native(_)));
        assert_eq!(value.get_cloned::<f64>().ok(), Some(3.25));
    }
}
