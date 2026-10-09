// Only immutable KeyIdentity participates in compatibility key comparison.
#![allow(clippy::mutable_key_type)]
use rils_execution::{
    RilsValue, Type, Value,
    environment::StorageSlot,
    runtime_builtins::call_native_symbol,
    value::{
        BTreeMapValue, BTreeSetValue, FieldSlot, HashKey, HashMapValue, HashSetValue, KeyIdentity,
        ReferenceValue, record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const KINDS: [&str; 4] = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"];
fn collection(kind: &str, key: Value) -> Value {
    let key = HashKey::from_value(&key).unwrap();
    let key_type = RefCell::new(key.ty());
    let slot = FieldSlot::new(Type::Bool, Value::Bool(true));
    match kind {
        "HashMap" => Value::HashMap(Rc::new(HashMapValue {
            entries: RefCell::new([(key, slot)].into()),
            key_type,
            value_type: RefCell::new(Type::Bool),
            borrowed: Cell::new(0),
        })),
        "BTreeMap" => Value::BTreeMap(Rc::new(BTreeMapValue {
            entries: RefCell::new([(key, slot)].into()),
            key_type,
            value_type: RefCell::new(Type::Bool),
            borrowed: Cell::new(0),
        })),
        "HashSet" => Value::HashSet(Rc::new(HashSetValue {
            entries: RefCell::new([key].into()),
            element_type: key_type,
            borrowed: Cell::new(0),
        })),
        "BTreeSet" => Value::BTreeSet(Rc::new(BTreeSetValue {
            entries: RefCell::new([key].into()),
            element_type: key_type,
            borrowed: Cell::new(0),
        })),
        _ => unreachable!(),
    }
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
fn call(kind: &str, method: &str, arguments: &[Value]) -> Result<Value, String> {
    let symbol = rils_builtins::builtin(kind)
        .unwrap()
        .member(method)
        .unwrap()
        .native_symbol
        .unwrap();
    call_native_symbol(symbol, arguments).unwrap()
}
fn contains(kind: &str, receiver: &Value, query: &Value) -> Result<Value, String> {
    call(
        kind,
        if kind.ends_with("Map") {
            "contains_key"
        } else {
            "contains"
        },
        &[receiver.clone(), query.clone()],
    )
}
fn no_clone_string(text: &str) -> Value {
    type Text = rils_stdlib::stdlib::string::String;
    let value = Value::Native(
        NativeObject::new(
            Rc::new(NativeType::new::<Text>(Type::String)),
            Text::from(text.to_owned()),
        )
        .unwrap(),
    );
    assert!(value.clone_owned().is_err());
    value
}
#[test]
fn all_compatibility_collections_query_non_clone_keys_by_borrowed_identity() {
    for kind in KINDS {
        let (receiver, _) = reference(collection(kind, Value::from_string("key 🦀")));
        let (query, slot) = reference(no_clone_string("key 🦀"));
        let guard = slot.borrow();
        assert_eq!(contains(kind, &receiver, &query), Ok(Value::Bool(true)));
        drop(guard);
        assert_eq!(
            contains(kind, &receiver, &reference(no_clone_string("absent")).0),
            Ok(Value::Bool(false))
        );
        if kind.ends_with("Map") {
            assert_eq!(
                call(kind, "get_cloned", &[receiver.clone(), query.clone()])
                    .unwrap()
                    .as_option()
                    .unwrap()
                    .0,
                Some(Value::Bool(true))
            );
        }
        let removed = call(kind, "remove", &[receiver.clone(), query.clone()]).unwrap();
        if kind.ends_with("Map") {
            assert_eq!(removed.as_option().unwrap().0, Some(Value::Bool(true)));
        } else {
            assert_eq!(removed, Value::Bool(true));
        }
        assert_eq!(contains(kind, &receiver, &query), Ok(Value::Bool(false)));
        assert_eq!(
            RilsValue::new(query)
                .with_ref::<String, _>(|text| text.clone())
                .unwrap(),
            "key 🦀"
        );
    }
}
fn some(value: Value) -> Value {
    Value::Option {
        value: Some(Rc::new(value)),
        element_type: Some(Type::String),
    }
}
#[test]
fn nested_queries_share_native_and_compatibility_identity_without_payload_clones() {
    let ty = Type::Option(Box::new(Type::String));
    let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
    for kind in ["HashMap", "HashSet"] {
        let mut codec = NativeRecordCodec::new();
        let native = codec
            .into_native(some(Value::from_string("nested")), layout.clone())
            .unwrap();
        let (receiver, _) = reference(collection(kind, codec.from_native(native).unwrap()));
        let query = some(no_clone_string("nested"));
        assert!(
            HashKey::from_value(&query).is_err(),
            "query cannot be snapshotted"
        );
        let (query, _) = reference(query);
        assert_eq!(contains(kind, &receiver, &query), Ok(Value::Bool(true)));
        let native = codec
            .into_native(some(Value::from_string("nested")), layout.clone())
            .unwrap();
        assert_eq!(
            contains(
                kind,
                &receiver,
                &reference(codec.from_native(native).unwrap()).0
            ),
            Ok(Value::Bool(true))
        );
    }
}
#[test]
fn identity_keeps_width_container_kind_and_inactive_generic_types() {
    let id = |value: &Value| KeyIdentity::from_value(value, None, false).unwrap();
    assert_ne!(id(&Value::from_i32(1)), id(&Value::from_i64(1)));
    let none = |ty| Value::Option {
        value: None,
        element_type: Some(ty),
    };
    assert_ne!(id(&none(Type::I32)), id(&none(Type::String)));
    let err = |ok_type| Value::Result {
        value: Err(Rc::new(Value::from_i32(7))),
        ok_type: Some(ok_type),
        error_type: Some(Type::I32),
    };
    assert_ne!(id(&err(Type::Bool)), id(&err(Type::String)));
    let unresolved = Value::Option {
        value: None,
        element_type: None,
    };
    assert!(
        KeyIdentity::from_value(&unresolved, None, false)
            .unwrap_err()
            .contains("complete type witness")
    );
    assert_eq!(
        KeyIdentity::from_value(&unresolved, Some(&Type::Option(Box::new(Type::I32))), false)
            .unwrap(),
        id(&none(Type::I32))
    );
    for kind in KINDS {
        let (receiver, _) = reference(collection(kind, Value::from_i32(1)));
        assert!(contains(kind, &receiver, &Value::from_i64(1)).is_err());
        assert!(contains(kind, &receiver, &Value::from_f64(1.0)).is_err());
    }
    use rils_execution::value::IndexedStorage;
    let sequence = || {
        Rc::new(IndexedStorage {
            elements: RefCell::new(vec![FieldSlot::new(Type::I32, Value::from_i32(7))]),
            element_type: RefCell::new(Some(Type::I32)),
            active_iterators: Cell::new(0),
        })
    };
    assert_ne!(id(&Value::Tuple(sequence())), id(&Value::Array(sequence())));
}
#[test]
fn query_access_conflicts_return_errors_and_leave_the_collection_intact() {
    for kind in KINDS {
        let (receiver, _) = reference(collection(kind, Value::from_string("key")));
        let (query, slot) = reference(no_clone_string("key"));
        {
            let _writing = slot.borrow_mut();
            assert!(contains(kind, &receiver, &query).is_err());
        }
        assert_eq!(contains(kind, &receiver, &query), Ok(Value::Bool(true)));
    }
}
#[test]
fn borrowed_identity_matches_owned_hash_and_ordering_for_every_scalar_key() {
    use std::collections::{BTreeMap, HashMap};
    let values = [
        Value::from_i8(i8::MIN),
        Value::from_i16(i16::MIN),
        Value::from_i32(i32::MIN),
        Value::from_i64(i64::MIN),
        Value::from_i128(i128::MIN),
        Value::from_isize(isize::MIN),
        Value::from_u8(u8::MAX),
        Value::from_u16(u16::MAX),
        Value::from_u32(u32::MAX),
        Value::from_u64(u64::MAX),
        Value::from_u128(u128::MAX),
        Value::from_usize(usize::MAX),
        Value::Bool(false),
        Value::from_char('🦀'),
        Value::from_string("identity"),
    ];
    for value in values {
        let key = HashKey::from_ordered_value(&value).unwrap();
        let id = KeyIdentity::from_value(&value, None, true).unwrap();
        let hash = HashMap::from([(key.clone(), 7)]);
        let tree = BTreeMap::from([(key, 7)]);
        assert_eq!(hash.get(&id), Some(&7));
        assert_eq!(tree.get(&id), Some(&7));
    }
    assert!(HashKey::from_ordered_value(&Value::Unit).is_err());
    assert!(HashKey::from_ordered_value(&some(Value::from_string("text"))).is_err());
}

#[test]
fn nominal_query_traits_and_explicit_copy_policy_survive_identity_extraction() {
    use rils_execution::value::{StructType, native_instance};
    use rils_value::DynamicValue;
    let definition = Rc::new(StructType {
        name: "Key".into(),
        opaque_native: false,
        field_indices: Default::default(),
        generic_parameters: vec![],
        fields: vec![],
        methods: Default::default(),
        trait_methods: Default::default(),
        associated_types: Default::default(),
        implemented_traits: RefCell::new(["Eq".into(), "Hash".into()].into()),
    });
    let layout = RecordLayoutResolver::new(std::slice::from_ref(&definition))
        .resolve(&Type::named("Key"))
        .unwrap();
    let codec = Rc::new(NativeRecordCodec::with_definitions(
        std::slice::from_ref(&definition),
        &[],
    ));
    let make = || {
        native_instance::from_native(
            DynamicValue::record(layout.clone(), vec![]).unwrap(),
            codec.clone(),
        )
        .unwrap()
    };
    for kind in ["HashMap", "HashSet"] {
        let (receiver, _) = reference(collection(kind, make()));
        let value = make();
        assert!(!value.is_copy());
        let (query, _) = reference(value);
        assert_eq!(contains(kind, &receiver, &query), Ok(Value::Bool(true)));
        for missing in ["Eq", "Hash"] {
            definition.implemented_traits.borrow_mut().remove(missing);
            assert!(
                contains(kind, &receiver, &query)
                    .unwrap_err()
                    .contains("Eq + Hash")
            );
            definition
                .implemented_traits
                .borrow_mut()
                .insert(missing.into());
        }
        assert!(!make().is_copy());
    }
}
