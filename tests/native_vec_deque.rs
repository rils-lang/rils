use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn typed_vec_deque_uses_native_storage_in_both_backends() {
    let source = r#"
        let mut queue: VecDeque<i32> = VecDeque::new();
        queue.push_back(7);
        queue
    "#;
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        let Value::Dynamic(object) = value else {
            panic!("typed VecDeque should have native storage");
        };
        assert_eq!(
            object.with(|value| value.sequence_len()).unwrap().unwrap(),
            1
        );
    }
}

#[test]
fn native_vec_deque_can_own_another_native_vec_deque() {
    let source = r#"
        let mut outer: VecDeque<VecDeque<i32>> = VecDeque::new();
        let mut inner: VecDeque<i32> = VecDeque::new();
        inner.push_back(7);
        outer.push_back(inner);
        let mut recovered = outer.pop_front().unwrap();
        recovered.pop_front().unwrap()
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(7));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(7)
    );
}

#[test]
fn native_vec_deque_matches_in_interpreter_and_vm() {
    for source in [
        r#"
            let mut queue: VecDeque<i32> = VecDeque::new();
            queue.push_back(2);
            queue.push_front(1);
            queue.push_back(3);
            let len = queue.len();
            let front = queue.front_cloned().unwrap();
            let back = queue.back_cloned().unwrap();
            let first = queue.pop_front().unwrap();
            let last = queue.pop_back().unwrap();
            let middle = queue.pop_front().unwrap();
            if len == 3usize && queue.is_empty() && queue.pop_front() == None && queue.pop_back() == None {
                front + back + first + last + middle + 32
            } else { 0 }
        "#,
        r#"
            let mut queue: VecDeque<i32> = VecDeque::new();
            queue.push_front(1);
            queue.clear();
            if queue.is_empty() && queue.len() == 0usize { 42 } else { 0 }
        "#,
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap(),
            Value::from_i32(42)
        );
    }
}

#[test]
fn cloning_an_element_keeps_the_owned_string_in_the_queue() {
    let source = r#"
        let mut queue: VecDeque<string> = VecDeque::new();
        queue.push_back("hello");
        let copy = queue.front_cloned().unwrap();
        let original = queue.pop_front().unwrap();
        if copy == original && queue.is_empty() { 42 } else { 0 }
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
}

#[test]
fn cloning_a_noncopy_composite_element_keeps_native_queue_storage() {
    let source = r#"
        let mut queue: VecDeque<Option<string>> = VecDeque::new();
        queue.push_back(Some("hello"));
        let copy = queue.front_cloned().unwrap().unwrap();
        let original = queue.pop_front().unwrap().unwrap();
        if copy == original && queue.is_empty() { 42 } else { 0 }
    "#;
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value, Value::from_i32(42));
    }
}
