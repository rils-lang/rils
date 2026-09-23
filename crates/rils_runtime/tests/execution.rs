use rils_runtime::{Engine, ExecutionLimits, Value, eval};

#[test]
fn evaluates_owned_values_and_explicit_clones() {
    let value = eval(
        r#"
        let original = "rils";
        let copied = clone(&original);
        original == copied
        "#,
    )
    .expect("valid ownership flow should execute");

    assert_eq!(value, Value::Bool(true));
}

#[test]
fn evaluates_rc_and_weak_handles() {
    let value = eval(
        r#"
        let handle: Rc<i32> = Rc::new(7);
        let weak = handle.downgrade();
        weak.upgrade().unwrap().strong_count()
        "#,
    )
    .expect("Rc and Weak should execute in the interpreter");
    assert!(matches!(value, Value::Usize(count) if count >= 1));
}

#[test]
fn evaluates_cell_interior_mutability() {
    let value = eval(
        r#"
        let cell: Cell<i32> = Cell::new(1);
        cell.set(2);
        cell.replace(3) + cell.get()
        "#,
    )
    .expect("Cell should execute in the interpreter");
    assert_eq!(value, Value::I32(5));
}

#[test]
fn evaluates_ref_cell_borrows() {
    let value = eval(
        r#"
        let cell: RefCell<i32> = RefCell::new(4);
        *cell.borrow() + cell.replace(5)
        "#,
    )
    .expect("RefCell should execute in the interpreter");
    assert_eq!(value, Value::I32(8));
}

#[test]
fn evaluates_vec_deque_operations() {
    let value = eval(
        r#"
        let mut queue: VecDeque<i32> = VecDeque::new();
        queue.push_back(2);
        queue.push_front(1);
        queue.pop_front().unwrap() + queue.pop_back().unwrap()
        "#,
    )
    .expect("VecDeque should execute in the interpreter");
    assert_eq!(value, Value::I32(3));
}

#[test]
fn evaluates_binary_heap_max_order_and_empty_cases() {
    for (source, expected) in [
        (
            r#"let mut heap: BinaryHeap<i32> = BinaryHeap::new();
               heap.push(2); heap.push(5); heap.push(1); heap.push(5);
               heap.peek_cloned().unwrap() + heap.pop().unwrap()
                 + heap.pop().unwrap() + heap.pop().unwrap() + heap.pop().unwrap()"#,
            Value::I32(18),
        ),
        (
            r#"let mut heap: BinaryHeap<string> = BinaryHeap::new();
               heap.push("a"); heap.push("z"); heap.pop().unwrap()"#,
            Value::String("z".into()),
        ),
        (
            r#"let mut heap: BinaryHeap<char> = BinaryHeap::new();
               heap.push('b'); heap.push('a'); heap.pop().unwrap()"#,
            Value::Char('b'),
        ),
        (
            r#"let mut heap: BinaryHeap<i32> = BinaryHeap::new();
               heap.push(1); heap.clear(); heap.is_empty() && heap.pop().is_none()"#,
            Value::Bool(true),
        ),
        (
            r#"let mut heap: std::collections::BinaryHeap<i32> = std::collections::BinaryHeap::new();
               heap.push(7); heap.len()"#,
            Value::Usize(1),
        ),
    ] {
        assert_eq!(eval(source).unwrap(), expected);
    }
}

