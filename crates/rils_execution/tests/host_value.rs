use rils_execution::{RilsValue, Value};

#[test]
fn borrows_clones_and_moves_exact_native_values() {
    let result = RilsValue::new(Value::from_i32(42));
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 42);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 42);
    assert_eq!(result.into_owned::<i32>().ok(), Some(42));
}

#[test]
fn failed_move_preserves_the_owned_handle() {
    let result = RilsValue::new(Value::from_i32(42));
    let (result, error) = *result.into_owned::<i64>().err().unwrap();
    assert!(error.contains("Number"));
    assert_eq!(result.get_cloned::<i32>().unwrap(), 42);
}

#[test]
fn borrowed_string_can_be_read_without_cloning() {
    let result = RilsValue::new(Value::from_string("hello"));
    assert_eq!(result.with_ref::<String, _>(|text| text.len()).unwrap(), 5);
    assert_eq!(result.get_cloned::<String>().unwrap(), "hello");
    assert_eq!(result.into_owned::<String>().ok().as_deref(), Some("hello"));
}

#[test]
fn a_reference_cannot_be_moved_out() {
    use std::{cell::RefCell, rc::Rc};

    use rils_execution::{environment::StorageSlot, value::ReferenceValue};

    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage.borrow_mut().initialize(Value::from_i32(7));
    let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(storage, false)));
    let result = RilsValue::new(reference);
    assert_eq!(result.with_ref::<i32, _>(|value| *value).unwrap(), 7);
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
    let (result, error) = *result.into_owned::<i32>().err().unwrap();
    assert!(error.contains("not an owned"));
    assert_eq!(result.get_cloned::<i32>().unwrap(), 7);
}
