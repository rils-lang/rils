use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_both(source: &str, expected: Value) {
    assert_eq!(
        eval_value(source).unwrap(),
        expected,
        "interpreter: {source}"
    );
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), expected, "VM: {source}");
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute_value().unwrap(),
        expected,
        "loaded VM: {source}"
    );
}

#[test]
fn typed_btree_sets_use_native_storage_in_both_backends() {
    for source in [
        "let values: BTreeSet<i32> = BTreeSet::new(); values",
        "let mut values: BTreeSet<string> = BTreeSet::new(); values.insert(\"second\"); values.insert(\"first\"); values",
    ] {
        assert!(
            matches!(eval_value(source).unwrap(), Value::Dynamic(_)),
            "{source}"
        );
        assert!(
            matches!(
                compile(source).unwrap().execute_value().unwrap(),
                Value::Dynamic(_)
            ),
            "{source}"
        );
    }
}

#[test]
fn native_btree_set_operations_preserve_order_and_borrows() {
    assert_both(
        r#"
        let mut left: BTreeSet<i32> = BTreeSet::new();
        let mut right: BTreeSet<i32> = BTreeSet::new();
        left.insert(3); left.insert(1); left.insert(2);
        right.insert(2); right.insert(4);
        let union = left.union(&right);
        let intersection = left.intersection(&right);
        let first = left.first_cloned().unwrap();
        let last = left.last_cloned().unwrap();
        let mut sum = 0;
        for item in left.iter() { sum = sum + *item; }
        if union.len() == 4usize && intersection.len() == 1usize && first == 1 && last == 3 {
            sum + 36
        } else { 0 }
        "#,
        Value::from_i32(42),
    );
    assert_both(
        "let mut values: BTreeSet<char> = BTreeSet::new(); values.insert('b'); values.insert('a'); values.first_cloned().unwrap()",
        Value::from_char('a'),
    );
}
