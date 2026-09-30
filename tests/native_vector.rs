use rils::{BytecodeModule, Type, Value, compile, eval_value};

fn run_both(source: &str) -> [Value; 3] {
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ]
}

#[test]
fn empty_vec_constructors_require_a_concrete_item_type() {
    for source in [
        "let values: Vec<i32> = Vec::new(); values",
        "let values = Vec::<i32>::new(); values",
    ] {
        for value in run_both(source) {
            assert!(matches!(value, Value::Dynamic(_)), "{source}");
        }
    }

    let source = "let values = Vec::new(); values";
    assert!(
        compile(source)
            .err()
            .expect("untyped constructor must fail")
            .to_string()
            .contains("cannot infer the type arguments of `Vec::new()`")
    );
    let interpreted_error = eval_value(source).unwrap_err().to_string();
    assert!(
        interpreted_error.contains("cannot infer the type arguments of `Vec::new()`"),
        "{interpreted_error}"
    );
}

#[test]
fn typed_copy_vec_uses_native_storage() {
    let source = "let mut values: Vec<i32> = Vec::new(); values.push(3); values.push(5); values";
    for value in run_both(source) {
        let Value::Dynamic(object) = &value else {
            panic!("typed Vec<i32> should be native")
        };
        assert_eq!(
            object.descriptor().layout().rils_type(),
            &Type::Named {
                name: "Vec".into(),
                arguments: vec![Type::I32],
            }
        );
        assert_eq!(value.to_string(), "[3, 5]");
        let items = value.as_vec().unwrap();
        assert_eq!(
            items.iter().map(Value::as_i32).collect::<Vec<_>>(),
            vec![Some(3), Some(5)]
        );
    }
}

#[test]
fn native_vec_from_moves_non_copy_array_items_across_backends() {
    let source = r#"
        let mut values: Vec<string> = Vec::from(["first", "second"]);
        values.pop().unwrap() == "second"
            && values.pop().unwrap() == "first"
            && values.is_empty()
    "#;
    for value in run_both(source) {
        assert_eq!(value, Value::Bool(true));
    }
}

#[test]
fn native_vec_index_references_allow_sequential_alias_writes() {
    let source = r#"
        let mut values: Vec<i32> = Vec::new();
        values.push(3);
        let first = &mut values[0];
        let second = &mut values[0];
        *first = 7;
        *second = 9;
        values[0]
    "#;
    for value in run_both(&format!("fn result() -> i32 {{ {source} }} result()")) {
        assert_eq!(value.as_i32(), Some(9));
    }
}

#[test]
fn native_vec_methods_and_borrowed_iteration_match_backends() {
    for (source, expected) in [
        (
            "let mut v: Vec<i32> = Vec::new(); v.push(3); v.insert(0usize, 7); v.remove(1usize)",
            3,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); v.push(3); v.push(7); v.swap_remove(0usize)",
            3,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); v.push(3); let mut it = v.iter(); let item = it.next().unwrap(); *item",
            3,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); v.push(3); let mut it = v.into_iter(); it.next().unwrap()",
            3,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); v.push(3); v.push(4); let mut total = 0; for item in v { total = total + item; } total",
            7,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); let mut more: Vec<i32> = Vec::new(); more.push(3); v.extend(more); v.pop().unwrap()",
            3,
        ),
        (
            "let mut v: Vec<i32> = Vec::new(); let mut more: Vec<i32> = Vec::new(); more.push(3); v.extend(more); v.pop().unwrap()",
            3,
        ),
    ] {
        for value in run_both(&format!("fn result() -> i32 {{ {source} }} result()")) {
            assert_eq!(value.as_i32(), Some(expected), "{source}");
        }
    }
}

#[test]
fn native_vec_rejects_growth_while_element_is_borrowed() {
    let source = "fn result() -> i32 { let mut v: Vec<i32> = Vec::new(); v.push(1); let item = &v[0]; v.push(2); *item } result()";
    let compiled = compile(source).unwrap();
    for error in [
        eval_value(source).unwrap_err().to_string(),
        compiled.execute_value().unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("structural") || error.contains("borrow"),
            "{error}"
        );
    }
}

