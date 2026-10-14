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
        Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true))),
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
                Value::from(rils_execution::value::native_i32(i32::MIN)),
                Value::from(rils_execution::value::native_i32(i32::MIN)),
                Value::from(rils_execution::value::native_i32(i32::MAX)),
                Value::from(rils_execution::value::native_i64(i32::MIN.into())),
            ),
            (
                Type::Integer(IntegerType::U128),
                Value::from(rils_execution::value::native_u128(u128::MAX)),
                Value::from(rils_execution::value::native_u128(u128::MAX)),
                Value::from(rils_execution::value::native_u128(0)),
                Value::from(rils_execution::value::native_u64(u64::MAX)),
            ),
            (
                Type::String,
                Value::from(rils_execution::value::native_string("键🦀")),
                Value::from(rils_execution::value::native_string("键🦀")),
                Value::from(rils_execution::value::native_string("other")),
                Value::from(rils_execution::value::native_char('键')),
            ),
        ] {
            let (receiver, _) = reference(collection(kind, &ty, vec![stored]));
            let (query, source) = reference(query);
            // Shared inspection must work even while the source slot is borrowed.
            let reading = source.borrow();
            assert_eq!(
                contains(kind, &receiver, query.clone()),
                Ok(Value::from(rils_execution::value::native_bool(true))),
                "{kind} {ty}"
            );
            drop(reading);
            assert_eq!(
                contains(kind, &receiver, reference(absent).0),
                Ok(Value::from(rils_execution::value::native_bool(false)))
            );
            assert!(contains(kind, &receiver, reference(wrong).0).is_err());
            if kind.ends_with("Map") {
                let result = call(kind, "get_cloned", &[receiver.clone(), query.clone()]).unwrap();
                assert_eq!(
                    rils_execution::value::dynamic_option::take_owned(result).unwrap(),
                    Some(Value::from(rils_execution::value::native_bool(true)))
                );
            }
            let removed = call(kind, "remove", &[receiver.clone(), query.clone()]).unwrap();
            if kind.ends_with("Map") {
                assert_eq!(
                    rils_execution::value::dynamic_option::take_owned(removed).unwrap(),
                    Some(Value::from(rils_execution::value::native_bool(true)))
                );
            } else {
                assert_eq!(
                    removed,
                    Value::from(rils_execution::value::native_bool(true))
                );
            }
            assert_eq!(
                contains(kind, &receiver, query),
                Ok(Value::from(rils_execution::value::native_bool(false)))
            );
            assert_eq!(
                call(kind, "len", &[receiver]),
                Ok(Value::from(rils_execution::value::native_usize(0)))
            );
        }
    }
}

#[test]
fn all_native_collections_query_keys_without_clone_registration() {
    use rils_execution::value::{NativeObject, NativeType};
    type Text = rils_stdlib::stdlib::string::String;
    for kind in ["HashMap", "HashSet", "BTreeMap", "BTreeSet"] {
        let (receiver, _) = reference(collection(
            kind,
            &Type::String,
            vec![Value::from(rils_execution::value::native_string("key 🦀"))],
        ));
        let query = Value::from(
            NativeObject::new(
                Rc::new(NativeType::new::<Text>(Type::String)),
                Text::from("key 🦀".to_owned()),
            )
            .unwrap(),
        );
        assert!(query.clone_owned().is_err());
        let (query, source) = reference(query);
        let guard = source.borrow();
        assert_eq!(
            contains(kind, &receiver, query.clone()),
            Ok(Value::from(rils_execution::value::native_bool(true)))
        );
        drop(guard);
        call(kind, "remove", &[receiver.clone(), query.clone()]).unwrap();
        assert_eq!(
            contains(kind, &receiver, query.clone()),
            Ok(Value::from(rils_execution::value::native_bool(false)))
        );
        assert_eq!(
            query.with_ref::<String, _>(|text| text.clone()).unwrap(),
            "key 🦀"
        );
    }
}

#[test]
fn native_collection_scalar_key_matrix() {
    use rils_execution::value::*;
    let values = [
        Value::from(native_i8(i8::MIN)),
        Value::from(native_i16(i16::MIN)),
        Value::from(native_i32(i32::MIN)),
        Value::from(native_i64(i64::MIN)),
        Value::from(native_i128(i128::MIN)),
        Value::from(native_isize(isize::MIN)),
        Value::from(native_u8(u8::MAX)),
        Value::from(native_u16(u16::MAX)),
        Value::from(native_u32(u32::MAX)),
        Value::from(native_u64(u64::MAX)),
        Value::from(native_u128(u128::MAX)),
        Value::from(native_usize(usize::MAX)),
        Value::from(native_bool(false)),
        Value::from(native_char('🦀')),
        Value::from(native_string("identity")),
    ];
    for value in values {
        let ty = Type::of_value(&value).unwrap();
        for kind in ["HashMap", "HashSet", "BTreeMap", "BTreeSet"] {
            let (receiver, _) =
                reference(collection(kind, &ty, vec![value.clone_owned().unwrap()]));
            assert_eq!(
                contains(kind, &receiver, reference(value.clone_owned().unwrap()).0),
                Ok(Value::from(native_bool(true))),
                "{kind} {ty}"
            );
        }
    }
}

