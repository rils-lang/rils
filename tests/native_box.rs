use rils::{Value, compile, eval_value};

fn assert_both(source: &str, expected: Value) {
    assert_eq!(eval_value(source).unwrap(), expected);
    let module = compile(source).unwrap();
    assert_eq!(module.execute_value().unwrap(), expected);
    let loaded = rils::BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap(), expected);
}

#[test]
fn box_construction_infers_or_accepts_the_type_argument() {
    assert_both("Box::new(42).into_inner()", Value::from_i32(42));
    assert_both("Box::<i32>::new(42).into_inner()", Value::from_i32(42));
}

#[test]
fn explicit_box_type_arguments_are_checked() {
    let source = "Box::<string>::new(42)";
    assert!(compile(source).is_err());
    assert!(eval_value(source).is_err());
}

#[test]
fn box_moves_non_copy_string_values() {
    assert_both(
        "let value: string = \"hello\"; Box::new(value).into_inner()",
        Value::from_string("hello"),
    );
}

#[test]
fn box_moves_composite_values() {
    assert_both(
        "let item = Box::new(Some(42)); item.into_inner().unwrap()",
        Value::from_i32(42),
    );
    assert_both(
        "let values: Vec<i32> = Vec::from([1, 2]); Box::new(values).into_inner().len()",
        Value::from_usize(2),
    );
}

#[test]
fn box_moves_user_enum_values() {
    let source = r#"
        enum Status { Ready(i32), Waiting }
        let value: Status = Status::Ready(42);
        let restored: Status = Box::new(value).into_inner();
        match restored { Status::Ready(number) => number, Status::Waiting => 0 }
    "#;
    assert_both(source, Value::from_i32(42));
}

#[test]
fn box_handles_absent_user_values() {
    let source = r#"
        struct Item { value: i32 }
        let absent: Option<Item> = None;
        Box::new(absent).into_inner().is_none()
    "#;
    assert_both(source, Value::Bool(true));
}

#[test]
fn box_supports_recursive_user_structs() {
    let source = r#"
        struct Node { value: i32, next: Option<Box<Node>> }
        let tail: Node = Node { value: 42, next: None };
        let restored: Node = Box::new(tail).into_inner();
        restored.value
    "#;
    assert_both(source, Value::from_i32(42));
}

#[test]
fn box_moves_through_recursive_optional_fields() {
    let source = r#"
        struct Node { value: i32, next: Option<Box<Node>> }
        let tail: Node = Node { value: 42, next: None };
        let head: Node = Node { value: 1, next: Some(Box::new(tail)) };
        let boxed: Box<Node> = head.next.unwrap();
        let restored: Node = boxed.into_inner();
        restored.value
    "#;
    assert_both(source, Value::from_i32(42));
}
