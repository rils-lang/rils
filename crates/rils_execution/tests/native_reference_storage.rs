use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    RilsValue, Type, Value,
    environment::{Environment, StorageSlot},
    value::{ReferenceValue, native_instance::NativeInstancePlace, storage::TypedStorageContext},
};

fn initialized(value: Value, mutable: bool) -> StorageSlot {
    let mut slot = StorageSlot::uninitialized(mutable);
    slot.initialize(value);
    slot
}

#[test]
fn reference_reads_copy_values_but_never_clone_noncopy_payloads() {
    for value in [
        Value::from(rils_execution::value::native_i32(42)),
        Value::from(rils_execution::value::native_string("retained")),
    ] {
        let copy = value.is_copy();
        let ty = Type::of_value(&value).unwrap();
        let slot = Rc::new(RefCell::new(initialized(value, true)));
        let reference = Rc::new(ReferenceValue::new_storage(slot.clone(), true));
        assert_eq!(reference.read().is_ok(), copy);
        let nested_slot = Rc::new(RefCell::new(initialized(
            Value::from_reference(reference.clone()),
            false,
        )));
        let nested = ReferenceValue::new_storage(nested_slot, false);
        let copied_reference = nested.read().unwrap().as_reference().unwrap();
        assert!(Rc::ptr_eq(&copied_reference, &reference));
        let writing = slot.borrow_mut();
        assert!(reference.read().is_err());
        drop(writing);
        drop(copied_reference);
        drop(nested);
        drop(reference);
        let original = slot.borrow_mut().take().unwrap();
        let tuple = TypedStorageContext::new(&[], &[])
            .construct_tuple(&Type::Tuple(vec![ty]), vec![original])
            .unwrap();
        let Value::Dynamic(tuple) = tuple else {
            unreachable!()
        };
        let field = NativeInstancePlace::new(tuple).unwrap().field("0").unwrap();
        let reference = field.borrow(false, None).unwrap();
        assert_eq!(reference.read().is_ok(), copy);
        if copy {
            assert_eq!(reference.read().unwrap().get_cloned::<i32>().unwrap(), 42);
        } else {
            assert!(
                Value::from_reference(Rc::new(reference))
                    .with_ref::<String, _>(|text| text == "retained")
                    .unwrap()
            );
        }
    }
}

#[test]
fn copied_native_leases_block_moves_until_the_last_copy_drops() {
    let slot = Rc::new(RefCell::new(initialized(
        Value::from(rils_execution::value::native_string("owned")),
        true,
    )));
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
    assert!(matches!(reference, Value::Dynamic(_)));
    assert!(reference.is_copy());
    let mut binding = initialized(reference, false);
    let first = binding.take().unwrap();
    let second = binding.take().unwrap();
    binding.clear();
    assert!(slot.borrow_mut().take().is_err());
    first
        .as_reference()
        .unwrap()
        .write(Value::from(rils_execution::value::native_string("changed")))
        .unwrap();
    assert!(second.as_reference().unwrap().read().is_err());
    assert!(
        second
            .with_ref::<String, _>(|text| text == "changed")
            .unwrap()
    );
    drop(first);
    assert!(slot.borrow_mut().take().is_err());
    drop(second);
    assert_eq!(
        slot.borrow_mut()
            .take()
            .unwrap()
            .get_cloned::<String>()
            .ok()
            .as_deref(),
        Some("changed")
    );
}

#[test]
fn composed_reference_copies_preserve_identity_type_and_local_origin() {
    let environment = Environment::global();
    environment.borrow_mut().define(
        "number",
        Value::from(rils_execution::value::native_i32(1)),
        true,
        None,
    );
    let slot = environment.borrow().slot("number").unwrap();
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)));
    let ty = Type::of_value(&reference).unwrap();
    let tuple = TypedStorageContext::new(&[], &[])
        .construct_tuple(
            &Type::Tuple(vec![ty.clone()]),
            vec![reference.clone_owned().unwrap()],
        )
        .unwrap();
    let tuple = tuple.clone_owned().unwrap();
    assert!(tuple.contains_reference());
    assert!(tuple.contains_local_reference(&environment));
    let Value::Dynamic(tuple) = tuple else {
        panic!("native tuple expected")
    };
    let decoded = NativeInstancePlace::new(tuple)
        .unwrap()
        .field("0")
        .unwrap()
        .take()
        .unwrap();
    assert_eq!(Type::of_value(&decoded), Some(ty));
    assert!(decoded == reference);
    assert!(decoded.contains_local_reference(&environment));
    decoded
        .as_reference()
        .unwrap()
        .write(Value::from(rils_execution::value::native_i32(42)))
        .unwrap();
    assert_eq!(
        reference
            .as_reference()
            .unwrap()
            .read()
            .unwrap()
            .get_cloned::<i32>()
            .ok(),
        Some(42)
    );
}

#[test]
fn shared_reference_rejects_writes_after_native_roundtrip() {
    let slot = Rc::new(RefCell::new(initialized(
        Value::from(rils_execution::value::native_i32(7)),
        true,
    )));
    let reference =
        Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
    let copied = reference.clone_owned().unwrap();
    assert!(
        copied
            .as_reference()
            .unwrap()
            .write(Value::from(rils_execution::value::native_i32(8)))
            .is_err()
    );
    assert_eq!(
        slot.borrow().read().unwrap().get_cloned::<i32>().ok(),
        Some(7)
    );
    assert_eq!(
        RilsValue::new(copied)
            .with_ref::<i32, _>(|value| *value)
            .unwrap(),
        7
    );
}
