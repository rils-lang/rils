use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_both(source: &str, expected: Value) {
    assert_eq!(
        eval_value(source).unwrap(),
        expected,
        "interpreter: {source}"
    );
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), expected, "VM: {source}");
    let restored = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        restored.execute_value().unwrap(),
        expected,
        "loaded VM: {source}"
    );
}

#[test]
fn native_collection_insert_moves_non_copy_values() {
    assert_both(
        "let mut values: HashSet<Option<string>> = HashSet::new(); values.insert(Some(\"text\")); values.len()",
        Value::from_usize(1),
    );
    assert_both(
        "let mut values: HashMap<i32, Box<string>> = HashMap::new(); values.insert(1, Box::new(\"text\")); values.len()",
        Value::from_usize(1),
    );
}
