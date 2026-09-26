use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::call_native_symbol,
    value::{FieldSlot, IndexedStorage, ReferenceValue},
};

fn symbol(name: &str) -> &'static str {
    rils_builtins::builtin("Vec")
        .unwrap()
        .member(name)
        .unwrap()
        .native_symbol
        .unwrap()
}

#[test]
fn native_vec_receiver_preserves_referenced_slots_and_rejects_reordering() {
    let sequence = Rc::new(IndexedStorage {
        elements: RefCell::new(vec![
            FieldSlot {
                value: Some(Value::I32(1)),
                type_annotation: Type::I32,
                references: 1,
            },
            FieldSlot {
                value: Some(Value::I32(2)),
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
        ("push", vec![receiver.clone(), Value::I32(3)]),
        (
            "insert",
            vec![receiver.clone(), Value::Usize(1), Value::I32(3)],
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
