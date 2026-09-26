use rils::{BytecodeModule, Value, compile, eval};

fn assert_native_i8(value: Value, expected: i8) {
    let Value::Native(object) = value else {
        panic!("i8 result must retain native storage");
    };
    assert!(object.is_inline());
    assert_eq!(object.descriptor().rils_type().to_string(), "i8");
    assert_eq!(Value::Native(object).to_string(), expected.to_string());
}

#[test]
fn native_i8_matches_in_interpreter_vm_and_loaded_bytecode() {
    for (source, expected) in [
        ("1i8", 1),
        ("1i8 + 2i8", 3),
        ("127i8.wrapping_add(1i8)", -128),
        (include_str!("fixtures/native_i8.rils"), 126),
    ] {
        assert_native_i8(eval(source).unwrap(), expected);
        let compiled = compile(source).unwrap();
        assert_native_i8(compiled.execute().unwrap(), expected);
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_native_i8(loaded.execute().unwrap(), expected);
    }
}
