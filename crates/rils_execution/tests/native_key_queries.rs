use rils_execution::{
    IntegerType, Type, Value,
    environment::StorageSlot,
    runtime_builtins::call_native_symbol,
    value::{ReferenceValue, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver},
};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};
use std::{cell::RefCell, rc::Rc};

fn wrap(payload: DynamicValue) -> Value {
    Value::Dynamic(
        DynamicObject::new(Rc::new(DynamicType::new(payload.layout_handle())), payload).unwrap(),
    )
}
fn reference(value: Value) -> (Value, Rc<RefCell<StorageSlot>>) {
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(value);
    let slot = Rc::new(RefCell::new(slot));
    (
        Value::Reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true))),
        slot,
    )
}
fn collection(kind: &str, ty: &Type, keys: Vec<Value>) -> Value {
    let mut codec = NativeRecordCodec::new();
    let key = RecordLayoutResolver::new(&[]).resolve(ty).unwrap();
    let map = kind.ends_with("Map");
    let item = if map {
        DynamicLayout::aggregate(
            Type::Tuple(vec![ty.clone(), Type::Bool]),
            vec![
                ("0".into(), key.clone()),
                ("1".into(), DynamicLayout::copy_of::<bool>(Type::Bool)),
            ],
        )
        .unwrap()
    } else {
        key.clone()
    };
    let entries = keys
        .into_iter()
        .map(|value| {
            let key = codec.into_native(value, key.clone()).unwrap();
            if map {
                DynamicValue::record(
                    item.clone(),
                    vec![
                        key,
                        DynamicValue::from_rust(DynamicLayout::copy_of::<bool>(Type::Bool), true)
                            .unwrap(),
                    ],
                )
                .unwrap()
            } else {
                key
            }
        })
        .collect();
    let layout = DynamicLayout::sequence(
        Type::Named {
            name: kind.into(),
            arguments: if map {
                vec![ty.clone(), Type::Bool]
            } else {
                vec![ty.clone()]
            },
        },
        item,
    );
    wrap(DynamicValue::sequence(layout, entries).unwrap())
}
fn call(kind: &str, method: &str, arguments: &[Value]) -> Result<Value, String> {
    let symbol = rils_builtins::builtin(kind)
        .unwrap()
        .member(method)
        .unwrap()
        .native_symbol
        .unwrap();
    call_native_symbol(symbol, arguments).unwrap()
}
fn contains(kind: &str, receiver: &Value, key: Value) -> Result<Value, String> {
    let method = if kind.ends_with("Map") {
        "contains_key"
    } else {
        "contains"
    };
    call(kind, method, &[receiver.clone(), key])
}

#[test]
fn collection_queries_read_scalar_wrappers_and_reject_other_integer_widths() {
    for kind in ["HashMap", "HashSet", "BTreeMap", "BTreeSet"] {
        for (ty, stored, query, absent, wrong) in [
            (
                Type::I32,
                Value::from_i32(i32::MIN),
                Value::from_i32(i32::MIN),
                Value::from_i32(i32::MAX),
                Value::from_i64(i32::MIN.into()),
            ),
            (
                Type::Integer(IntegerType::U128),
                Value::from_u128(u128::MAX),
                Value::U128(u128::MAX),
                Value::from_u128(0),
                Value::from_u64(u64::MAX),
            ),
            (
                Type::String,
                Value::from_string("键🦀"),
                Value::from_string("键🦀"),
                Value::from_string("other"),
                Value::from_char('键'),
            ),
        ] {
            let (receiver, _) = reference(collection(kind, &ty, vec![stored]));
            let (query, source) = reference(query);
            // Shared inspection must work even while the source slot is borrowed.
            let reading = source.borrow();
            assert_eq!(
                contains(kind, &receiver, query.clone()),
                Ok(Value::Bool(true)),
                "{kind} {ty}"
            );
            drop(reading);
            assert_eq!(
                contains(kind, &receiver, reference(absent).0),
                Ok(Value::Bool(false))
            );
            assert!(contains(kind, &receiver, reference(wrong).0).is_err());
            if kind.ends_with("Map") {
                let result = call(kind, "get_cloned", &[receiver.clone(), query.clone()]).unwrap();
                assert_eq!(result.as_option().unwrap().0, Some(Value::Bool(true)));
            }
            let removed = call(kind, "remove", &[receiver.clone(), query.clone()]).unwrap();
            if kind.ends_with("Map") {
                assert_eq!(removed.as_option().unwrap().0, Some(Value::Bool(true)));
            } else {
                assert_eq!(removed, Value::Bool(true));
            }
            assert_eq!(contains(kind, &receiver, query), Ok(Value::Bool(false)));
            assert_eq!(call(kind, "len", &[receiver]), Ok(Value::from_usize(0)));
        }
    }
}

fn some(text: &str) -> Value {
    Value::Option {
        value: Some(Rc::new(Value::from_string(text))),
        element_type: Some(Type::String),
    }
}
#[test]
fn compatibility_and_native_sum_queries_share_identity_without_consuming_sources() {
    let ty = Type::Option(Box::new(Type::String));
    for kind in ["HashMap", "HashSet"] {
        let (receiver, _) = reference(collection(
            kind,
            &ty,
            vec![
                some("owned"),
                Value::Option {
                    value: None,
                    element_type: Some(Type::String),
                },
            ],
        ));
        for item in [
            some("owned"),
            Value::Option {
                value: None,
                element_type: None,
            },
        ] {
            let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
            let native = wrap(
                NativeRecordCodec::new()
                    .into_native(item.clone_owned().unwrap(), layout)
                    .unwrap(),
            );
            for item in [item, native] {
                let (query, slot) = reference(item);
                assert_eq!(contains(kind, &receiver, query), Ok(Value::Bool(true)));
                assert!(slot.borrow_mut().take().is_ok());
            }
        }
        let bad = Value::Option {
            value: None,
            element_type: Some(Type::I32),
        };
        assert!(contains(kind, &receiver, reference(bad).0).is_err());
        assert_eq!(call(kind, "len", &[receiver]), Ok(Value::from_usize(2)));
    }
}

