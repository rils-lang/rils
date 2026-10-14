use std::{cell::Cell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    value::{
        NativeObject, NativeType, native_instance::NativeInstancePlace,
        storage::TypedStorageContext,
    },
};

fn optional_string() -> Value {
    TypedStorageContext::new(&[], &[])
        .construct_option(
            &Type::Option(Box::new(Type::String)),
            Some(Value::from(rils_execution::value::native_string("owned"))),
        )
        .unwrap()
}

fn tuple(value: Value) -> Value {
    TypedStorageContext::new(&[], &[])
        .construct_tuple(
            &Type::Tuple(vec![Type::of_value(&value).unwrap()]),
            vec![value],
        )
        .unwrap()
}

fn place(value: &Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("indexed aggregate must use native storage")
    };
    NativeInstancePlace::new(object.clone()).unwrap()
}

#[test]
fn arrays_retain_sum_layouts_including_empty_and_inactive_branches() {
    let context = TypedStorageContext::new(&[], &[]);
    let item = Type::Result(Box::new(Type::I32), Box::new(Type::String));
    for length in [0, 1, 3] {
        let ty = Type::Array {
            element: Box::new(item.clone()),
            length,
        };
        let fields = (0..length)
            .map(|_| {
                context
                    .construct_result(
                        &item,
                        Err(Value::from(rils_execution::value::native_string("owned"))),
                    )
                    .unwrap()
            })
            .collect();
        let value = context.construct_array(&ty, fields).unwrap();
        assert_eq!(Type::of_value(&value), Some(ty));
        assert!(!value.is_copy());
        let Value::Dynamic(object) = value else {
            panic!("native array expected")
        };
        let payload = object
            .into_value()
            .map_err(|failure| failure.1)
            .expect("array owns its bytes");
        for index in 0..length {
            assert_eq!(
                payload
                    .view()
                    .field(index)
                    .unwrap()
                    .layout()
                    .unwrap()
                    .rils_type(),
                &item
            );
        }
    }
}

#[test]
fn native_tuple_partial_move_and_replacement_preserve_field_declarations() {
    let value = tuple(optional_string());
    let field = place(&value).field("0").unwrap();
    let taken = field.take().unwrap();
    assert!(value.is_partially_moved());
    assert!(field.take().is_err());
    assert!(
        field
            .assign(Value::from(rils_execution::value::native_i32(1)))
            .is_err()
    );
    assert!(value.is_partially_moved());
    field.assign(optional_string()).unwrap();
    assert!(!value.is_partially_moved());
    assert_eq!(value.to_string(), "(Some(owned),)");
    let Value::Dynamic(taken) = taken else {
        panic!("native Option expected")
    };
    assert!(
        taken.into_value().is_ok(),
        "field move must retain unique ownership"
    );
}

#[test]
fn native_tuple_references_reject_moves_and_replacements_before_mutation() {
    let value = tuple(optional_string());
    let root = place(&value);
    let field = root.field("0").unwrap();
    let reference = field.borrow(false, None).unwrap();
    assert!(root.take().is_err());
    assert!(field.take().is_err());
    assert!(field.assign(optional_string()).is_err());
    assert_eq!(value.to_string(), "(Some(owned),)");
    drop(reference);
    assert_eq!(field.take().unwrap().to_string(), "Some(owned)");
}

#[test]
fn native_declaration_outlives_move_without_retaining_payload() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let dropped = Rc::new(Cell::new(0));
    let descriptor = Rc::new(NativeType::new::<Probe>(Type::named("Probe")));
    let make = || {
        tuple(Value::Native(
            NativeObject::new(descriptor.clone(), Probe(dropped.clone())).unwrap(),
        ))
    };
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(make());
    drop(slot.take().unwrap());
    assert_eq!(
        dropped.get(),
        1,
        "declaration must not retain moved payload"
    );
    slot.assign(make()).unwrap();
    assert!(
        slot.assign(tuple(Value::from(rils_execution::value::native_i32(1))))
            .is_err()
    );
    assert_eq!(
        dropped.get(),
        1,
        "failed assignment must preserve destination"
    );
    drop(slot.take().unwrap());
    assert_eq!(dropped.get(), 2);
}
