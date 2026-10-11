#![allow(clippy::mutable_key_type)]
use rils_execution::{
    RilsValue, Type, Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{
        BTreeMapValue, BTreeSetValue, FieldSlot, HashKey, HashMapValue, HashSetValue,
        ReferenceValue,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const KINDS: [&str; 4] = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"];
fn reference(value: Value) -> Value {
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(value);
    Value::Reference(Rc::new(ReferenceValue::new_storage(
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
fn owned(kind: &str, method: &str, args: Vec<Value>) -> Result<Value, String> {
    call_native_owned_symbol(symbol(kind, method), args, &NativeOwnedContext::default()).unwrap()
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
fn collection(kind: &str, native: bool) -> (Value, Vec<usize>) {
    let key = text("key 🦀");
    let value = text("value 🦀");
    let addresses = if kind.ends_with("Map") {
        vec![address(&key), address(&value)]
    } else {
        vec![address(&key)]
    };
    if native {
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
        owned(kind, "insert", args).unwrap();
        return (source, addresses);
    }
    let key = HashKey::from_owned_value(key, None, kind.starts_with("BTree")).unwrap();
    let source = match kind {
        "HashMap" => Value::HashMap(Rc::new(HashMapValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            key_type: RefCell::new(Type::String),
            value_type: RefCell::new(Type::String),
            entries: RefCell::new([(key, FieldSlot::new(Type::String, value))].into()),
        })),
        "BTreeMap" => Value::BTreeMap(Rc::new(BTreeMapValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            key_type: RefCell::new(Type::String),
            value_type: RefCell::new(Type::String),
            entries: RefCell::new([(key, FieldSlot::new(Type::String, value))].into()),
        })),
        "HashSet" => Value::HashSet(Rc::new(HashSetValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            element_type: RefCell::new(Type::String),
            entries: RefCell::new([key].into()),
        })),
        "BTreeSet" => Value::BTreeSet(Rc::new(BTreeSetValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            element_type: RefCell::new(Type::String),
            entries: RefCell::new([key].into()),
        })),
        _ => unreachable!(),
    };
    (source, addresses)
}
fn insert_extra(kind: &str, receiver: &Value) -> Result<Value, String> {
    let mut args = vec![receiver.clone(), text("extra")];
    if kind.ends_with("Map") {
        args.push(text("extra value"));
    }
    owned(kind, "insert", args)
}

#[test]
fn native_and_compatibility_borrowed_outputs_keep_native_layouts_addresses_and_leases() {
    for kind in KINDS {
        for native in [false, true] {
            let (source, addresses) = collection(kind, native);
            let receiver = reference(source);
            let iterator = reference(owned(kind, "iter", vec![receiver.clone()]).unwrap());
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
                            let reference = Value::Reference(reference.clone());
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
fn missing_item_witness_fails_before_advancing_or_returning_none() {
    for kind in KINDS {
        let (source, _) = collection(kind, false);
        let receiver = reference(source);
        let mut iterator = owned(kind, "iter", vec![receiver]).unwrap();
        match &mut iterator {
            Value::BorrowedMapIterator(iter) => Rc::get_mut(iter).unwrap().key_type = Type::Unknown,
            Value::BorrowedSetIterator(iter) => {
                Rc::get_mut(iter).unwrap().element_type = Type::Unknown
            }
            _ => unreachable!(),
        }
        let observer = iterator.clone();
        let iterator = reference(iterator);
        for _ in 0..2 {
            assert!(
                next(&iterator)
                    .unwrap_err()
                    .contains("complete borrowed iterator item type")
            );
        }
        let index = match &observer {
            Value::BorrowedMapIterator(iter) => &iter.index,
            Value::BorrowedSetIterator(iter) => &iter.index,
            _ => unreachable!(),
        };
        assert_eq!(index.get(), 0);
        index.set(1); // Even an exhausted iterator needs a complete None witness.
        assert!(
            next(&iterator)
                .unwrap_err()
                .contains("complete borrowed iterator item type")
        );
        assert_eq!(index.get(), 1);
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
        .construct_tuple(&small_type, vec![Value::from_i32(1)])
        .unwrap();
    let independent = small.clone_owned().unwrap();
    let Value::Dynamic(small) = small else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(small.clone())
        .unwrap()
        .field("0")
        .unwrap();
    place.assign(Value::from_i32(2)).unwrap();
    assert_eq!(place.take().unwrap(), Value::from_i32(2));
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
        Value::from_i32(1)
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
            .construct_tuple(&Type::Tuple(vec![Type::Unknown]), vec![Value::Unit])
            .is_err()
    );
}
