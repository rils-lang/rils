use rils::{Value, compile, eval};

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
        assert_eq!(eval(source).unwrap(), Value::I32(42));
        assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
    }
}