#[test]
fn string_vec_uses_native_storage() {
    let source = "let mut v: Vec<string> = Vec::new(); v.push(\"hello\"); v";
    for value in run_both(source) {
        assert!(matches!(value, Value::Dynamic(_)));
        assert_eq!(
            value.as_vec().unwrap()[0].as_string().as_deref(),
            Some("hello")
        );
        let cloned = value.clone_owned().unwrap();
        assert_eq!(
            cloned.as_vec().unwrap()[0].as_string().as_deref(),
            Some("hello")
        );
    }
}

#[test]
fn native_string_vec_methods_and_borrows_match_backends() {
    for (source, expected) in [
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let item = &v[0]; item.len()",
            5usize,
        ),
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"old\"); { let item = &mut v[0]; *item = \"newer\"; } v.pop().unwrap().len()",
            5,
        ),
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut it = v.iter(); let item = it.next().unwrap(); item.len()",
            5,
        ),
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut it = v.into_iter(); it.next().unwrap().len()",
            5,
        ),
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut more: Vec<string> = Vec::new(); more.push(\"second\"); v.extend(more); v.pop().unwrap().len()",
            6,
        ),
        (
            "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut total = 0usize; for item in v { total = total + item.len(); } total",
            5,
        ),
    ] {
        for value in run_both(&format!("fn result() -> usize {{ {source} }} result()")) {
            assert_eq!(value.as_usize(), Some(expected), "{source}");
        }
    }
    for value in run_both(
        "let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let needle = \"first\"; v.contains(&needle)",
    ) {
        assert_eq!(value, Value::Bool(true));
    }
}

#[test]
fn native_string_vec_preserves_non_copy_indexing_rule() {
    let source = "let mut v: Vec<string> = Vec::new(); v.push(\"hello\"); v[0]";
    assert!(eval_value(source).is_err());
    assert!(compile(source).is_err() || compile(source).unwrap().execute_value().is_err());
}

#[test]
fn native_vec_extend_moves_nested_non_copy_elements() {
    let source = r#"
        let mut destination: Vec<Option<string>> = Vec::new();
        let mut source: Vec<Option<string>> = Vec::new();
        source.push(Some("moved"));
        destination.extend(source);
        destination.pop().unwrap().unwrap().len()
    "#;
    for value in run_both(source) {
        assert_eq!(value.as_usize(), Some(5));
    }
}

#[test]
fn native_vec_moves_boxed_user_values_into_push_and_insert() {
    for operation in ["push(Box::new(tail))", "insert(0usize, Box::new(tail))"] {
        let source = format!(
            r#"
                struct Node {{ value: i32, next: Option<Box<Node>> }}
                let tail: Node = Node {{ value: 42, next: None }};
                let mut values: Vec<Box<Node>> = Vec::new();
                values.{operation};
                let restored: Node = values.pop().unwrap().into_inner();
                restored.value
            "#
        );
        for value in run_both(&source) {
            assert_eq!(value.as_i32(), Some(42), "{operation}");
        }
    }
}

#[test]
fn native_vec_moves_user_records_into_push_and_insert() {
    for operation in ["push(tail)", "insert(0usize, tail)"] {
        let source = format!(
            r#"
                struct Node {{ value: i32, next: Option<Box<Node>> }}
                let tail: Node = Node {{ value: 42, next: None }};
                let mut values: Vec<Node> = Vec::new();
                values.{operation};
                let restored: Node = values.pop().unwrap();
                restored.value
            "#
        );
        for value in run_both(&source) {
            assert_eq!(value.as_i32(), Some(42), "{operation}");
        }
    }
}

#[test]
fn native_string_vec_rejects_growth_during_borrowed_iteration() {
    let source = "fn result() -> usize { let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut it = v.iter(); let item = it.next().unwrap(); v.push(\"second\"); item.len() } result()";
    let compiled = compile(source).unwrap();
    for error in [
        eval_value(source).unwrap_err().to_string(),
        compiled.execute_value().unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("structural") || error.contains("borrow"),
            "{error}"
        );
    }
}

