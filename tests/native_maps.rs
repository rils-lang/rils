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
fn typed_maps_use_native_storage() {
    for source in [
        "let values: HashMap<i32, string> = HashMap::new(); values",
        "let values: BTreeMap<string, i32> = BTreeMap::new(); values",
        "let mut values: HashMap<i32, i32> = HashMap::new(); values.insert(1, 2); values",
        "let mut values: BTreeMap<i32, i32> = BTreeMap::new(); values.insert(1, 2); values",
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
fn native_map_and_set_size_queries_match_all_backends() {
    for source in [
        "let mut values: HashMap<i32, i32> = HashMap::new(); let empty = values.is_empty() && values.len() == 0usize; values.insert(1, 2); empty && !values.is_empty() && values.len() == 1usize",
        "let mut values: BTreeMap<i32, i32> = BTreeMap::new(); let empty = values.is_empty() && values.len() == 0usize; values.insert(1, 2); empty && !values.is_empty() && values.len() == 1usize",
        "let mut values: HashSet<i32> = HashSet::new(); let empty = values.is_empty() && values.len() == 0usize; values.insert(1); empty && !values.is_empty() && values.len() == 1usize",
        "let mut values: BTreeSet<i32> = BTreeSet::new(); let empty = values.is_empty() && values.len() == 0usize; values.insert(1); empty && !values.is_empty() && values.len() == 1usize",
    ] {
        assert_both(source, Value::Bool(true));
    }
}

#[test]
fn native_map_methods_preserve_values_and_borrows() {
    assert_both(
        r#"
        let mut map: HashMap<i32, string> = HashMap::new();
        map.insert(2, "second"); map.insert(1, "first");
        let old = map.insert(1, "new").unwrap();
        let key = 1;
        let current = map.get_cloned(&key).unwrap();
        let removed_key = 2;
        let removed = map.remove(&removed_key).unwrap();
        let mut sum = 0;
        for item in map.iter() { sum = sum + *item.0; }
        if old == "first" && current == "new" && removed == "second" && map.len() == 1usize { sum } else { 0 }
        "#,
        Value::from_i32(1),
    );
    assert_both(
        r#"
        let mut map: BTreeMap<i32, i32> = BTreeMap::new();
        map.insert(3, 30); map.insert(1, 10); map.insert(2, 20);
        let first = map.first_key_cloned().unwrap();
        let last = map.last_key_cloned().unwrap();
        let mut sum = 0;
        for item in map.iter() { sum = sum + *item.1; }
        let key = 2;
        if first == 1 && last == 3 && map.contains_key(&key) { sum } else { 0 }
        "#,
        Value::from_i32(60),
    );
}

#[test]
fn owned_map_iterators_move_entries_in_both_backends() {
    for (source, expected) in [
        (
            "let mut map: HashMap<i32, string> = HashMap::new(); map.insert(1, \"one\"); let mut entries: core::collections::HashMapIntoIter<i32, string> = map.into_iter(); let entry: (i32, string) = entries.next().unwrap(); entry.0 == 1 && entry.1 == \"one\" && entries.next().is_none()",
            true,
        ),
        (
            "let mut map: BTreeMap<i32, string> = BTreeMap::new(); map.insert(2, \"two\"); map.insert(1, \"one\"); let mut entries: core::collections::BTreeMapIntoIter<i32, string> = map.into_iter(); let first = entries.next().unwrap(); let second = entries.next().unwrap(); first.0 == 1 && first.1 == \"one\" && second.0 == 2 && second.1 == \"two\" && entries.next().is_none()",
            true,
        ),
    ] {
        assert_both(source, Value::Bool(expected));
    }
}
