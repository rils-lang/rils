use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{FieldSlot, IndexedStorage, ReferenceValue},
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
        let array = Value::Array(Rc::new(IndexedStorage {
            elements: RefCell::new(
                items
                    .into_iter()
                    .map(|item| FieldSlot {
                        value: Some(Value::from_i32(item)),
                        type_annotation: Type::I32,
                        references: 0,
                    })
                    .collect(),
            ),
            element_type: RefCell::new(Some(Type::I32)),
            active_iterators: std::cell::Cell::new(0),
        }));
        let value = call_native_owned_symbol(
            symbol("from"),
            vec![array],
            &NativeOwnedContext {
                structs: Vec::new(),
                enums: Vec::new(),
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
    let sequence = Rc::new(IndexedStorage {
        elements: RefCell::new(vec![
            FieldSlot {
                value: Some(Value::from_i32(1)),
                type_annotation: Type::I32,
                references: 1,
            },
            FieldSlot {
                value: Some(Value::from_i32(2)),
                type_annotation: Type::I32,
                references: 0,
            },
        ]),
        element_type: RefCell::new(Some(Type::I32)),
        active_iterators: std::cell::Cell::new(0),
    });
    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage
        .borrow_mut()
        .initialize(Value::Vec(sequence.clone()));
    let receiver = Value::Reference(Rc::new(ReferenceValue::new_storage(storage, true)));

    let before = sequence.elements.borrow().as_ptr();
    assert_eq!(
        call_native_symbol(symbol("len"), std::slice::from_ref(&receiver)).unwrap(),
        Ok(Value::Usize(2))
    );
    assert_eq!(sequence.elements.borrow().as_ptr(), before);
    assert_eq!(sequence.elements.borrow()[0].references, 1);

    for (name, arguments) in [
        ("push", vec![receiver.clone(), Value::from_i32(3)]),
        (
            "insert",
            vec![receiver.clone(), Value::Usize(1), Value::from_i32(3)],
        ),
        ("remove", vec![receiver.clone(), Value::Usize(1)]),
        ("swap_remove", vec![receiver.clone(), Value::Usize(1)]),
    ] {
        let result = call_native_symbol(symbol(name), &arguments).unwrap();
        assert!(result.unwrap_err().contains("referenced"), "Vec::{name}");
        assert_eq!(sequence.elements.borrow().len(), 2);
        assert_eq!(sequence.elements.borrow()[0].references, 1);
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

    assert_eq!(first.read().unwrap().as_i32(), Some(3));
    first.write(Value::from_i32(7)).unwrap();
    assert_eq!(second.read().unwrap().as_i32(), Some(7));
    second.write(Value::from_i32(9)).unwrap();
    let reborrowed = second.reborrow(true).unwrap();
    reborrowed.write(Value::from_i32(11)).unwrap();
    assert_eq!(first.read().unwrap().as_i32(), Some(11));
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
