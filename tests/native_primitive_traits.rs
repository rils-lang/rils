use rils::{Value, compile, eval};

#[test]
fn numeric_and_string_defaults_match_native_trait_registrations() {
    for (ty, zero) in [
        ("i8", "0i8"),
        ("i16", "0i16"),
        ("i32", "0i32"),
        ("i64", "0i64"),
        ("i128", "0i128"),
        ("isize", "0isize"),
        ("u8", "0u8"),
        ("u16", "0u16"),
        ("u32", "0u32"),
        ("u64", "0u64"),
        ("u128", "0u128"),
        ("usize", "0usize"),
        ("f32", "0.0f32"),
        ("f64", "0.0f64"),
        ("string", "\"\""),
    ] {
        let source = format!("<{} as Default>::default() == {zero}", ty);
        assert_eq!(eval(&source).unwrap(), Value::Bool(true), "{source}");
        assert_eq!(
            compile(&source).unwrap().execute().unwrap(),
            Value::Bool(true),
            "{source}"
        );
    }
}

#[test]
fn integer_and_string_keys_work_and_float_keys_are_rejected() {
    let source = r#"
        let mut integers: HashSet<i32> = HashSet::new();
        let mut strings: HashSet<string> = HashSet::new();
        integers.insert(42);
        strings.insert("answer");
        let integer = 42;
        let text = "answer";
        integers.contains(&integer) && strings.contains(&text)
    "#;
    assert_eq!(eval(source).unwrap(), Value::Bool(true));
    assert_eq!(
        compile(source).unwrap().execute().unwrap(),
        Value::Bool(true)
    );

    for ty in ["f32", "f64"] {
        let source = format!("let values: HashSet<{ty}> = HashSet::new();");
        assert!(compile(&source).is_err(), "{source}");
    }
}
