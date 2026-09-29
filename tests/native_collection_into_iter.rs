use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn concrete_collection_iterators_work_in_both_backends() {
    for source in [
        "let mut values: VecDeque<string> = VecDeque::new(); values.push_back(\"first\"); values.push_back(\"second\"); let mut items: core::collections::VecDequeIntoIter<string> = values.into_iter(); items.next().unwrap() == \"first\" && items.next().unwrap() == \"second\" && items.next().is_none()",
        "let mut values: BinaryHeap<i32> = BinaryHeap::new(); values.push(2); values.push(1); let mut items: core::collections::BinaryHeapIntoIter<i32> = values.into_iter(); let first = items.next().unwrap(); let second = items.next().unwrap(); first + second == 3 && items.next().is_none()",
        "let mut values: BTreeSet<i32> = BTreeSet::new(); values.insert(2); values.insert(1); let mut items: core::collections::BTreeSetIntoIter<i32> = values.into_iter(); items.next().unwrap() == 1 && items.next().unwrap() == 2 && items.next().is_none()",
        "let mut values: HashSet<string> = HashSet::new(); values.insert(\"one\"); let mut items: core::collections::HashSetIntoIter<string> = values.into_iter(); items.next().unwrap() == \"one\" && items.next().is_none()",
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::Bool(true), "{source}");
        let module = compile(source).unwrap();
        assert_eq!(
            module.execute_value().unwrap(),
            Value::Bool(true),
            "{source}"
        );
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        assert_eq!(
            loaded.execute_value().unwrap(),
            Value::Bool(true),
            "{source}"
        );
    }
}
