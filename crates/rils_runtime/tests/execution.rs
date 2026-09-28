use rils_runtime::{Engine, ExecutionLimits, Value, eval_value};

#[test]
fn rejects_manual_implementations_of_callable_traits() {
    let fixture = include_str!("fixtures/sealed_callable_impl.rils");
    for name in ["FnOnce", "FnMut", "Fn"] {
        for source in [
            fixture.replace("FnOnce", name),
            fixture.replace("FnOnce", &format!("core::ops::{name}")),
            format!(
                "use core::ops::{name} as Callback; {}",
                fixture.replace("FnOnce", "Callback")
            ),
        ] {
            let error =
                eval_value(&source).expect_err("callable traits are reserved for function values");
            assert!(error.to_string().contains("sealed"), "{name}: {error}");
        }
    }
}

#[test]
fn executes_a_generic_trait_implementation() {
    let value = eval_value(include_str!("fixtures/generic_trait.rils"))
        .expect("generic trait arguments should match the implementation");
    assert_eq!(value, Value::from_i32(7));
}

#[test]
fn iterator_satisfies_into_iterator_bound_without_an_explicit_impl() {
    let source = r#"
        struct Counter;
        impl Iterator for Counter {
            type Item = i32;
            fn next(&mut self) -> Option<i32> { None }
        }
        fn accepts<T: IntoIterator>(value: T) -> i32 { 7 }
        accepts((Counter {}))
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(7));
    let rejected = source.replace("accepts((Counter {}))", "struct Plain; accepts((Plain {}))");
    let error = eval_value(&rejected).expect_err("Plain does not implement IntoIterator");
    assert!(error.to_string().contains("IntoIterator"), "{error}");
}

#[test]
fn iterator_can_override_a_default_method() {
    let source = include_str!("fixtures/iterator_default_override.rils");
    assert_eq!(eval_value(source).unwrap(), Value::Usize(42));
    let mismatched = source.replace("fn count(self) -> usize", "fn count(self) -> i32");
    let error = eval_value(&mismatched).expect_err("override must match the trait signature");
    assert!(
        error.to_string().contains("return type of method `count`"),
        "{error}"
    );
}

