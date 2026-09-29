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
