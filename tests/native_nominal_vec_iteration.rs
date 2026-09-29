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
fn native_vec_moves_user_enums_only_when_iterator_advances() {
    let stored = "enum Event { Value(i32) } let events: Vec<Event> = Vec::new(); events";
    assert!(matches!(eval_value(stored).unwrap(), Value::Dynamic(_)));
    assert!(matches!(
        compile(stored).unwrap().execute_value().unwrap(),
        Value::Dynamic(_)
    ));
    let source = r#"
        enum Event { Value(i32) }
        let mut events: Vec<Event> = Vec::new();
        events.push(Event::Value(7));
        events.push(Event::Value(35));
        let mut sum = 0;
        for event in events {
            match event { Event::Value(value) => { sum = sum + value; } }
        }
        sum
    "#;
    assert_both(source, Value::from_i32(42));
}

#[test]
fn explicit_into_iter_decodes_user_struct_on_next() {
    let source = r#"
        struct Item { value: i32 }
        let mut items: Vec<Item> = Vec::new();
        items.push(Item { value: 42 });
        let mut iterator = items.into_iter();
        let item = iterator.next().unwrap();
        item.value
    "#;
    assert_both(source, Value::from_i32(42));
}

#[test]
fn borrowed_iter_reads_user_struct_without_moving_collection() {
    let source = r#"
        struct Item { value: i32 }
        fn run() -> i32 {
            let mut items: Vec<Item> = Vec::new();
            items.push(Item { value: 19 });
            let mut iterator = items.iter();
            let item = iterator.next().unwrap();
            if items.len() == 1 { item.value } else { 0 }
        }
        run()
    "#;
    assert_both(source, Value::from_i32(19));
}

#[test]
fn borrowed_iter_reads_user_struct_with_owned_string_field() {
    let source = r#"
        struct Item { value: string }
        fn run() -> usize {
            let mut items: Vec<Item> = Vec::new();
            items.push(Item { value: "hello" });
            let mut iterator = items.iter();
            let item = iterator.next().unwrap();
            let text = &item.value;
            text.len()
        }
        run()
    "#;
    assert_both(source, Value::from_usize(5));
}

#[test]
fn native_map_moves_user_values_when_iterator_advances() {
    let source = r#"
        struct Item { value: i32 }
        let mut map: HashMap<i32, Item> = HashMap::new();
        map.insert(7, Item { value: 42 });
        let mut iterator = map.into_iter();
        let entry = iterator.next().unwrap();
        entry.1.value
    "#;
    assert_both(source, Value::from_i32(42));

    let ordered = r#"
        struct Item { value: i32 }
        let mut map: BTreeMap<i32, Item> = BTreeMap::new();
        map.insert(2, Item { value: 20 });
        map.insert(1, Item { value: 22 });
        let mut sum = 0;
        for entry in map { sum = sum + entry.1.value; }
        sum
    "#;
    assert_both(ordered, Value::from_i32(42));
}

#[test]
fn native_set_owned_iterator_keeps_elements() {
    let source = r#"
        let mut set: BTreeSet<i32> = BTreeSet::new();
        set.insert(5);
        set.insert(2);
        let mut iterator = set.into_iter();
        iterator.next().unwrap() + iterator.next().unwrap()
    "#;
    assert_both(source, Value::from_i32(7));

    let hashed = r#"
        let mut set: HashSet<i32> = HashSet::new();
        set.insert(5);
        set.insert(2);
        let mut sum = 0;
        for value in set { sum = sum + value; }
        sum
    "#;
    assert_both(hashed, Value::from_i32(7));
}
