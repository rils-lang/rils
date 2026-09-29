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
fn vec_deque_iterates_owned_user_records_in_queue_order() {
    let source = r#"
        struct Item { value: i32 }
        let mut queue: VecDeque<Item> = VecDeque::new();
        queue.push_back(Item { value: 20 });
        queue.push_front(Item { value: 22 });
        let mut iterator = queue.into_iter();
        let first = iterator.next().unwrap();
        let second = iterator.next().unwrap();
        first.value - second.value
    "#;
    assert_both(source, Value::from_i32(2));
}

#[test]
fn binary_heap_supports_owned_for_iteration() {
    let source = r#"
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        heap.push(20);
        heap.push(22);
        let mut sum = 0;
        for value in heap { sum = sum + value; }
        sum
    "#;
    assert_both(source, Value::from_i32(42));
}
