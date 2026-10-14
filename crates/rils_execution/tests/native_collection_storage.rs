use std::{cell::RefCell, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
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
