use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::call_native_symbol,
    value::{
        NativeObject, NativeType, ReferenceValue, native_instance::NativeInstancePlace,
        storage::TypedStorageContext,
    },
};

fn call(trait_name: &str, method: &str, receiver: &Value) -> Result<Value, String> {
    let symbol = rils_builtins::builtin_member(trait_name, method)
        .unwrap()
        .native_symbol
        .unwrap();
    call_native_symbol(symbol, std::slice::from_ref(receiver)).unwrap()
}

#[test]
fn borrowed_display_keeps_storage_guarded_and_never_requests_clone() {
    let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    let weak = Rc::downgrade(&slot);
    let descriptor = Rc::new(
        NativeType::new::<()>(Type::named("DisplayProbe"))
            .register_method("Display::fmt", move |_| {
                assert!(weak.upgrade().unwrap().try_borrow_mut().is_err());
                Ok(rils_execution::value::native_string("observed").into())
            })
            .register_method("Clone::clone", |_| {
                panic!("formatting must not clone its receiver")
            }),
    );
    slot.borrow_mut()
        .initialize(NativeObject::new(descriptor, ()).unwrap().into());
    let reference =
        Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
    assert!(reference.as_reference().unwrap().read().is_err());
    assert_eq!(reference.to_string(), "observed");
    assert_eq!(reference.type_name(), "&DisplayProbe");
    assert_eq!(format!("{reference:?}"), "observed");
    assert_eq!(
        rils_execution::formatting::format_arguments("{}", std::slice::from_ref(&reference))
            .unwrap(),
        "observed"
    );
    let nested_slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
    nested_slot.borrow_mut().initialize(reference);
    let nested = Value::from_reference(Rc::new(ReferenceValue::new_storage(nested_slot, false)));
    assert_eq!(nested.to_string(), "observed");
    assert_eq!(nested.type_name(), "&&DisplayProbe");
    let receiver = rils_execution::formatting::receiver_reference(&nested).unwrap();
    assert_eq!(receiver.type_name(), "&DisplayProbe");
    assert_eq!(receiver.to_string(), "observed");
    let Value::Dynamic(tuple) = TypedStorageContext::new(&[], &[])
        .construct_tuple(
            &Type::Tuple(vec![Type::of_value(&nested).unwrap()]),
            vec![nested.clone()],
        )
        .unwrap()
    else {
        panic!("native tuple")
    };
    let projected = NativeInstancePlace::new(tuple)
        .unwrap()
        .field("0")
        .unwrap()
        .borrow(false, None)
        .unwrap();
    let projected = Value::from_reference(Rc::new(projected));
    assert!(rils_execution::value::native_ops::has_method(&projected, "Display::fmt").unwrap());
    assert_eq!(
        rils_execution::formatting::format_arguments("{}", &[projected]).unwrap(),
        "observed"
    );
    let _writing = slot.borrow_mut();
    assert!(
        rils_execution::formatting::format_arguments("{}", std::slice::from_ref(&nested)).is_err()
    );
}

#[test]
fn native_operations_keep_the_original_slot_borrowed() {
    for (trait_name, method, native_symbol) in [
        ("Clone", "clone", "Clone::clone"),
        ("Iterator", "next", "Iterator::next"),
    ] {
        let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        let weak = Rc::downgrade(&slot);
        let calls = Rc::new(std::cell::Cell::new(0));
        let observed = calls.clone();
        let descriptor = Rc::new(
            NativeType::new::<()>(Type::named("OperationProbe")).register_method(
                native_symbol,
                move |_| {
                    assert!(weak.upgrade().unwrap().try_borrow_mut().is_err());
                    observed.set(observed.get() + 1);
                    if trait_name == "Clone" {
                        Ok(Value::from(rils_execution::value::native_i32(42)))
                    } else {
                        TypedStorageContext::new(&[], &[])
                            .construct_option(&Type::Option(Box::new(Type::I32)), None)
                    }
                },
            ),
        );
        slot.borrow_mut()
            .initialize(Value::Native(NativeObject::new(descriptor, ()).unwrap()));
        let reference =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        assert!(rils_execution::value::native_ops::has_method(&reference, native_symbol).unwrap());
        assert!(!rils_execution::value::native_ops::has_method(&reference, "missing").unwrap());
        assert_eq!(calls.get(), 0);
        call(trait_name, method, &reference).unwrap();
        assert_eq!(calls.get(), 1);
        {
            let _writing = slot.borrow_mut();
            assert!(
                rils_execution::value::native_ops::has_method(&reference, native_symbol).is_err()
            );
            assert!(
                call(trait_name, method, &reference)
                    .unwrap_err()
                    .contains("mutably accessed")
            );
        }
        assert_eq!(calls.get(), 1);
        slot.borrow_mut().clear();
        assert!(
            call(trait_name, method, &reference)
                .unwrap_err()
                .contains("moved")
        );
    }
}

