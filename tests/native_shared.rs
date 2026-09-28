use rils::{Value, compile, eval_value};

#[test]
fn shared_handles_match_in_interpreter_and_vm() {
    let source = r#"
        let owner: core::rc::Rc<i32> = core::rc::Rc::new(40);
        let weak: core::rc::Weak<i32> = owner.downgrade();
        let copy = owner.clone();
        if owner.strong_count() >= 2usize
            && weak.strong_count() == 2usize
            && weak.weak_count() == 1usize
            && weak.upgrade().is_some()
        { 42 } else { 0 }
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
}

#[test]
fn shared_handle_can_be_held_by_an_option() {
    let source = r#"
        let owner: core::rc::Rc<i32> = core::rc::Rc::new(40);
        let holder: Option<core::rc::Rc<i32>> = Some(owner);
        holder.is_some()
    "#;
    assert_eq!(eval_value(source).unwrap(), Value::Bool(true));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::Bool(true)
    );
}

#[test]
fn opaque_native_handle_cannot_be_constructed_with_empty_fields() {
    let source = "let item = (Rc {});";
    let runtime_error = eval_value(source).unwrap_err().to_string();
    assert!(
        runtime_error.contains("cannot construct opaque type"),
        "{runtime_error}"
    );
    let compile_error = compile(source).err().unwrap().to_string();
    assert!(
        compile_error.contains("cannot construct opaque type"),
        "{compile_error}"
    );
}

#[test]
fn private_box_storage_cannot_be_constructed_from_fields() {
    let source = "let value = (Box { value: 42 });";
    let runtime_error = eval_value(source).unwrap_err().to_string();
    assert!(
        runtime_error.contains("cannot construct opaque type")
            || runtime_error.contains("is not a record type"),
        "{runtime_error}"
    );
    let compile_error = compile(source).err().unwrap().to_string();
    assert!(
        compile_error.contains("cannot construct opaque type"),
        "{compile_error}"
    );
}

#[test]
fn private_box_storage_cannot_be_read_as_a_field() {
    let source = "fn value(boxed: Box<i32>) -> i32 { boxed.value }";
    assert!(compile(source).is_err());
}

#[test]
fn user_unit_struct_remains_constructible() {
    let source = "struct Handle; let item = (Handle {}); 42";
    assert_eq!(eval_value(source).unwrap(), Value::from_i32(42));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::from_i32(42)
    );
}
