use rils::{Value, compile, eval_value};

#[test]
fn arrays_keep_their_element_types_in_both_backends() {
    let source = include_str!("fixtures/exported_signature_types.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), Value::from_i32(42));
}

#[test]
fn exported_parameters_are_checked() {
    for source in [
        "Vec::from(42)",
        "Vec::from(\"text\")",
        "Vec::from(Some(1))",
        "let x: Vec<string> = Vec::from([1, 2]);",
        "std::io::write([1, 2])",
        "std::io::write_line(Some(1))",
        "fn emit<T: Display>(value: T) { std::io::write(value); } emit([1, 2]);",
        "let output = std::io::write; output([1, 2])",
        "struct Hidden { value: i32 } std::io::write(Hidden { value: 1 })",
    ] {
        assert!(compile(source).is_err(), "accepted: {source}");
        assert!(eval_value(source).is_err(), "accepted: {source}");
    }
}

#[test]
fn formatted_io_accepts_display_types() {
    for source in [
        "std::io::write(\"\")",
        "std::io::write(42)",
        "std::io::write(true)",
    ] {
        assert!(
            compile(source).is_ok(),
            "rejected: {source}: {:?}",
            compile(source).err()
        );
    }
}

#[test]
fn const_array_signatures_infer_lengths() {
    let source = include_str!("fixtures/const_array_signatures.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
    let invalid = source.replace("[22, 2]", "[22]");
    assert!(compile(&invalid).is_err());
    assert!(eval_value(&invalid).is_err());
}

#[test]
fn io_invokes_user_display_including_loaded_bytecode() {
    let source = include_str!("fixtures/io_custom_display.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    let module = compile(source).unwrap();
    let mut host = rils::BytecodeHost::standard();
    host.enable_standard_io().unwrap();
    assert_eq!(
        module.execute_value_with_host(&host).unwrap(),
        Value::from_i32(42)
    );
    let loaded = rils::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute_value_with_host(&host).unwrap(),
        Value::from_i32(42)
    );
    let failure = include_str!("fixtures/io_display_failure.rils");
    let error = eval_value(failure).unwrap_err().to_string();
    assert!(error.contains("display was invoked"), "{error}");
    let error = compile(failure)
        .unwrap()
        .execute_value_with_host(&host)
        .unwrap_err()
        .to_string();
    assert!(error.contains("display was invoked"), "{error}");
}