#[test]
fn function_values_satisfy_precise_fn_bounds() {
    let source = include_str!("fixtures/function_trait_bound.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(14));

    for replacement in [
        "fn double(value: i32) -> string {\n    \"wrong\"\n}",
        "fn double(value: u8) -> i32 {\n    2\n}",
    ] {
        let mismatched = source.replace(
            "fn double(value: i32) -> i32 {\n    value * 2\n}",
            replacement,
        );
        let error = eval_value(&mismatched).expect_err("Fn bound requires an exact signature");
        assert!(
            error
                .to_string()
                .contains("does not implement required trait `Fn<(i32,), i32>`"),
            "{error}"
        );
    }
}

#[test]
fn callable_traits_follow_capture_effects() {
    let source = include_str!("fixtures/callable_trait_kinds.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(17));

    for (old, new, expected) in [
        (
            "invoke_mut(make_counter())",
            "invoke_shared(make_counter())",
            "Fn<(), i32>",
        ),
        (
            "F: FnOnce<(), string>",
            "F: FnMut<(), string>",
            "FnMut<(), string>",
        ),
    ] {
        let invalid = source.replace(old, new);
        let error = eval_value(&invalid).expect_err("the callable has weaker capture guarantees");
        assert!(error.to_string().contains(expected), "{error}");
    }
}

#[test]
fn callable_traits_accept_more_than_four_arguments() {
    let source = include_str!("fixtures/callable_many_arguments.rils");
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(108));
}

#[test]
fn evaluates_owned_values_and_explicit_clones() {
    let value = eval_value(
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
    let value = eval_value(
        r#"
        let handle: Rc<i32> = Rc::new(7);
        let weak = handle.downgrade();
        weak.upgrade().unwrap().strong_count()
        "#,
    )
    .expect("Rc and Weak should execute in the interpreter");
    assert!(value.as_usize().is_some_and(|count| count >= 1));
}

#[test]
fn evaluates_cell_interior_mutability() {
    let value = eval_value(
        r#"
        let cell: Cell<i32> = Cell::new(1);
        cell.set(2);
        cell.replace(3) + cell.get()
        "#,
    )
    .expect("Cell should execute in the interpreter");
    assert_eq!(value, Value::from_i32(5));
}

#[test]
fn evaluates_ref_cell_borrows() {
    let value = eval_value(
        r#"
        let cell: RefCell<i32> = RefCell::new(4);
        *cell.borrow() + cell.replace(5)
        "#,
    )
    .expect("RefCell should execute in the interpreter");
    assert_eq!(value, Value::from_i32(8));
}

#[test]
fn evaluates_vec_deque_operations() {
    let value = eval_value(
        r#"
        let mut queue: VecDeque<i32> = VecDeque::new();
        queue.push_back(2);
        queue.push_front(1);
        queue.pop_front().unwrap() + queue.pop_back().unwrap()
        "#,
    )
    .expect("VecDeque should execute in the interpreter");
    assert_eq!(value, Value::from_i32(3));
}

#[test]
fn evaluates_binary_heap_max_order_and_empty_cases() {
    for (source, expected) in [
        (
            r#"let mut heap: BinaryHeap<i32> = BinaryHeap::new();
               heap.push(2); heap.push(5); heap.push(1); heap.push(5);
               heap.peek_cloned().unwrap() + heap.pop().unwrap()
                 + heap.pop().unwrap() + heap.pop().unwrap() + heap.pop().unwrap()"#,
            Value::from_i32(18),
        ),
        (
            r#"let mut heap: BinaryHeap<string> = BinaryHeap::new();
               heap.push("a"); heap.push("z"); heap.pop().unwrap()"#,
            Value::from_string("z"),
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
        assert_eq!(eval_value(source).unwrap(), expected);
    }
}

#[test]
fn binary_heap_rejects_unorderable_values() {
    let error =
        eval_value(r#"let mut heap: BinaryHeap<bool> = BinaryHeap::new(); heap.push(true);"#)
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
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(10));
    assert_eq!(
        eval_value(r#"let mut map = std::collections::BTreeMap::new(); map.insert("b", 2); map.insert("a", 1); map.first_key_cloned().unwrap()"#).unwrap(),
        Value::from_string("a")
    );
    assert_eq!(
        eval_value(r#"let mut map: BTreeMap<i32, i32> = BTreeMap::new(); map.insert(2, 20); map.insert(1, 10); let mut sum = 0; for entry in map { sum = sum + entry.0; } sum"#).unwrap(),
        Value::from_i32(3)
    );
}

#[test]
fn borrowed_indexed_iterator_preserves_the_source() {
    for (source, expected) in [
        (
            "{ let values = [2, 3, 5]; let mut iter = values.iter(); let first = iter.next().unwrap(); if values.len() == 3 { *first } else { 0 } }",
            2,
        ),
        (
            "{ let mut values: Vec<i32> = Vec::new(); values.push(4); values.push(7); let mut sum = 0; for value in values.iter() { sum = sum + *value; } if values.len() == 2 { sum } else { 0 } }",
            11,
        ),
        (
            "{ let values = [2, 3, 5]; let count = values.iter().count(); if count == 3usize && values.len() == 3usize { 3 } else { 0 } }",
            3,
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(expected));
    }
}

#[test]
fn borrowed_indexed_iterator_rejects_escape_and_mutation() {
    let escaped =
        eval_value("fn escaped() -> Iter<&i32> { let values = [1, 2]; values.iter() } escaped()")
            .expect_err("borrowed iterator must not outlive its source");
    assert!(
        escaped
            .to_string()
            .contains("references cannot be returned"),
        "{escaped}"
    );

    let error = eval_value(
        "{ let mut values: Vec<i32> = Vec::new(); values.push(1); let iter = values.iter(); values.push(2); iter.count() }",
    )
    .expect_err("Vec mutation must be rejected during borrowed iteration");
    assert!(
        error.to_string().contains("cannot structurally mutate"),
        "{error}"
    );

    let replaced = eval_value(
        "{ let mut values = Vec::from([1]); let iter = values.iter(); values = Vec::from([2]); iter.count() }",
    )
    .expect_err("source replacement must be rejected during borrowed iteration");
    assert!(replaced.to_string().contains("referenced"), "{replaced}");

    assert_eq!(
        eval_value("{ let mut values = Vec::from([1]); { let iter = values.iter(); iter.count() }; values.push(2); values.len() }").unwrap(),
        Value::Usize(2),
    );
}

#[test]
fn borrowed_map_and_set_iterators_preserve_collections() {
    for (source, expected) in [
        (
            "{ let mut map: HashMap<i32, i32> = HashMap::new(); map.insert(1, 10); map.insert(2, 20); let mut sum = 0; for entry in map.iter() { sum = sum + *entry.0 + *entry.1; } if map.len() == 2usize { sum } else { 0 } }",
            33,
        ),
        (
            "{ let mut map: BTreeMap<i32, i32> = BTreeMap::new(); map.insert(2, 20); map.insert(1, 10); let mut iter = map.iter(); let first = iter.next().unwrap(); if map.len() == 2usize { *first.0 + *first.1 } else { 0 } }",
            11,
        ),
        (
            "{ let mut set: HashSet<i32> = HashSet::new(); set.insert(2); set.insert(3); let mut sum = 0; for item in set.iter() { sum = sum + *item; } if set.len() == 2usize { sum } else { 0 } }",
            5,
        ),
        (
            "{ let mut set: BTreeSet<i32> = BTreeSet::new(); set.insert(3); set.insert(2); let mut iter = set.iter(); let first = iter.next().unwrap(); if set.len() == 2usize { *first } else { 0 } }",
            2,
        ),
    ] {
        assert_eq!(eval_value(source).unwrap(), Value::from_i32(expected));
    }
}

#[test]
fn borrowed_map_and_set_items_block_structural_mutation() {
    for source in [
        "{ let mut map: HashMap<i32, i32> = HashMap::new(); map.insert(1, 10); let iter = map.iter(); map.insert(2, 20); iter.count() }",
        "{ let mut map: BTreeMap<i32, i32> = BTreeMap::new(); map.insert(1, 10); let item = { let mut iter = map.iter(); iter.next().unwrap() }; let key = 1; map.remove(&key); *item.1 }",
        "{ let mut set: HashSet<i32> = HashSet::new(); set.insert(1); let iter = set.iter(); set.insert(2); iter.count() }",
        "{ let mut set: BTreeSet<i32> = BTreeSet::new(); set.insert(1); let item = { let mut iter = set.iter(); iter.next().unwrap() }; let key = 1; set.remove(&key); *item }",
    ] {
        let error = eval_value(source).expect_err("mutation must be rejected while borrowed");
        assert!(
            error.to_string().contains("cannot mutate"),
            "{source}: {error}"
        );
    }
    assert_eq!(
        eval_value("{ let mut map: BTreeMap<i32, i32> = BTreeMap::new(); map.insert(1, 10); { let iter = map.iter(); iter.count() }; map.insert(2, 20); map.len() }").unwrap(),
        Value::Usize(2),
    );
    assert_eq!(
        eval_value("{ let mut set: HashSet<i32> = HashSet::new(); set.insert(1); { let iter = set.iter(); iter.count() }; set.insert(2); set.len() }").unwrap(),
        Value::Usize(2),
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
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(3));
    let error =
        eval_value(r#"let mut map: BTreeMap<f64, i32> = BTreeMap::new(); map.insert(1.5, 1);"#)
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
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(127));
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
    assert_eq!(eval_value(source).unwrap(), Value::Bool(true));
    let error = eval_value(r#"let mut set: BTreeSet<f64> = BTreeSet::new(); set.insert(1.5);"#)
        .unwrap_err();
    assert!(error.to_string().contains("BTreeSet elements must be"));
}

#[test]
fn enforces_configured_execution_limits() {
    let mut engine = Engine::new();
    engine.set_execution_limits(ExecutionLimits::new(1_000, 8));

    let error = engine
        .eval_value(
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
    let value = eval_value(
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
    assert_eq!(value, Value::from_i32(41));
}

#[test]
fn rejects_local_reference_return_escape() {
    let error = eval_value(
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
    let value = eval_value(
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
    assert_eq!(value, Value::from_i32(7));
}

#[test]
fn hash_maps_can_carry_reference_values_locally() {
    let value = eval_value(
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
    assert_eq!(value, Value::from_i32(42));
}
