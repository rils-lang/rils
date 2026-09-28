use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn typed_binary_heap_uses_native_storage_in_both_backends() {
    let source = r#"
        let mut heap: BinaryHeap<i32> = BinaryHeap::new();
        heap.push(7);
        heap
    "#;
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for (backend, value) in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ]
    .into_iter()
    .enumerate()
    {
        let Value::Dynamic(object) = value else {
            panic!("typed BinaryHeap should have native storage in backend {backend}");
        };
        assert_eq!(
            object.with(|value| value.sequence_len()).unwrap().unwrap(),
            1
        );
    }
}

#[test]
fn native_binary_heap_orders_other_integer_widths() {
    for (source, expected) in [
        (
            r#"
                let mut heap: BinaryHeap<i8> = BinaryHeap::new();
                heap.push(1i8); heap.push(7i8); heap.push(3i8);
                heap.pop().unwrap()
            "#,
            Value::from_i8(7),
        ),
        (
            r#"
                let mut heap: BinaryHeap<usize> = BinaryHeap::new();
                heap.push(2usize); heap.push(11usize); heap.push(5usize);
                heap.pop().unwrap()
            "#,
            Value::from_usize(11),
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
    }
}

#[test]
fn native_binary_heap_matches_in_interpreter_and_vm() {
    for (source, expected) in [
        (
            r#"
                let mut heap: BinaryHeap<i32> = BinaryHeap::new();
                heap.push(2); heap.push(5); heap.push(1); heap.push(5);
                let highest = heap.peek_cloned().unwrap();
                let total = heap.pop().unwrap() + heap.pop().unwrap()
                    + heap.pop().unwrap() + heap.pop().unwrap();
                if heap.is_empty() && heap.len() == 0usize { highest + total } else { 0 }
            "#,
            Value::from_i32(18),
        ),
        (
            r#"
                let mut heap: BinaryHeap<char> = BinaryHeap::new();
                heap.push('a'); heap.push('z');
                heap.pop().unwrap()
            "#,
            Value::Char('z'),
        ),
        (
            r#"
                let mut heap: BinaryHeap<string> = BinaryHeap::new();
                heap.push("a"); heap.push("z");
                heap.pop().unwrap()
            "#,
            Value::from_string("z"),
        ),
        (
            r#"
                let mut heap: BinaryHeap<i32> = BinaryHeap::new();
                heap.push(1); heap.clear();
                heap.is_empty() && heap.pop().is_none()
            "#,
            Value::Bool(true),
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute_value().unwrap(), expected);
    }
}
