use rils::{Value, compile, eval};

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
    assert_eq!(eval(source).unwrap(), Value::I32(42));
    assert_eq!(compile(source).unwrap().execute().unwrap(), Value::I32(42));
}
