use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{ReferenceValue, storage::TypedStorageContext},
};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

fn symbol(name: &str) -> &'static str {
    rils_builtins::builtin("Vec")
        .unwrap()
        .member(name)
        .unwrap()
        .native_symbol
        .unwrap()
}

#[test]
fn vec_from_typed_array_constructs_native_storage() {
    for items in [vec![], vec![2, 5]] {
        let length = items.len();
        let array = TypedStorageContext::new(&[], &[])
            .construct_array(
                &Type::Array {
                    element: Box::new(Type::I32),
                    length,
                },
                items
                    .into_iter()
                    .map(|value| Value::from(rils_execution::value::native_i32(value)))
                    .collect(),
            )
            .unwrap();
        let value = call_native_owned_symbol(
            symbol("from"),
            vec![array],
            &NativeOwnedContext {
                structs: Vec::new(),
                enums: Vec::new(),
                ..Default::default()
            },
        )
        .unwrap()
        .unwrap();
        let Value::Dynamic(object) = value else {
            panic!("Vec::from must construct native storage");
        };
        assert_eq!(
            object.with(|payload| payload.sequence_len()),
            Ok(Ok(length))
        );
    }
}

#[test]
fn native_vec_receiver_preserves_referenced_slots_and_rejects_reordering() {
    let context = NativeOwnedContext::default();
    let array = context
        .storage()
        .construct_array(
            &Type::Array {
                element: Box::new(Type::I32),
                length: 2,
            },
            vec![
                Value::from(rils_execution::value::native_i32(1)),
                Value::from(rils_execution::value::native_i32(2)),
            ],
        )
        .unwrap();
    let value = call_native_owned_symbol(symbol("from"), vec![array], &context)
        .unwrap()
        .unwrap();
    let Value::Dynamic(sequence) = &value else {
        panic!("native Vec expected")
    };
    let element =
        ReferenceValue::new_guarded_dynamic_indexed_element(sequence.clone(), 0, false, None)
            .unwrap();
    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage.borrow_mut().initialize(value);
    let receiver =
        Value::from_reference(Rc::new(ReferenceValue::new_storage(storage.clone(), true)));
    assert_eq!(
        call_native_symbol(symbol("len"), std::slice::from_ref(&receiver)).unwrap(),
        Ok(Value::from(rils_execution::value::native_usize(2)))
    );
    for (name, arguments) in [
        ("clear", vec![receiver.clone()]),
        ("pop", vec![receiver.clone()]),
        (
            "truncate",
            vec![
                receiver.clone(),
                Value::from(rils_execution::value::native_usize(0)),
            ],
        ),
        (
            "push",
            vec![
                receiver.clone(),
                Value::from(rils_execution::value::native_i32(3)),
            ],
        ),
        (
            "insert",
            vec![
                receiver.clone(),
                Value::from(rils_execution::value::native_usize(1)),
                Value::from(rils_execution::value::native_i32(3)),
            ],
        ),
        (
            "remove",
            vec![
                receiver.clone(),
                Value::from(rils_execution::value::native_usize(1)),
            ],
        ),
        (
            "swap_remove",
            vec![
                receiver.clone(),
                Value::from(rils_execution::value::native_usize(1)),
            ],
        ),
    ] {
        let result = if rils_execution::runtime_builtins::requires_owned_native_call(symbol(name)) {
            call_native_owned_symbol(symbol(name), arguments, &context).unwrap()
        } else {
            rils_execution::runtime_builtins::call_native_symbol(symbol(name), &arguments).unwrap()
        };
        assert!(result.unwrap_err().contains("referenced"), "Vec::{name}");
        assert_eq!(storage.borrow().read().unwrap().to_string(), "[1, 2]");
        assert_eq!(element.read().unwrap().get_cloned::<i32>().ok(), Some(1));
    }
}

