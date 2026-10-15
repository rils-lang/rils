use rils_execution::{
    RilsValue, Type, Value,
    environment::StorageSlot,
    runtime_builtins::{
        NativeOwnedContext, call_native_owned_symbol, call_native_symbol,
        requires_owned_native_call,
    },
    value::ReferenceValue,
};
use rils_value::{NativeObject, NativeType};
use std::{cell::RefCell, rc::Rc};
const KINDS: [&str; 4] = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"];
fn reference(value: Value) -> Value {
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(value);
    Value::from_reference(Rc::new(ReferenceValue::new_storage(
        Rc::new(RefCell::new(slot)),
        true,
    )))
}
fn symbol(kind: &str, method: &str) -> &'static str {
    rils_builtins::builtin_member(kind, method)
        .unwrap()
        .native_symbol
        .unwrap()
}
fn call(kind: &str, method: &str, args: Vec<Value>) -> Result<Value, String> {
    let symbol = symbol(kind, method);
    if requires_owned_native_call(symbol) {
        call_native_owned_symbol(symbol, args, &NativeOwnedContext::default()).unwrap()
    } else {
        call_native_symbol(symbol, &args).unwrap()
    }
}
fn next(iterator: &Value) -> Result<Value, String> {
    call_native_symbol(symbol("Iterator", "next"), std::slice::from_ref(iterator)).unwrap()
}
fn text(value: &str) -> Value {
    type Text = rils_stdlib::stdlib::string::String;
    Value::Native(
        NativeObject::new(
            Rc::new(NativeType::new::<Text>(Type::String)),
            Text::from(value.to_owned()),
        )
        .unwrap(),
    )
}
fn address(value: &Value) -> usize {
    RilsValue::new(value.clone())
        .with_ref::<String, _>(|value| value.as_ptr() as usize)
        .unwrap()
}
fn collection(kind: &str) -> (Value, Vec<usize>) {
    let key = text("key 🦀");
    let value = text("value 🦀");
    let addresses = if kind.ends_with("Map") {
        vec![address(&key), address(&value)]
    } else {
        vec![address(&key)]
    };

    let ty = Type::Named {
        name: kind.into(),
        arguments: if kind.ends_with("Map") {
            vec![Type::String, Type::String]
        } else {
            vec![Type::String]
        },
    };
    let source = NativeOwnedContext::default().empty_collection(&ty).unwrap();
    let receiver = reference(source.clone());
    let mut args = vec![receiver, key];
    if kind.ends_with("Map") {
        args.push(value);
    }
    call(kind, "insert", args).unwrap();
    (source, addresses)
}
fn insert_extra(kind: &str, receiver: &Value) -> Result<Value, String> {
    let mut args = vec![receiver.clone(), text("extra")];
    if kind.ends_with("Map") {
        args.push(text("extra value"));
    }
    call(kind, "insert", args)
}

#[test]
fn native_borrowed_outputs_keep_native_layouts_addresses_and_leases() {
    for kind in KINDS {
        {
            let (source, addresses) = collection(kind);
            let receiver = reference(source);
            let iterator = reference(call(kind, "iter", vec![receiver.clone()]).unwrap());
            let result = next(&iterator).unwrap();
            let reference_type = Type::Reference {
                mutable: false,
                inner: Box::new(Type::String),
            };
            let item_type = if kind.ends_with("Map") {
                Type::Tuple(vec![reference_type.clone(), reference_type.clone()])
            } else {
                reference_type.clone()
            };
            let expected = Type::Option(Box::new(item_type));
            assert_eq!(Type::of_value(&result), Some(expected.clone()));
            assert!(expected.accepts(&result));
            let Value::Dynamic(object) = &result else {
                panic!("native Option required")
            };
            object
                .with(|payload| {
                    for (index, expected) in addresses.iter().enumerate() {
                        let item = payload.view().option_item().unwrap();
                        let view = if kind.ends_with("Map") {
                            item.field(index).unwrap()
                        } else {
                            item
                        };
                        view.with_rust::<Rc<ReferenceValue>, _>(|reference| {
                            let reference = Value::from_reference(reference.clone());
                            assert_eq!(Type::of_value(&reference), Some(reference_type.clone()));
                            assert!(reference_type.accepts(&reference));
                            assert_eq!(address(&reference), *expected);
                        })
                        .unwrap();
                    }
                })
                .unwrap();
            let none = next(&iterator).unwrap();
            assert_eq!(Type::of_value(&none), Some(expected));
            let Value::Dynamic(none) = none else {
                panic!("native None required")
            };
            assert!(
                !none
                    .with(|value| value.view().option_is_some())
                    .unwrap()
                    .unwrap()
            );
            let copied = result.clone_owned().unwrap(); // Copy the reference handles, not the referents.
            drop(result);
            drop(iterator);
            assert!(insert_extra(kind, &receiver).is_err());
            drop(copied);
            insert_extra(kind, &receiver).unwrap();
        }
    }
}

#[test]
fn native_tuple_constructor_keeps_small_field_places_stable_and_moves_owned_fields() {
    use rils_execution::value::{
        native_instance::NativeInstancePlace, storage::TypedStorageContext,
    };
    let context = TypedStorageContext::new(&[], &[]);
    let small_type = Type::Tuple(vec![Type::I32]);
    let small = context
        .construct_tuple(
            &small_type,
            vec![Value::from(rils_execution::value::native_i32(1))],
        )
        .unwrap();
    let independent = small.clone_owned().unwrap();
    let Value::Dynamic(small) = small else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(small.clone())
        .unwrap()
        .field("0")
        .unwrap();
    place
        .assign(Value::from(rils_execution::value::native_i32(2)))
        .unwrap();
    assert_eq!(
        place.take().unwrap(),
        Value::from(rils_execution::value::native_i32(2))
    );
    let Value::Dynamic(independent) = independent else {
        panic!("native tuple copy")
    };
    assert_eq!(
        NativeInstancePlace::new(independent)
            .unwrap()
            .field("0")
            .unwrap()
            .take()
            .unwrap(),
        Value::from(rils_execution::value::native_i32(1))
    );
    let item = text("moved without Clone");
    let original = address(&item);
    let value = context
        .construct_tuple(&Type::Tuple(vec![Type::String]), vec![item])
        .unwrap();
    let Value::Dynamic(value) = value else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(value).unwrap().field("0").unwrap();
    let moved = place.take().unwrap();
    assert_eq!(address(&moved), original);
    assert!(place.take().is_err());
    assert!(context.construct_tuple(&small_type, vec![]).is_err());
    assert!(
        context
            .construct_tuple(
                &Type::Tuple(vec![Type::Unknown]),
                vec![Value::from(rils_execution::value::native_unit())]
            )
            .is_err()
    );
}
