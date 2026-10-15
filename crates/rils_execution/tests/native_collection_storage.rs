use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::{
        NativeOwnedContext, call_native_constructor_symbol, call_native_owned_symbol,
        call_native_symbol, call_native_symbol_with_callback, requires_owned_native_call,
    },
    value::ReferenceValue,
};

const COLLECTIONS: [&str; 6] = [
    "VecDeque",
    "BinaryHeap",
    "HashMap",
    "BTreeMap",
    "HashSet",
    "BTreeSet",
];

fn symbol(owner: &str, method: &str) -> &'static str {
    rils_builtins::builtin_member(owner, method)
        .unwrap()
        .native_symbol
        .unwrap()
}

fn ty(owner: &str, item: Type) -> Type {
    Type::Named {
        name: owner.into(),
        arguments: if owner.ends_with("Map") {
            vec![item, Type::I32]
        } else {
            vec![item]
        },
    }
}

fn insert(owner: &str, receiver: Value, item: Value) -> Result<Value, String> {
    let method = match owner {
        "VecDeque" => "push_back",
        "BinaryHeap" => "push",
        _ => "insert",
    };
    let mut args = vec![receiver, item];
    if owner.ends_with("Map") {
        args.push(Value::from(rils_execution::value::native_i32(42)));
    }
    call_native_owned_symbol(symbol(owner, method), args, &NativeOwnedContext::default()).unwrap()
}

#[test]
fn all_collection_constructors_require_concrete_native_layouts() {
    let context = NativeOwnedContext::default();
    for owner in COLLECTIONS {
        assert!(
            call_native_symbol(symbol(owner, "new"), &[])
                .unwrap()
                .is_err(),
            "{owner}"
        );
        assert!(
            context.empty_collection(&ty(owner, Type::Unknown)).is_err(),
            "{owner}"
        );
        let value = context.empty_collection(&ty(owner, Type::I32)).unwrap();
        let Value::Dynamic(object) = value else {
            panic!("{owner} must use native storage")
        };
        assert_eq!(
            object.descriptor().layout().rils_type(),
            &ty(owner, Type::I32)
        );
        assert_eq!(object.with(|payload| payload.sequence_len()), Ok(Ok(0)));
    }
}

#[test]
fn typed_constructor_dispatch_uses_the_same_result_witness_for_all_collections() {
    let context = NativeOwnedContext::default();
    for owner in std::iter::once("Vec").chain(COLLECTIONS) {
        let symbol = symbol(owner, "new");
        let expected = ty(owner, Type::I32);
        assert!(requires_owned_native_call(symbol), "{owner}");
        for via_callback in [false, true] {
            let value = if via_callback {
                call_native_symbol_with_callback::<String>(
                    symbol,
                    Vec::new(),
                    &context,
                    Some(&expected),
                    &mut |_, _| panic!("constructors do not invoke callbacks"),
                )
                .unwrap()
                .unwrap()
            } else {
                call_native_constructor_symbol(symbol, Vec::new(), &context, Some(&expected))
                    .unwrap()
                    .unwrap()
            };
            assert_eq!(Type::of_value(&value), Some(expected.clone()), "{owner}");
            assert_eq!(
                value.with_native_view(|view| view.sequence_len()).unwrap(),
                Ok(0),
                "{owner}"
            );
        }
        assert!(
            call_native_owned_symbol(symbol, Vec::new(), &context)
                .unwrap()
                .unwrap_err()
                .contains("cannot infer"),
            "{owner}"
        );
        for invalid in [
            Type::I32,
            ty(owner, Type::Unknown),
            Type::named("Unrelated"),
        ] {
            assert!(
                call_native_constructor_symbol(symbol, Vec::new(), &context, Some(&invalid))
                    .unwrap()
                    .is_err(),
                "{owner}: {invalid}"
            );
        }
        assert!(
            call_native_constructor_symbol(
                symbol,
                vec![Value::from(rils_execution::value::native_i32(1))],
                &context,
                Some(&expected),
            )
            .unwrap()
            .is_err(),
            "{owner}"
        );
    }
}

#[test]
fn array_constructor_infers_missing_types_and_rejects_incompatible_partial_types() {
    let context = NativeOwnedContext::default();
    let actual = ty("Vec", Type::I32);
    for (expected, compatible) in [
        (None, true),
        (Some(Type::Unknown), true),
        (Some(ty("Vec", Type::Unknown)), true),
        (Some(ty("VecDeque", Type::Unknown)), false),
        (
            Some(ty("Vec", Type::Option(Box::new(Type::Unknown)))),
            false,
        ),
        (Some(actual.clone()), true),
        (Some(ty("Vec", Type::String)), false),
    ] {
        let array = context
            .storage()
            .construct_array(
                &Type::Array {
                    element: Box::new(Type::I32),
                    length: 2,
                },
                vec![
                    rils_execution::value::native_i32(3).into(),
                    rils_execution::value::native_i32(7).into(),
                ],
            )
            .unwrap();
        let result = call_native_constructor_symbol(
            symbol("Vec", "from"),
            vec![array],
            &context,
            expected.as_ref(),
        )
        .unwrap();
        assert_eq!(
            result.is_ok(),
            compatible,
            "expected {expected:?}: {result:?}"
        );
        if let Ok(value) = result {
            assert_eq!(Type::of_value(&value), Some(actual.clone()));
            assert_eq!(
                value.with_native_view(|view| view.sequence_len()).unwrap(),
                Ok(2)
            );
        }
    }
}

#[test]
fn collection_mutations_preserve_storage_on_type_and_borrow_errors() {
    for owner in COLLECTIONS {
        let value = NativeOwnedContext::default()
            .empty_collection(&ty(owner, Type::I32))
            .unwrap();
        let Value::Dynamic(object) = &value else {
            unreachable!()
        };
        let object = object.clone();
        let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        slot.borrow_mut().initialize(value);
        let receiver =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        insert(
            owner,
            receiver.clone(),
            Value::from(rils_execution::value::native_i32(1)),
        )
        .unwrap();
        assert!(
            insert(
                owner,
                receiver.clone(),
                Value::from(rils_execution::value::native_string("wrong type"))
            )
            .is_err(),
            "{owner}"
        );
        let shared =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
        assert!(
            insert(
                owner,
                shared,
                Value::from(rils_execution::value::native_i32(2))
            )
            .is_err(),
            "{owner}"
        );
        let reference =
            ReferenceValue::new_guarded_dynamic_indexed_element(object.clone(), 0, false, None)
                .unwrap();
        let before = slot.borrow().read().unwrap().to_string();
        assert!(
            insert(
                owner,
                receiver.clone(),
                Value::from(rils_execution::value::native_i32(2))
            )
            .is_err(),
            "{owner}"
        );
        assert!(
            call_native_symbol(symbol(owner, "clear"), std::slice::from_ref(&receiver))
                .unwrap()
                .is_err(),
            "{owner}"
        );
        assert_eq!(slot.borrow().read().unwrap().to_string(), before, "{owner}");
        assert_eq!(
            object.with(|payload| payload.sequence_len()),
            Ok(Ok(1)),
            "{owner}"
        );
        drop(reference);
        call_native_symbol(symbol(owner, "clear"), &[receiver])
            .unwrap()
            .unwrap();
        assert_eq!(
            object.with(|payload| payload.sequence_len()),
            Ok(Ok(0)),
            "{owner}"
        );
    }
}
