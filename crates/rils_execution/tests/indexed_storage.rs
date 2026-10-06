use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    value::{FieldSlot, IndexedStorage, storage::TypedStorageContext},
};

fn optional_string() -> Value {
    Value::Option {
        value: Some(Rc::new(Value::from_string("owned"))),
        element_type: Some(Type::String),
    }
}

fn tuple(value: Value) -> Value {
    Value::Tuple(Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(Type::of_value(&value).unwrap(), value)]),
        element_type: RefCell::new(None),
        active_iterators: Cell::new(0),
    }))
}

fn annotation() -> Type {
    Type::Tuple(vec![Type::Option(Box::new(Type::String))])
}

#[test]
fn composition_keeps_unique_payload_ownership_and_concrete_array_metadata() {
    let expected_item = Type::Result(Box::new(Type::I32), Box::new(Type::String));
    let array = Value::Array(Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(
            Type::Result(Box::new(Type::Unknown), Box::new(Type::String)),
            Value::Result {
                value: Err(Rc::new(Value::from_string("owned"))),
                ok_type: None,
                error_type: Some(Type::String),
            },
        )]),
        element_type: RefCell::new(Some(Type::Unknown)),
        active_iterators: Cell::new(0),
    }));
    let expected = Type::Array {
        element: Box::new(expected_item),
        length: 1,
    };
    let value = TypedStorageContext::new(&[], &[])
        .apply_declared(array, &expected)
        .unwrap();
    assert_eq!(Type::of_value(&value), Some(expected));
    let Value::Array(sequence) = value else {
        unreachable!()
    };
    let value = sequence.elements.borrow_mut()[0].value.take().unwrap();
    let Value::Dynamic(object) = value else {
        panic!("missing native result")
    };
    assert!(
        object.into_value().is_ok(),
        "composition must not share owned storage"
    );
}

#[test]
fn shared_indexed_storage_is_rejected_before_mutation() {
    let value = tuple(optional_string());
    let Value::Tuple(observer) = value.clone() else {
        unreachable!()
    };
    let error = TypedStorageContext::new(&[], &[])
        .apply_declared(value, &annotation())
        .unwrap_err();
    assert!(error.contains("shared indexed storage"));
    let slots = observer.elements.borrow();
    assert!(matches!(slots[0].value, Some(Value::Option { .. })));
    assert_eq!(slots[0].value.as_ref().unwrap().to_string(), "Some(owned)");
}

#[test]
fn unchanged_children_preserve_shared_storage_and_references() {
    let value = tuple(Value::Unit);
    let Value::Tuple(observer) = value.clone() else {
        unreachable!()
    };
    observer.elements.borrow_mut()[0].references = 2;
    let value = TypedStorageContext::new(&[], &[])
        .apply_declared(value, &Type::Tuple(vec![Type::Unit]))
        .unwrap();
    let Value::Tuple(sequence) = value else {
        unreachable!()
    };
    assert!(Rc::ptr_eq(&observer, &sequence));
    assert_eq!(sequence.elements.borrow()[0].references, 2);
}

#[test]
fn active_references_iterators_and_partial_moves_reject_composition() {
    for case in 0..3 {
        let value = tuple(optional_string());
        let Value::Tuple(sequence) = &value else {
            unreachable!()
        };
        let expected = if case == 2 {
            let mut moved = FieldSlot::new(Type::I32, Value::from_i32(1));
            moved.value.take();
            sequence.elements.borrow_mut().push(moved);
            Type::Tuple(vec![Type::Option(Box::new(Type::String)), Type::I32])
        } else {
            if case == 0 {
                sequence.elements.borrow_mut()[0].references = 1;
            } else {
                sequence.active_iterators.set(1);
            }
            annotation()
        };
        let error = TypedStorageContext::new(&[], &[])
            .apply_declared(value, &expected)
            .unwrap_err();
        assert!(
            error.contains(match case {
                0 => "active element references",
                1 => "indexed iteration",
                _ => "partially moved",
            }),
            "{error}"
        );
    }
}
