use rils::{BytecodeModule, Value, compile, eval};

fn assert_native_string(value: Value, expected: &str) {
    assert_eq!(value.as_string().as_deref(), Some(expected));
    let Value::Native(object) = value else {
        panic!("string result must retain native storage");
    };
    assert_eq!(object.descriptor().rils_type().to_string(), "string");
}

#[test]
fn native_string_matches_in_interpreter_vm_and_loaded_bytecode() {
    for (source, expected) in [
        ("\"héllo\"", "héllo"),
        ("\"a\" + \"b\"", "ab"),
        (include_str!("fixtures/native_string.rils"), "HÉLLO!"),
    ] {
        assert_native_string(eval(source).unwrap(), expected);
        let compiled = compile(source).unwrap();
        assert_native_string(compiled.execute().unwrap(), expected);
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        assert_native_string(loaded.execute().unwrap(), expected);
    }
}