#[test]
fn projected_keys_keep_their_native_owner_and_conflicting_access_is_reported() {
    let source = collection(
        "HashMap",
        &Type::String,
        vec![Value::from_string("projected")],
    );
    let Value::Dynamic(object) = source else {
        unreachable!()
    };
    let key = Value::Reference(Rc::new(
        ReferenceValue::new_guarded_dynamic_indexed_field(object.clone(), 0, Some(0), false, None)
            .unwrap(),
    ));
    let (receiver, _) = reference(collection(
        "HashSet",
        &Type::String,
        vec![Value::from_string("projected")],
    ));
    assert_eq!(
        contains("HashSet", &receiver, key.clone()),
        Ok(Value::Bool(true))
    );
    assert_eq!(
        object
            .with(|payload| payload.sequence_len())
            .unwrap()
            .unwrap(),
        1
    );
    let (query, slot) = reference(Value::from_string("projected"));
    {
        let _writing = slot.borrow_mut();
        assert!(
            contains("HashSet", &receiver, query.clone())
                .unwrap_err()
                .contains("mutably accessed")
        );
    }
    assert_eq!(contains("HashSet", &receiver, query), Ok(Value::Bool(true)));
    assert!(
        object
            .with_mut(|payload| payload.clear_sequence())
            .unwrap()
            .is_err()
    );
    drop(key);
    object
        .with_mut(|payload| payload.clear_sequence())
        .unwrap()
        .unwrap();
}

#[test]
fn nominal_keys_require_eq_and_hash_but_never_gain_copy_implicitly() {
    use rils_execution::value::{StructType, native_instance};
    let definition = Rc::new(StructType {
        field_indices: Default::default(),
        name: "Key".into(),
        opaque_native: false,
        generic_parameters: vec![],
        fields: vec![],
        methods: Default::default(),
        trait_methods: Default::default(),
        implemented_traits: Default::default(),
        associated_types: Default::default(),
    });
    let codec = Rc::new(NativeRecordCodec::with_definitions(
        std::slice::from_ref(&definition),
        &[],
    ));
    let layout = RecordLayoutResolver::new(std::slice::from_ref(&definition))
        .resolve(&Type::named("Key"))
        .unwrap();
    let make = || {
        native_instance::from_native(
            DynamicValue::record(layout.clone(), vec![]).unwrap(),
            codec.clone(),
        )
        .unwrap()
    };
    let (receiver, _) = reference(collection("HashSet", &Type::I32, vec![]));
    assert!(contains("HashSet", &receiver, reference(make()).0).is_err());
    let set = DynamicLayout::sequence(
        Type::Named {
            name: "HashSet".into(),
            arguments: vec![Type::named("Key")],
        },
        layout.clone(),
    );
    let (receiver, _) = reference(wrap(
        DynamicValue::sequence(
            set,
            vec![DynamicValue::record(layout.clone(), vec![]).unwrap()],
        )
        .unwrap(),
    ));
    for traits in [vec![], vec!["Eq"], vec!["Hash"], vec!["Eq", "Hash"]] {
        *definition.implemented_traits.borrow_mut() =
            traits.iter().map(|name| (*name).to_owned()).collect();
        let (query, slot) = reference(make());
        let reading = slot.borrow();
        let found = contains("HashSet", &receiver, query.clone());
        if traits.len() == 2 {
            assert_eq!(found, Ok(Value::Bool(true)));
        } else {
            assert!(found.unwrap_err().contains("Eq + Hash"));
        }
        drop(reading);
        drop(query);
        assert!(!slot.borrow_mut().take().unwrap().is_copy());
    }
}

#[test]
fn compatibility_array_queries_report_borrow_conflicts_and_moved_fields() {
    use rils_execution::value::{FieldSlot, IndexedStorage};
    let array = Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(Type::I32, Value::from_i32(42))]),
        element_type: RefCell::new(Some(Type::I32)),
        active_iterators: Default::default(),
    });
    let ty = Type::Array {
        element: Box::new(Type::I32),
        length: 1,
    };
    let (receiver, _) = reference(collection(
        "HashSet",
        &ty,
        vec![Value::Array(array.clone()).clone_owned().unwrap()],
    ));
    let (query, _) = reference(Value::Array(array.clone()));
    {
        let _writing = array.elements.borrow_mut();
        assert!(
            contains("HashSet", &receiver, query.clone())
                .unwrap_err()
                .contains("mutably accessed")
        );
    }
    assert_eq!(
        contains("HashSet", &receiver, query.clone()),
        Ok(Value::Bool(true))
    );
    array.elements.borrow_mut()[0].value = None;
    assert!(
        contains("HashSet", &receiver, query)
            .unwrap_err()
            .contains("moved")
    );
    assert_eq!(
        call("HashSet", "len", &[receiver]),
        Ok(Value::from_usize(1))
    );
}
