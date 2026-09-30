use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn consuming_result_methods_move_non_copy_native_payloads() {
    for source in [
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<Vec<string>, string> = Ok(items);
            let mut recovered = value.unwrap(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<Vec<string>, string> = Ok(items);
            let mut recovered = value.unwrap_or(Vec::new()); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<Vec<string>, string> = Ok(items);
            let mut recovered = value.expect("missing"); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<i32, Vec<string>> = Err(items);
            let mut recovered = value.unwrap_err(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<i32, Vec<string>> = Err(items);
            let mut recovered = value.expect_err("missing"); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<Vec<string>, string> = Ok(items);
            let mut recovered = value.ok().unwrap(); recovered.pop().unwrap()"#,
        r#"let mut items: Vec<string> = Vec::new(); items.push("first");
            let value: Result<i32, Vec<string>> = Err(items);
            let mut recovered = value.err().unwrap(); recovered.pop().unwrap()"#,
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(value.as_string().as_deref(), Some("first"), "{source}");
        }
    }
}

#[test]
fn concrete_results_use_native_layout_in_both_backends() {
    for (source, expected, debug) in [
        (
            "let result: Result<i32, string> = Ok(7); result",
            "Ok(7)",
            "Ok(7)",
        ),
        (
            "let result: Result<i32, string> = Err(\"failed\"); result",
            "Err(failed)",
            "Err(\"failed\")",
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(value.to_string(), expected);
            assert_eq!(format!("{value:?}"), debug);
            assert!(value.as_result().is_some());
            let Value::Dynamic(object) = value else {
                panic!("concrete Result must have native storage");
            };
            assert_eq!(
                object.descriptor().layout().rils_type().to_string(),
                "Result<i32, string>"
            );
        }
    }
}

#[test]
fn native_results_keep_pattern_and_method_behavior() {
    for source in [
        "let result: Result<i32, string> = Ok(7); result.unwrap()",
        "let result: Result<i32, string> = Ok(7); match result { Ok(value) => value, Err(_) => 0 }",
        "let value: Result<i32, string> = Ok(7); let copy = value.clone(); copy.unwrap()",
        "fn inner() -> Result<i32, string> { let value: Result<i32, string> = Ok(7); Ok(value?) } inner().unwrap()",
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(7));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap(),
            Value::from_i32(7)
        );
    }
}

#[test]
fn concrete_result_error_paths_match_across_backends() {
    for source in [
        "let value: Result<i32, string> = Err(\"missing\"); value.unwrap_or(7)",
        "let value: Result<i32, string> = Err(\"missing\"); match value { Ok(number) => number, Err(_) => 7 }",
        "fn inner() -> Result<i32, string> { let value: Result<i32, string> = Err(\"missing\"); Ok(value?) } match inner() { Ok(_) => 0, Err(_) => 7 }",
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(7));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap(),
            Value::from_i32(7)
        );
    }
}

#[test]
fn copy_composite_result_payload_uses_native_layout() {
    let source = "let value: Result<Option<i32>, string> = Ok(Some(7)); value";
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value.to_string(), "Ok(Some(7))");
        let Value::Dynamic(object) = value else {
            panic!("Result<Option<i32>, string> must use native storage");
        };
        assert_eq!(
            object.descriptor().layout().rils_type().to_string(),
            "Result<Option<i32>, string>"
        );
    }
}

#[test]
fn noncopy_composite_result_payload_uses_native_layout() {
    for (source, expected) in [
        (
            "let value: Result<Option<string>, string> = Ok(Some(\"hello\")); value",
            "Ok(Some(hello))",
        ),
        (
            "let value: Result<Option<string>, string> = Err(\"failure\"); value",
            "Err(failure)",
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(value.to_string(), expected);
            assert!(matches!(value, Value::Dynamic(_)));
            assert_eq!(value.clone_owned().unwrap().to_string(), expected);
        }
    }
}