#[test]
fn binary_heap_rejects_unorderable_values() {
    let error = eval(r#"let mut heap: BinaryHeap<bool> = BinaryHeap::new(); heap.push(true);"#)
        .unwrap_err();
    assert!(error.to_string().contains("does not support ordering"));
}

#[test]
fn evaluates_btree_map_ordered_operations() {
    let source = r#"
        let mut map: BTreeMap<i32, string> = BTreeMap::new();
        map.insert(3, "three");
        map.insert(1, "one");
        map.insert(2, "two");
        let first = map.first_key_cloned().unwrap();
        let last = map.last_key_cloned().unwrap();
        let key = 2;
        let found = map.get_cloned(&key).unwrap();
        let mut total = 0;
        for entry in map.into_iter() {
            total = total + entry.0;
        }
        if found == "two" { first + last + total } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(10));
    assert_eq!(
        eval(r#"let mut map = std::collections::BTreeMap::new(); map.insert("b", 2); map.insert("a", 1); map.first_key_cloned().unwrap()"#).unwrap(),
        Value::String("a".into())
    );
    assert_eq!(
        eval(r#"let mut map: BTreeMap<i32, i32> = BTreeMap::new(); map.insert(2, 20); map.insert(1, 10); let mut sum = 0; for entry in map { sum = sum + entry.0; } sum"#).unwrap(),
        Value::I32(3)
    );
}

#[test]
fn btree_map_handles_replacement_removal_and_invalid_keys() {
    let source = r#"
        let mut map: BTreeMap<char, i32> = BTreeMap::new();
        map.insert('b', 1);
        let previous = map.insert('b', 2).unwrap();
        let key = 'b';
        let removed = map.remove(&key).unwrap();
        if map.is_empty() { previous + removed } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(3));
    let error = eval(r#"let mut map: BTreeMap<f64, i32> = BTreeMap::new(); map.insert(1.5, 1);"#)
        .unwrap_err();
    assert!(error.to_string().contains("BTreeMap key must be"));
}

#[test]
fn evaluates_btree_set_order_and_algebra() {
    let source = r#"
        let mut left: BTreeSet<i32> = BTreeSet::new();
        let mut right: BTreeSet<i32> = BTreeSet::new();
        left.insert(3); left.insert(1); left.insert(2);
        right.insert(2); right.insert(4);
        let first = left.first_cloned().unwrap();
        let last = left.last_cloned().unwrap();
        let union = left.union(&right);
        let intersection = left.intersection(&right);
        let difference = left.difference(&right);
        let symmetric = left.symmetric_difference(&right);
        let mut order = 0;
        for value in left {
            order = order * 10 + value;
        }
        if union.len() == 4 && intersection.len() == 1
            && difference.len() == 2 && symmetric.len() == 3 {
            order + first + last
        } else { 0 }
    "#;
    assert_eq!(eval(source).unwrap(), Value::I32(127));
}

#[test]
fn btree_set_handles_boundaries_and_invalid_elements() {
    let source = r#"
        let mut set: BTreeSet<char> = BTreeSet::new();
        let empty = set.first_cloned().is_none() && set.last_cloned().is_none();
        let inserted = set.insert('b');
        let duplicate = set.insert('b');
        let key = 'b';
        let present = set.contains(&key);
        let removed = set.remove(&key);
        empty && inserted && !duplicate && present && removed && set.is_empty()
    "#;
    assert_eq!(eval(source).unwrap(), Value::Bool(true));
    let error =
        eval(r#"let mut set: BTreeSet<f64> = BTreeSet::new(); set.insert(1.5);"#).unwrap_err();
    assert!(error.to_string().contains("BTreeSet elements must be"));
}

#[test]
fn enforces_configured_execution_limits() {
    let mut engine = Engine::new();
    engine.set_execution_limits(ExecutionLimits::new(1_000, 8));

    let error = engine
        .eval(
            r#"
            fn recurse() {
                recurse()
            }
            recurse()
            "#,
        )
        .expect_err("unbounded recursion must exhaust the call-depth budget");

    let message = error.to_string();
    assert!(
        message.contains("frame limit"),
        "unexpected error: {message}"
    );
}

#[test]
fn supports_local_reference_containers_and_input_reference_returns() {
    let value = eval(
        r#"
        fn identity(value: &i32) -> Option<&i32> {
            Some(value)
        }
        fn run() -> i32 {
            let source = 41;
            let wrapped = identity(&source);
            let pair = (wrapped, [Some(&source)]);
            *pair.0.unwrap()
        }
        run()
        "#,
    )
    .expect("references may be carried by local generic containers");
    assert_eq!(value, Value::I32(41));
}

#[test]
fn rejects_local_reference_return_escape() {
    let error = eval(
        r#"
        fn invalid() -> &i32 {
            let value = 1;
            &value
        }
        invalid()
        "#,
    )
    .expect_err("a local reference must not escape its function");
    assert!(error.to_string().contains("cannot be returned"));
}

#[test]
fn generic_structs_can_carry_local_references() {
    let value = eval(
        r#"
        struct Wrapper<T> { value: T }
        fn run() -> i32 {
            let source = 7;
            let wrapped: Wrapper<&i32> = Wrapper { value: &source };
            *wrapped.value
        }
        run()
        "#,
    )
    .expect("generic struct instances may carry local references");
    assert_eq!(value, Value::I32(7));
}

#[test]
fn hash_maps_can_carry_reference_values_locally() {
    let value = eval(
        r#"
        fn run() -> i32 {
            let mut values: HashMap<string, &i32> = HashMap::new();
            let key = "answer";
            let source = 42;
            values.insert(key.clone(), &source);
            let found = values.get_cloned(&key);
            *found.unwrap()
        }
        run()
        "#,
    )
    .expect("HashMap values may carry local references");
    assert_eq!(value, Value::I32(42));
}