fn option(ty: Type, text: Option<&str>) -> Value {
    rils_execution::value::storage::TypedStorageContext::new(&[], &[])
        .construct_option(
            &Type::Option(Box::new(ty)),
            text.map(|text| Value::from(rils_execution::value::native_string(text))),
        )
        .unwrap()
}

#[test]
fn native_sum_queries_preserve_sources_and_inactive_generic_types() {
    let ty = Type::Option(Box::new(Type::String));
    for kind in ["HashMap", "HashSet"] {
        let (receiver, _) = reference(collection(
            kind,
            &ty,
            vec![
                option(Type::String, Some("owned")),
                option(Type::String, None),
            ],
        ));
        for text in [Some("owned"), None] {
            let (query, slot) = reference(option(Type::String, text));
            assert_eq!(
                contains(kind, &receiver, query),
                Ok(Value::from(rils_execution::value::native_bool(true)))
            );
            assert!(slot.borrow_mut().take().is_ok());
        }
        assert!(contains(kind, &receiver, reference(option(Type::I32, None)).0).is_err());
        assert_eq!(
            call(kind, "len", &[receiver]),
            Ok(Value::from(rils_execution::value::native_usize(2)))
        );
    }
}

#[test]
fn projected_keys_keep_their_native_owner_and_conflicting_access_is_reported() {
    let source = collection(
        "HashMap",
        &Type::String,
        vec![Value::from(rils_execution::value::native_string(
            "projected",
        ))],
    );
    let Value::Dynamic(object) = source else {
        unreachable!()
    };
    let key = Value::from_reference(Rc::new(
        ReferenceValue::new_guarded_dynamic_indexed_field(object.clone(), 0, Some(0), false, None)
            .unwrap(),
    ));
    let (receiver, _) = reference(collection(
        "HashSet",
        &Type::String,
        vec![Value::from(rils_execution::value::native_string(
            "projected",
        ))],
    ));
    assert_eq!(
        contains("HashSet", &receiver, key.clone()),
        Ok(Value::from(rils_execution::value::native_bool(true)))
    );
    assert_eq!(
        object
            .with(|payload| payload.sequence_len())
            .unwrap()
            .unwrap(),
        1
    );
    let (query, slot) = reference(Value::from(rils_execution::value::native_string(
        "projected",
    )));
    {
        let _writing = slot.borrow_mut();
        assert!(
            contains("HashSet", &receiver, query.clone())
                .unwrap_err()
                .contains("mutably accessed")
        );
    }
    assert_eq!(
        contains("HashSet", &receiver, query),
        Ok(Value::from(rils_execution::value::native_bool(true)))
    );
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
            assert_eq!(
                found,
                Ok(Value::from(rils_execution::value::native_bool(true)))
            );
        } else {
            assert!(found.unwrap_err().contains("Eq + Hash"));
        }
        drop(reading);
        drop(query);
        assert!(!slot.borrow_mut().take().unwrap().is_copy());
    }
}

#[test]
fn native_array_queries_report_borrow_conflicts_and_moved_fields() {
    use rils_execution::value::{
        native_instance::NativeInstancePlace, storage::TypedStorageContext,
    };
    let ty = Type::Array {
        element: Box::new(Type::String),
        length: 1,
    };
    let array = || {
        TypedStorageContext::new(&[], &[])
            .construct_array(
                &ty,
                vec![Value::from(rils_execution::value::native_string("key"))],
            )
            .unwrap()
    };
    let (receiver, _) = reference(collection("HashSet", &ty, vec![array()]));
    let Value::Dynamic(object) = array() else {
        panic!("native array expected")
    };
    let (query, _) = reference(Value::Dynamic(object.clone()));
    object
        .with_mut(|_| {
            assert!(contains("HashSet", &receiver, query.clone()).is_err());
        })
        .unwrap();
    assert_eq!(
        contains("HashSet", &receiver, query.clone()),
        Ok(Value::from(rils_execution::value::native_bool(true)))
    );
    NativeInstancePlace::new(object)
        .unwrap()
        .field("0")
        .unwrap()
        .take()
        .unwrap();
    assert!(
        contains("HashSet", &receiver, query)
            .unwrap_err()
            .contains("moved")
    );
    assert_eq!(
        call("HashSet", "len", &[receiver]),
        Ok(Value::from(rils_execution::value::native_usize(1)))
    );
}
