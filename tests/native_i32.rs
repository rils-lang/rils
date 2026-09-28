use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_native_i32(value: Value, expected: i32) {
    assert_eq!(value.as_i32(), Some(expected));
    let Value::Native(object) = value else {
        panic!("i32 result must retain native storage");
    };
    assert!(object.is_inline());
    assert_eq!(object.descriptor().rils_type().to_string(), "i32");
}

#[test]
fn native_i32_matches_in_interpreter_vm_and_loaded_bytecode() {
    for (source, expected) in [
        ("1", 1),
        ("1 + 2", 3),
        ("2147483647i32.wrapping_add(1i32)", i32::MIN),
        (include_str!("fixtures/native_i32.rils"), 42),
    ] {
        assert_native_i32(eval_value(source).unwrap(), expected);
        let compiled = compile(source).unwrap();
        assert_native_i32(compiled.execute_value().unwrap(), expected);
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_native_i32(loaded.execute_value().unwrap(), expected);
    }
}
