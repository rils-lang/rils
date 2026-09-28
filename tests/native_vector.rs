use rils::{BytecodeModule, Type, Value, compile, eval};

fn run_both(source: &str) -> [Value; 3] {
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    [
        eval(source).unwrap(),
        compiled.execute().unwrap(),
        loaded.execute().unwrap(),
    ]
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
            "let mut v: Vec<i32> = Vec::new(); let mut more = Vec::new(); more.push(3); v.extend(more); v.pop().unwrap()",
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
        eval(source).unwrap_err().to_string(),
        compiled.execute().unwrap_err().to_string(),
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
    assert!(eval(source).is_err());
    assert!(compile(source).is_err() || compile(source).unwrap().execute().is_err());
}

#[test]
fn native_string_vec_rejects_growth_during_borrowed_iteration() {
    let source = "fn result() -> usize { let mut v: Vec<string> = Vec::new(); v.push(\"first\"); let mut it = v.iter(); let item = it.next().unwrap(); v.push(\"second\"); item.len() } result()";
    let compiled = compile(source).unwrap();
    for error in [
        eval(source).unwrap_err().to_string(),
        compiled.execute().unwrap_err().to_string(),
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
    for source in [
        "let mut v: Vec<bool> = Vec::new(); v.push(true); v.pop().unwrap()",
        "let mut v: Vec<char> = Vec::new(); v.push('x'); v.pop().unwrap()",
        "let mut v: Vec<Option<i32>> = Vec::new(); v.push(Some(3)); let item = v.pop().unwrap(); item.unwrap()",
        "let mut v: Vec<(i32, bool)> = Vec::new(); v.push((3, true)); let item = v.pop().unwrap(); item.0",
    ] {
        let compiled = compile(source).unwrap();
        assert_eq!(
            eval(source).unwrap(),
            compiled.execute().unwrap(),
            "{source}"
        );
    }
}