#[test]
fn native_vec_can_be_borrowed_as_slice() {
    let source = "fn first(values: &[i32]) -> i32 { values[0] } let mut v: Vec<i32> = Vec::new(); v.push(5); first(&v)";
    for value in run_both(source) {
        assert_eq!(value.as_i32(), Some(5));
    }
}

#[test]
fn native_vec_copy_element_matrix() {
    for (ty, item) in [
        ("bool", "true"),
        ("char", "'x'"),
        ("i8", "3i8"),
        ("i32", "3"),
        ("i64", "3i64"),
        ("usize", "3usize"),
        ("f32", "3.0f32"),
        ("Option<i32>", "Some(3)"),
        ("(i32, bool)", "(3, true)"),
    ] {
        let source = format!("let mut v: Vec<{ty}> = Vec::new(); v.push({item}); v");
        for value in run_both(&source) {
            assert!(matches!(value, Value::Dynamic(_)), "{source}");
            assert_eq!(value.as_vec().unwrap().len(), 1, "{source}");
        }
    }
}

#[test]
fn nested_non_copy_element_uses_native_storage() {
    for (ty, item) in [
        ("Option<string>", "Some(\"text\")"),
        ("Result<string, string>", "Ok(\"text\")"),
        ("(string, i32)", "(\"text\", 3)"),
        ("Vec<string>", "Vec::new()"),
    ] {
        let source = format!("let mut v: Vec<{ty}> = Vec::new(); v.push({item}); v");
        for value in run_both(&source) {
            assert!(matches!(value, Value::Dynamic(_)), "{source}");
            assert_eq!(value.clone_owned().unwrap().to_string(), value.to_string());
        }
    }
}

#[test]
fn nested_non_copy_owned_operations_match_backends() {
    for (source, expected) in [
        (
            "let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); v.pop().unwrap().unwrap().len()",
            4usize,
        ),
        (
            "let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); v.remove(0usize).unwrap().len()",
            4usize,
        ),
        (
            "let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); let mut it = v.into_iter(); it.next().unwrap().unwrap().len()",
            4usize,
        ),
    ] {
        for value in run_both(source) {
            assert_eq!(value.as_usize(), Some(expected), "{source}");
        }
    }
}

#[test]
fn nested_non_copy_elements_can_be_referenced() {
    let source = "fn result() -> bool { let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); let mut it = v.iter(); it.next().is_some() } result()";
    for value in run_both(source) {
        assert_eq!(value, Value::Bool(true));
    }
}

#[test]
fn nested_non_copy_borrowed_reads_use_recursive_native_clone() {
    for source in [
        "fn result() -> bool { let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); let item = &v[0]; item.is_some() } result()",
        "fn result() -> bool { let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); let mut it = v.iter(); it.next().unwrap().is_some() } result()",
        "let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"text\")); let needle = Some(\"text\"); v.contains(&needle)",
    ] {
        for value in run_both(source) {
            assert_eq!(value, Value::Bool(true), "{source}");
        }
    }
}

#[test]
fn nested_non_copy_borrow_preserves_structural_mutation_rule() {
    let source = "fn result() -> usize { let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"first\")); let item = &v[0]; v.push(Some(\"second\")); 0usize } result()";
    let compiled = compile(source).unwrap();
    for error in [
        eval_value(source).unwrap_err().to_string(),
        compiled.execute_value().unwrap_err().to_string(),
    ] {
        assert!(
            error.contains("structural") || error.contains("borrow"),
            "{error}"
        );
    }
}

#[test]
fn nested_non_copy_borrow_can_replace_element() {
    let source = "fn result() -> usize { let mut v: Vec<Option<string>> = Vec::new(); v.push(Some(\"old\")); { let item = &mut v[0]; *item = Some(\"newer\"); } v.pop().unwrap().unwrap().len() } result()";
    for value in run_both(source) {
        assert_eq!(value.as_usize(), Some(5));
    }
}
