// Compatibility collections still contain interior-mutability wrappers; only
// their immutable key identities participate in ordering and hashing.
#![allow(clippy::mutable_key_type)]
use rils_execution::{
    RilsHostType, RilsValue, Type, Value,
    value::{
        BTreeMapValue, BTreeSetValue, FieldSlot, HashKey, HashMapValue, HashSetValue,
        MapCollection, ReferenceValue, SetCollection, equality::borrowed_equal,
        record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    rc::Rc,
};

fn map(ordered: bool, key: HashKey, value: Value) -> MapCollection {
    let ty = Type::of_value(&value).unwrap();
    let key_type = RefCell::new(key.ty());
    let entry = (key, FieldSlot::new(ty.clone(), value));
    if ordered {
        MapCollection::BTree(Rc::new(BTreeMapValue {
            entries: RefCell::new(BTreeMap::from([entry])),
            borrowed: Cell::new(0),
            key_type,
            value_type: RefCell::new(ty),
        }))
    } else {
        MapCollection::Hash(Rc::new(HashMapValue {
            entries: RefCell::new(HashMap::from([entry])),
            borrowed: Cell::new(0),
            key_type,
            value_type: RefCell::new(ty),
        }))
    }
}
fn set(ordered: bool, key: HashKey) -> SetCollection {
    let ty = RefCell::new(key.ty());
    if ordered {
        SetCollection::BTree(Rc::new(BTreeSetValue {
            entries: RefCell::new(BTreeSet::from([key])),
            borrowed: Cell::new(0),
            element_type: ty,
        }))
    } else {
        SetCollection::Hash(Rc::new(HashSetValue {
            entries: RefCell::new(HashSet::from([key])),
            borrowed: Cell::new(0),
            element_type: ty,
        }))
    }
}
fn reference(reference: ReferenceValue) -> Value {
    Value::Reference(Rc::new(reference))
}
fn map_is_locked(map: &MapCollection) -> bool {
    match map {
        MapCollection::Hash(map) => map.entries.try_borrow_mut().is_err(),
        MapCollection::BTree(map) => map.entries.try_borrow_mut().is_err(),
    }
}
fn set_is_locked(set: &SetCollection) -> bool {
    match set {
        SetCollection::Hash(set) => set.entries.try_borrow_mut().is_err(),
        SetCollection::BTree(set) => set.entries.try_borrow_mut().is_err(),
    }
}

#[test]
fn string_projections_borrow_the_actual_stored_key_and_hold_the_collection_guard() {
    for ordered in [false, true] {
        let key = HashKey::from_value(&Value::from_string("原始🦀")).unwrap();
        let address = key
            .with_ref::<String, _>(|text| text.as_ptr() as usize)
            .unwrap();
        let source = map(ordered, key.clone(), Value::from_i32(42));
        // Deliberately use an equal key with a different buffer as the locator.
        let locator = HashKey::from_value(&Value::from_string("原始🦀")).unwrap();
        let projected =
            reference(ReferenceValue::new_map_key(source.clone(), locator, None).unwrap());
        assert_eq!(
            borrowed_equal(&projected, &Value::from_string("原始🦀")),
            Ok(true)
        );
        assert_eq!(
            RilsValue::new(projected).with_ref::<String, _>(|text| {
                assert!(map_is_locked(&source));
                text.as_ptr() as usize
            }),
            Ok(address)
        );
        assert_eq!(source.borrowed().get(), 0);
        let source = set(ordered, key);
        let locator = HashKey::from_value(&Value::from_string("原始🦀")).unwrap();
        let projected =
            reference(ReferenceValue::new_set_item(source.clone(), locator, None).unwrap());
        assert_eq!(
            RilsValue::new(projected).with_ref::<String, _>(|text| {
                assert!(set_is_locked(&source));
                text.as_ptr() as usize
            }),
            Ok(address)
        );
        assert_eq!(source.borrowed().get(), 0);
    }
}

#[test]
fn scalar_projection_matrix_uses_registered_raw_and_wrapped_host_views() {
    fn check<T: RilsHostType + Copy + std::fmt::Debug + PartialEq>(value: Value, expected: T) {
        for ordered in [false, true] {
            let key = HashKey::from_value(&value).unwrap();
            let source = set(ordered, key.clone());
            let projected = reference(ReferenceValue::new_set_item(source, key, None).unwrap());
            assert_eq!(borrowed_equal(&projected, &value), Ok(true));
            assert_eq!(
                RilsValue::new(projected).with_ref::<T, _>(|value| *value),
                Ok(expected)
            );
        }
    }
    check(Value::Unit, ());
    check(Value::Bool(true), true);
    check(Value::from_char('界'), '界');
    check(Value::from_i8(i8::MIN), i8::MIN);
    check(Value::from_i16(i16::MIN), i16::MIN);
    check(Value::from_i32(i32::MIN), i32::MIN);
    check(Value::from_i64(i64::MIN), i64::MIN);
    check(Value::from_i128(i128::MIN), i128::MIN);
    check(Value::from_isize(isize::MIN), isize::MIN);
    check(Value::from_u8(u8::MAX), u8::MAX);
    check(Value::from_u16(u16::MAX), u16::MAX);
    check(Value::from_u32(u32::MAX), u32::MAX);
    check(Value::from_u64(u64::MAX), u64::MAX);
    check(Value::from_u128(u128::MAX), u128::MAX);
    check(Value::from_usize(usize::MAX), usize::MAX);
}

struct Probe {
    drops: Rc<Cell<usize>>,
}
impl Drop for Probe {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
impl RilsHostType for Probe {
    type Native = Self;
    fn from_native(value: Self) -> Self {
        value
    }
    fn as_native_ref(value: &Self) -> &Self {
        value
    }
}
#[test]
fn map_values_borrow_non_clone_payloads_and_keep_drop_ownership() {
    for ordered in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let payload = Value::Native(
            NativeObject::new(
                Rc::new(NativeType::new::<Probe>(Type::named("Probe"))),
                Probe {
                    drops: drops.clone(),
                },
            )
            .unwrap(),
        );
        let key = HashKey::from_value(&Value::from_i32(7)).unwrap();
        let source = map(ordered, key.clone(), payload);
        let projected =
            reference(ReferenceValue::new_map_value(source.clone(), key, None).unwrap());
        assert!(
            RilsValue::new(projected)
                .with_ref::<Probe, _>(|probe| {
                    assert!(map_is_locked(&source));
                    Rc::ptr_eq(&probe.drops, &drops)
                })
                .unwrap()
        );
        assert_eq!(drops.get(), 0);
        assert_eq!(source.borrowed().get(), 0);
        drop(source);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn missing_moved_and_conflicting_collection_targets_return_errors() {
    let key = HashKey::from_value(&Value::from_i32(7)).unwrap();
    for ordered in [false, true] {
        let source = map(ordered, key.clone(), Value::from_string("value"));
        let projected =
            reference(ReferenceValue::new_map_value(source.clone(), key.clone(), None).unwrap());
        macro_rules! check_map {
            ($map:expr) => {{
                let mut entries = $map.entries.borrow_mut();
                assert!(
                    borrowed_equal(&projected, &Value::from_string("value"))
                        .unwrap_err()
                        .contains("mutably accessed")
                );
                assert!(
                    RilsValue::new(projected.clone())
                        .with_ref::<String, _>(|text| text.len())
                        .unwrap_err()
                        .contains("mutably accessed")
                );
                entries.get_mut(&key).unwrap().value = None;
                drop(entries);
                assert!(
                    borrowed_equal(&projected, &Value::Unit)
                        .unwrap_err()
                        .contains("moved")
                );
                $map.entries.borrow_mut().clear();
                assert!(
                    borrowed_equal(&projected, &Value::Unit)
                        .unwrap_err()
                        .contains("no longer exists")
                );
            }};
        }
        match &source {
            MapCollection::Hash(map) => check_map!(map),
            MapCollection::BTree(map) => check_map!(map),
        }
        drop(projected);
        assert_eq!(source.borrowed().get(), 0);
        let source = set(ordered, key.clone());
        let projected =
            reference(ReferenceValue::new_set_item(source.clone(), key.clone(), None).unwrap());
        macro_rules! check_set {
            ($set:expr) => {{
                let mut entries = $set.entries.borrow_mut();
                assert!(
                    borrowed_equal(&projected, &Value::from_i32(7))
                        .unwrap_err()
                        .contains("mutably accessed")
                );
                entries.clear();
                drop(entries);
                assert!(
                    borrowed_equal(&projected, &Value::from_i32(7))
                        .unwrap_err()
                        .contains("no longer exists")
                );
            }};
        }
        match &source {
            SetCollection::Hash(set) => check_set!(set),
            SetCollection::BTree(set) => check_set!(set),
        }
        drop(projected);
        assert_eq!(source.borrowed().get(), 0);
    }
}

#[test]
fn native_sum_projections_keep_layout_and_serve_native_collection_queries() {
    let ty = Type::Option(Box::new(Type::String));
    let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
    let value = NativeRecordCodec::new()
        .from_native(
            NativeRecordCodec::new()
                .into_native(
                    Value::Option {
                        value: Some(Rc::new(Value::from_string("nested"))),
                        element_type: Some(Type::String),
                    },
                    layout.clone(),
                )
                .unwrap(),
        )
        .unwrap();
    // Ensure the key representation holds a dynamic sum, rather than a decoded Option.
    let payload = NativeRecordCodec::new()
        .into_native(value, layout.clone())
        .unwrap();
    let value = rils_execution::value::native_instance::from_native(
        payload,
        Rc::new(NativeRecordCodec::new()),
    )
    .unwrap();
    let key = HashKey::from_value(&value).unwrap();
    let source = set(false, key.clone());
    let projected = reference(ReferenceValue::new_set_item(source, key, None).unwrap());
    let Value::Reference(reference) = &projected else {
        unreachable!()
    };
    assert_eq!(reference.native_layout().unwrap().unwrap().rils_type(), &ty);
    assert_eq!(borrowed_equal(&projected, &value), Ok(true));
    assert_eq!(
        RilsValue::new(projected.clone())
            .with_native_view(|view| view.option_is_some())
            .unwrap(),
        Ok(true)
    );
    let set_layout = rils_value::DynamicLayout::sequence(
        Type::Named {
            name: "HashSet".into(),
            arguments: vec![ty],
        },
        layout,
    );
    let Value::Dynamic(value) = value else {
        unreachable!()
    };
    let set = rils_value::DynamicValue::sequence(
        set_layout.clone(),
        vec![value.into_value().ok().unwrap()],
    )
    .unwrap();
    let receiver = Value::Dynamic(
        rils_value::DynamicObject::new(Rc::new(rils_value::DynamicType::new(set_layout)), set)
            .unwrap(),
    );
    let symbol = rils_builtins::builtin("HashSet")
        .unwrap()
        .member("contains")
        .unwrap()
        .native_symbol
        .unwrap();
    assert_eq!(
        rils_execution::runtime_builtins::call_native_symbol(symbol, &[receiver, projected])
            .unwrap(),
        Ok(Value::Bool(true))
    );
}

#[test]
fn nominal_collection_projections_retain_trait_metadata_for_native_queries() {
    use rils_execution::value::{StructType, native_instance};
    use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};
    let definition = Rc::new(StructType {
        name: "Key".into(),
        opaque_native: false,
        field_indices: Default::default(),
        generic_parameters: vec![],
        fields: vec![],
        methods: Default::default(),
        trait_methods: Default::default(),
        implemented_traits: RefCell::new(HashSet::from(["Eq".into(), "Hash".into()])),
        associated_types: Default::default(),
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
    assert!(!make().is_copy());
    let key = HashKey::from_value(&make()).unwrap();
    let legacy = map(false, key.clone(), make());
    let key_ref =
        reference(ReferenceValue::new_map_key(legacy.clone(), key.clone(), None).unwrap());
    let value_ref = reference(ReferenceValue::new_map_value(legacy, key.clone(), None).unwrap());
    let set_ref =
        reference(ReferenceValue::new_set_item(set(false, key.clone()), key, None).unwrap());
    let set_layout = DynamicLayout::sequence(
        Type::Named {
            name: "HashSet".into(),
            arguments: vec![Type::named("Key")],
        },
        layout.clone(),
    );
    let entries = DynamicValue::sequence(
        set_layout.clone(),
        vec![DynamicValue::record(layout, vec![]).unwrap()],
    )
    .unwrap();
    let receiver =
        Value::Dynamic(DynamicObject::new(Rc::new(DynamicType::new(set_layout)), entries).unwrap());
    let symbol = rils_builtins::builtin("HashSet")
        .unwrap()
        .member("contains")
        .unwrap()
        .native_symbol
        .unwrap();
    for projected in [key_ref, value_ref, set_ref] {
        assert_eq!(
            rils_execution::runtime_builtins::call_native_symbol(
                symbol,
                &[receiver.clone(), projected.clone()]
            )
            .unwrap(),
            Ok(Value::Bool(true))
        );
        definition.implemented_traits.borrow_mut().remove("Hash");
        assert!(
            rils_execution::runtime_builtins::call_native_symbol(
                symbol,
                &[receiver.clone(), projected]
            )
            .unwrap()
            .unwrap_err()
            .contains("Eq + Hash")
        );
        definition
            .implemented_traits
            .borrow_mut()
            .insert("Hash".into());
    }
}

#[test]
fn consuming_unique_string_key_moves_the_buffer_while_shared_keys_stay_independent() {
    let key = HashKey::from_value(&Value::from_string("独占内容")).unwrap();
    let original = key
        .with_ref::<String, _>(|text| text.as_ptr() as usize)
        .unwrap();
    assert_eq!(
        RilsValue::new(key.into_value()).with_ref::<String, _>(|text| text.as_ptr() as usize),
        Ok(original)
    );
    let key = HashKey::from_value(&Value::from_string("共享内容")).unwrap();
    let original = key
        .with_ref::<String, _>(|text| text.as_ptr() as usize)
        .unwrap();
    let copied = RilsValue::new(key.clone().into_value());
    assert_ne!(
        copied
            .with_ref::<String, _>(|text| text.as_ptr() as usize)
            .unwrap(),
        original
    );
    assert_eq!(copied.get_cloned::<String>().unwrap(), "共享内容");
    assert_eq!(key.ty(), Type::String);
    assert!(format!("{key:?}").contains("共享内容"));
}