#[test]
fn clone_projected_value_preserves_owner_and_reference_identity() {
    let context = TypedStorageContext::new(&[], &[]);
    let Value::Dynamic(tuple) = context
        .construct_tuple(
            &Type::Tuple(vec![Type::String, Type::String]),
            vec![
                Value::from(rils_execution::value::native_string("original")),
                Value::from(rils_execution::value::native_string("moved")),
            ],
        )
        .unwrap()
    else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(tuple.clone()).unwrap();
    drop(place.field("1").unwrap().take().unwrap());
    let reference = Value::from_reference(Rc::new(
        place.field("0").unwrap().borrow(false, None).unwrap(),
    ));
    let cloned = call("Clone", "clone", &reference).unwrap();
    assert_eq!(cloned.get_cloned::<String>().unwrap(), "original");
    assert!(cloned.as_reference().is_none());
    assert!(Value::Dynamic(tuple).is_partially_moved());

    let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
    let original_reference = reference.as_reference().unwrap();
    slot.borrow_mut().initialize(reference);
    let nested = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, false)));
    let cloned = call("Clone", "clone", &nested).unwrap();
    assert!(Rc::ptr_eq(
        &cloned.as_reference().unwrap(),
        &original_reference
    ));
}

#[test]
fn projected_iterator_advances_original_storage_and_clone_keeps_its_registration() {
    let int = |value| Value::from(rils_execution::value::native_i32(value));
    let range = rils_execution::value::native_range(int(1), int(4)).unwrap();
    let ty = Type::of_value(&range).unwrap();
    let context = TypedStorageContext::new(&[], &[]);
    let Value::Dynamic(tuple) = context
        .construct_tuple(
            &Type::Tuple(vec![ty, Type::String]),
            vec![
                range,
                Value::from(rils_execution::value::native_string("moved")),
            ],
        )
        .unwrap()
    else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(tuple.clone()).unwrap();
    drop(place.field("1").unwrap().take().unwrap());
    let mutable = Value::from_reference(Rc::new(
        place.field("0").unwrap().borrow(true, None).unwrap(),
    ));
    let alias = Value::from_reference(Rc::new(
        place.field("0").unwrap().borrow(true, None).unwrap(),
    ));
    let shared = Value::from_reference(Rc::new(
        place.field("0").unwrap().borrow(false, None).unwrap(),
    ));
    assert!(rils_execution::value::native_ops::is_iterator(&shared));
    assert!(rils_execution::value::native_ops::has_method(&shared, "Clone::clone").unwrap());
    assert!(!rils_execution::value::native_ops::has_method(&shared, "missing").unwrap());
    assert!(
        call("Iterator", "next", &shared)
            .unwrap_err()
            .contains("&mut self")
    );
    for (receiver, expected) in [(&mutable, 1), (&alias, 2)] {
        let output = call("Iterator", "next", receiver).unwrap();
        assert_eq!(
            rils_execution::value::dynamic_option::take_owned(output)
                .unwrap()
                .unwrap()
                .get_cloned::<i32>()
                .unwrap(),
            expected
        );
    }
    let cloned = call("Clone", "clone", &shared).unwrap();
    let Value::Native(cloned) = cloned else {
        panic!("registered native clone")
    };
    assert_eq!(
        rils_execution::value::native_ops::next(&cloned)
            .unwrap()
            .unwrap()
            .unwrap()
            .get_cloned::<i32>()
            .unwrap(),
        3
    );
    let original = call("Iterator", "next", &mutable).unwrap();
    assert_eq!(
        rils_execution::value::dynamic_option::take_owned(original)
            .unwrap()
            .unwrap()
            .get_cloned::<i32>()
            .unwrap(),
        3
    );
    assert!(
        rils_execution::value::dynamic_option::take_owned(
            call("Iterator", "next", &alias).unwrap()
        )
        .unwrap()
        .is_none()
    );
    assert!(Value::Dynamic(tuple).is_partially_moved());
}
