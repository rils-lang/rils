use rils::{BytecodeModule, Value, compile, eval};

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
        eval(source).unwrap(),
        compiled.execute().unwrap(),
        loaded.execute().unwrap(),
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
        assert_eq!(eval(source).unwrap(), expected);
        assert_eq!(compile(source).unwrap().execute().unwrap(), expected);
    }
}
