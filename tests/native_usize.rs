use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_native_usize(value: Value, expected: usize) {
    assert_eq!(value.as_usize(), Some(expected));
    let Value::Native(object) = value else {
        panic!("usize result must retain native storage");
    };
    assert!(object.is_inline());
    assert_eq!(object.descriptor().rils_type().to_string(), "usize");
}

#[test]
fn native_usize_matches_in_interpreter_vm_and_loaded_bytecode() {
    for (source, expected) in [
        ("1usize", 1),
        ("1usize + 2usize", 3),
        ("1usize.saturating_sub(2usize)", 0),
        ("\"hello\".len()", 5),
        (include_str!("fixtures/native_usize.rils"), 2),
    ] {
        assert_native_usize(eval_value(source).unwrap(), expected);
        let compiled = compile(source).unwrap();
        assert_native_usize(compiled.execute_value().unwrap(), expected);
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_native_usize(loaded.execute_value().unwrap(), expected);
    }
}