#[test]
fn two_mutable_handles_to_one_native_element_use_short_borrows() {
    let item = DynamicLayout::copy_of::<i32>(Type::I32);
    let vector = DynamicLayout::sequence(
        Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::I32],
        },
        item.clone(),
    );
    let payload = DynamicValue::sequence(
        vector.clone(),
        vec![DynamicValue::from_rust(item.clone(), 3).unwrap()],
    )
    .unwrap();
    let object: DynamicObject<Value> =
        DynamicObject::new(Rc::new(DynamicType::new(vector)), payload).unwrap();
    let first =
        ReferenceValue::new_guarded_dynamic_indexed_element(object.clone(), 0, true, None).unwrap();
    let second =
        ReferenceValue::new_guarded_dynamic_indexed_element(object.clone(), 0, true, None).unwrap();

    assert_eq!(first.read().unwrap().get_cloned::<i32>().ok(), Some(3));
    first
        .write(Value::from(rils_execution::value::native_i32(7)))
        .unwrap();
    assert_eq!(second.read().unwrap().get_cloned::<i32>().ok(), Some(7));
    second
        .write(Value::from(rils_execution::value::native_i32(9)))
        .unwrap();
    let reborrowed = second.reborrow(true).unwrap();
    reborrowed
        .write(Value::from(rils_execution::value::native_i32(11)))
        .unwrap();
    assert_eq!(first.read().unwrap().get_cloned::<i32>().ok(), Some(11));
    assert!(
        object
            .with_mut(|payload| payload
                .push_sequence_item(DynamicValue::from_rust(item.clone(), 10).unwrap()))
            .unwrap()
            .is_err()
    );
    drop(first);
    assert!(
        object
            .with_mut(|payload| payload.clear_sequence())
            .unwrap()
            .is_err()
    );
    drop(second);
    drop(reborrowed);
    object
        .with_mut(|payload| payload.push_sequence_item(DynamicValue::from_rust(item, 10).unwrap()))
        .unwrap()
        .unwrap();
    assert_eq!(object.with(|payload| payload.sequence_len()), Ok(Ok(2)));
}

#[test]
fn reference_handle_stays_compact() {
    assert!(
        std::mem::size_of::<ReferenceValue>() <= 80,
        "reference handle grew to {} bytes",
        std::mem::size_of::<ReferenceValue>()
    );
}

#[test]
fn owned_sequence_arguments_reject_aliases_borrows_and_layout_mismatches() {
    let context = NativeOwnedContext::default();
    let make = |ty: Type, value: Value| {
        let array = context
            .storage()
            .construct_array(
                &Type::Array {
                    element: Box::new(ty),
                    length: 1,
                },
                vec![value],
            )
            .unwrap();
        call_native_owned_symbol(symbol("from"), vec![array], &context)
            .unwrap()
            .unwrap()
    };
    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage
        .borrow_mut()
        .initialize(make(Type::I32, rils_execution::value::native_i32(1).into()));
    let receiver = Value::from_reference(Rc::new(ReferenceValue::new_storage(storage, true)));
    let other = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    other
        .borrow_mut()
        .initialize(make(Type::I32, rils_execution::value::native_i32(2).into()));
    let borrowed = Value::from_reference(Rc::new(ReferenceValue::new_storage(other, false)));
    for (source, expected) in [
        (receiver.clone(), "itself"),
        (borrowed, "owned"),
        (
            make(Type::Bool, rils_execution::value::native_bool(true).into()),
            "layout",
        ),
    ] {
        let error =
            call_native_owned_symbol(symbol("extend"), vec![receiver.clone(), source], &context)
                .unwrap()
                .unwrap_err();
        assert!(error.contains(expected), "expected {expected}: {error}");
        assert_eq!(
            call_native_symbol(symbol("len"), std::slice::from_ref(&receiver))
                .unwrap()
                .unwrap()
                .get_cloned::<usize>()
                .unwrap(),
            1
        );
    }
}

#[test]
fn borrowed_sequence_iteration_uses_shared_dispatch_and_retains_its_lease() {
    assert!(!rils_execution::runtime_builtins::requires_owned_native_call(symbol("iter")));
    let context = NativeOwnedContext::default();
    for array in [false, true] {
        for length in [0, 1, 4] {
            let value = context
                .storage()
                .construct_array(
                    &Type::Array {
                        element: Box::new(Type::I32),
                        length,
                    },
                    (0..length)
                        .map(|i| rils_execution::value::native_i32(i as i32).into())
                        .collect(),
                )
                .unwrap();
            let value = if array {
                value
            } else {
                call_native_owned_symbol(symbol("from"), vec![value], &context)
                    .unwrap()
                    .unwrap()
            };
            assert!(
                call_native_symbol(symbol("iter"), std::slice::from_ref(&value))
                    .unwrap()
                    .is_err()
            );
            let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
            storage.borrow_mut().initialize(value);
            let receiver =
                Value::from_reference(Rc::new(ReferenceValue::new_storage(storage, true)));
            let iterator = call_native_symbol(symbol("iter"), std::slice::from_ref(&receiver))
                .unwrap()
                .unwrap();
            if !array {
                assert!(
                    call_native_symbol(symbol("clear"), std::slice::from_ref(&receiver))
                        .unwrap()
                        .is_err()
                );
            }
            drop(iterator);
            if !array {
                call_native_symbol(symbol("clear"), std::slice::from_ref(&receiver))
                    .unwrap()
                    .unwrap();
            }
        }
    }
}
