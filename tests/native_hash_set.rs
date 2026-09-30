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
fn typed_hash_sets_use_native_storage() {
    for source in [
        "let values: HashSet<i32> = HashSet::new(); values",
        "let mut values: HashSet<string> = HashSet::new(); values.insert(\"first\"); values",
        "let mut values: HashSet<Option<i32>> = HashSet::new(); values.insert(Some(7)); values",
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
fn native_hash_set_handles_composite_keys_and_set_algebra() {
    assert_both(
        r#"
        let mut left: HashSet<Option<i32>> = HashSet::new();
        let mut right: HashSet<Option<i32>> = HashSet::new();
        left.insert(Some(7)); left.insert(None);
        right.insert(Some(7)); right.insert(Some(9));
        let shared = left.intersection(&right);
        let union = left.union(&right);
        let absent: Option<i32> = None;
        if left.contains(&absent) && shared.len() == 1usize && union.len() == 3usize { 42 } else { 0 }
        "#,
        Value::from_i32(42),
    );
    assert_both(
        "let mut values: HashSet<i32> = HashSet::new(); values.insert(40); values.insert(2); let mut total = 0; for value in values { total = total + value; } total",
        Value::from_i32(42),
    );
}

#[test]
fn hash_set_native_symbols_cover_algebra_and_mutation() {
    assert_both(
        r#"
        let mut left: HashSet<i32> = HashSet::new();
        let mut right: HashSet<i32> = HashSet::new();
        let empty: HashSet<i32> = HashSet::new();
        left.insert(1); left.insert(2);
        right.insert(2); right.insert(3);
        let union = left.union(&right);
        let intersection = left.intersection(&right);
        let difference = left.difference(&right);
        let symmetric = left.symmetric_difference(&right);
        let one = 1; let two = 2; let three = 3;
        let valid = left.is_subset(&union) && union.is_superset(&right)
            && left.is_disjoint(&empty) && union.len() == 3usize
            && intersection.contains(&two) && intersection.len() == 1usize
            && difference.contains(&one) && difference.len() == 1usize
            && symmetric.contains(&one) && symmetric.contains(&three)
            && symmetric.len() == 2usize;
        left.remove(&one);
        left.clear();
        valid && left.is_empty()
        "#,
        Value::Bool(true),
    );
}
