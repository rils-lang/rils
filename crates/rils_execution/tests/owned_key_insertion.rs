#![allow(clippy::mutable_key_type)]
use rils_execution::{
    RilsValue, Type, Value,
    environment::StorageSlot,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{
        BTreeMapValue, BTreeSetValue, HashKey, HashMapValue, HashSetValue, ReferenceValue,
        record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};
const KINDS: [&str; 4] = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"];
fn empty(kind: &str) -> Value {
    match kind {
        "HashMap" => Value::HashMap(Rc::new(HashMapValue {
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
            borrowed: Cell::new(0),
        })),
        "BTreeMap" => Value::BTreeMap(Rc::new(BTreeMapValue {
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::Unknown),
            value_type: RefCell::new(Type::Unknown),
            borrowed: Cell::new(0),
        })),
        "HashSet" => Value::HashSet(Rc::new(HashSetValue {
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::Unknown),
            borrowed: Cell::new(0),
        })),
        "BTreeSet" => Value::BTreeSet(Rc::new(BTreeSetValue {
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::Unknown),
            borrowed: Cell::new(0),
        })),
        _ => unreachable!(),
    }
}
fn len(source: &Value) -> usize {
    match source {
        Value::HashMap(map) => map.entries.borrow().len(),
        Value::BTreeMap(map) => map.entries.borrow().len(),
        Value::HashSet(set) => set.entries.borrow().len(),
        Value::BTreeSet(set) => set.entries.borrow().len(),
        _ => unreachable!(),
    }
}
fn reference(value: Value, mutable: bool) -> Value {
    let mut slot = StorageSlot::uninitialized(true);
    slot.initialize(value);
    Value::Reference(Rc::new(ReferenceValue::new_storage(
        Rc::new(RefCell::new(slot)),
        mutable,
    )))
}
fn symbol(kind: &str, method: &str) -> &'static str {
    rils_builtins::builtin(kind)
        .unwrap()
        .member(method)
        .unwrap()
        .native_symbol
        .unwrap()
}
fn call(kind: &str, method: &str, arguments: &[Value]) -> Result<Value, String> {
    call_native_symbol(symbol(kind, method), arguments).unwrap()
}
fn insert(kind: &str, receiver: &Value, key: Value, value: Value) -> Result<Value, String> {
    let mut arguments = vec![receiver.clone(), key];
    if kind.ends_with("Map") {
        arguments.push(value);
    }
    call_native_owned_symbol(
        symbol(kind, "insert"),
        arguments,
        &NativeOwnedContext::default(),
    )
    .unwrap()
}
fn no_clone(text: &str) -> Value {
    type Text = rils_stdlib::stdlib::string::String;
    Value::Native(
        NativeObject::new(
            Rc::new(NativeType::new::<Text>(Type::String)),
            Text::from(text.to_owned()),
        )
        .unwrap(),
    )
}
fn address(value: &Value) -> usize {
    RilsValue::new(value.clone())
        .with_ref::<String, _>(|text| text.as_ptr() as usize)
        .unwrap()
}
fn next_key(kind: &str, source: Value) -> Value {
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(source).unwrap() else {
        panic!("iterator")
    };
    let item = next_builtin(&mut iterator).unwrap().unwrap().unwrap();
    if kind.ends_with("Map") {
        let Value::Tuple(fields) = item else {
            panic!("map pair")
        };
        fields.elements.borrow_mut()[0].value.take().unwrap()
    } else {
        item
    }
}
#[test]
fn every_legacy_insert_moves_non_clone_keys_and_preserves_the_original_buffer() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        let value = no_clone("owned 🦀");
        let original = address(&value);
        assert!(value.clone_owned().is_err());
        insert(kind, &receiver, value, Value::Bool(true)).unwrap();
        // Formatting, equality and membership must not clone stored keys.
        assert!(!format!("{source}").is_empty());
        let contains = if kind.ends_with("Map") {
            "contains_key"
        } else {
            "contains"
        };
        assert_eq!(
            call(
                kind,
                contains,
                &[receiver.clone(), Value::from_string("owned 🦀")]
            ),
            Ok(Value::Bool(true))
        );
        assert!(source.clone_owned().is_err());
        let cloned_method = if kind == "HashMap" {
            Some("keys_cloned")
        } else if kind == "BTreeMap" {
            Some("first_key_cloned")
        } else if kind == "BTreeSet" {
            Some("first_cloned")
        } else {
            None
        };
        if let Some(method) = cloned_method {
            assert!(call(kind, method, std::slice::from_ref(&receiver)).is_err());
        }
        if kind.ends_with("Set") {
            assert!(call(kind, "union", &[receiver.clone(), receiver.clone()]).is_err());
        }
        drop(receiver);
        let key = next_key(kind, source);
        assert_eq!(address(&key), original, "{kind}");
        assert!(key.clone_owned().is_err());
    }
}
struct Probe(Rc<Cell<usize>>);
impl Drop for Probe {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
fn probe(drops: &Rc<Cell<usize>>) -> Value {
    Value::Native(
        NativeObject::new(
            Rc::new(NativeType::new::<Probe>(Type::named("Probe"))),
            Probe(drops.clone()),
        )
        .unwrap(),
    )
}
#[test]
fn duplicate_insertion_moves_old_values_and_retains_the_original_key() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        let drops = Rc::new(Cell::new(0));
        let value = no_clone("same");
        let original = address(&value);
        insert(
            kind,
            &receiver,
            value,
            if kind.ends_with("Map") {
                probe(&drops)
            } else {
                Value::Unit
            },
        )
        .unwrap();
        let previous = insert(
            kind,
            &receiver,
            no_clone("same"),
            if kind.ends_with("Map") {
                probe(&drops)
            } else {
                Value::Unit
            },
        )
        .unwrap();
        if kind.ends_with("Map") {
            assert_eq!(drops.get(), 0);
            drop(previous);
            assert_eq!(drops.get(), 1);
        } else {
            assert_eq!(previous, Value::Bool(false));
        }
        drop(receiver);
        assert_eq!(address(&next_key(kind, source)), original);
        if kind.ends_with("Map") {
            assert_eq!(drops.get(), 2);
        }
    }
}
#[test]
fn insert_failure_preserves_entries_and_rejects_shared_non_copy_inputs() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        let alias = no_clone("shared");
        assert!(
            insert(kind, &receiver, alias.clone(), Value::Unit)
                .unwrap_err()
                .contains("shared")
        );
        assert_eq!(len(&source), 0);
        insert(kind, &receiver, no_clone("original"), Value::Bool(true)).unwrap();
        assert!(insert(kind, &receiver, Value::from_i32(7), Value::Bool(false)).is_err());
        let readonly = reference(source.clone(), false);
        assert!(insert(kind, &readonly, no_clone("other"), Value::Bool(false)).is_err());
        let locator = HashKey::from_value(&Value::from_string("original")).unwrap();
        use rils_execution::value::{MapCollection, SetCollection};
        let borrowed = match &source {
            Value::HashMap(map) => ReferenceValue::new_map_key(
                MapCollection::Hash(map.clone()),
                locator.identity(),
                None,
            ),
            Value::BTreeMap(map) => ReferenceValue::new_map_key(
                MapCollection::BTree(map.clone()),
                locator.identity(),
                None,
            ),
            Value::HashSet(set) => ReferenceValue::new_set_item(
                SetCollection::Hash(set.clone()),
                locator.identity(),
                None,
            ),
            Value::BTreeSet(set) => ReferenceValue::new_set_item(
                SetCollection::BTree(set.clone()),
                locator.identity(),
                None,
            ),
            _ => unreachable!(),
        }
        .unwrap();
        assert!(
            borrowed.read().is_err(),
            "snapshot must read the stored key, not the locator"
        );
        assert!(insert(kind, &receiver, no_clone("other"), Value::Bool(false)).is_err());
        drop(borrowed);
        assert_eq!(len(&source), 1);
        let contains = if kind.ends_with("Map") {
            "contains_key"
        } else {
            "contains"
        };
        assert_eq!(
            call(kind, contains, &[receiver, Value::from_string("original")]),
            Ok(Value::Bool(true))
        );
    }
}
#[test]
fn owned_nested_keys_reach_native_fields_without_copying_their_string() {
    let ty = Type::Option(Box::new(Type::String));
    for kind in ["HashMap", "HashSet"] {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        let text = no_clone("nested");
        let original = address(&text);
        let key = Value::Option {
            value: Some(Rc::new(text)),
            element_type: Some(Type::String),
        };
        insert(kind, &receiver, key, Value::Bool(true)).unwrap();
        drop(receiver);
        let collection_type = Type::Named {
            name: kind.into(),
            arguments: if kind.ends_with("Map") {
                vec![ty.clone(), Type::Bool]
            } else {
                vec![ty.clone()]
            },
        };
        let layout = RecordLayoutResolver::new(&[])
            .resolve(&collection_type)
            .unwrap();
        let native = NativeRecordCodec::new()
            .into_native(source, layout)
            .unwrap();
        let item = native.view().sequence_item(0).unwrap();
        let key = if kind.ends_with("Map") {
            item.field(0).unwrap()
        } else {
            item
        };
        let address = key
            .option_item()
            .unwrap()
            .leaf()
            .unwrap()
            .with_rust::<rils_stdlib::stdlib::string::String, _>(|text| {
                text.as_ref().as_ptr() as usize
            })
            .unwrap();
        assert_eq!(address, original);
    }
}
#[test]
fn fallible_key_apis_do_not_hide_clone_or_panic() {
    let key = HashKey::from_owned_value(no_clone("unique"), None, false).unwrap();
    assert!(key.to_value().is_err());
    assert!(key.clone_owned().is_err());
    assert!(key.clone().into_value().is_err());
    assert_eq!(
        RilsValue::new(key.into_value().unwrap()).with_ref::<String, _>(|text| text.len()),
        Ok(6)
    );
    let copy = HashKey::from_owned_value(Value::from_i32(42), None, false).unwrap();
    assert_eq!(copy.clone().into_value().unwrap(), Value::from_i32(42));
    assert_eq!(copy.into_value().unwrap(), Value::from_i32(42));
}

#[test]
fn explicit_collection_clones_have_independent_consumable_keys() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        insert(
            kind,
            &receiver,
            Value::from_string("cloneable"),
            Value::Bool(true),
        )
        .unwrap();
        let cloned = source.clone_owned().unwrap();
        let output = next_key(kind, cloned);
        assert_eq!(output.as_string().unwrap(), "cloneable");
        drop(receiver);
        let original = next_key(kind, source);
        assert_ne!(address(&output), address(&original));
    }
}

#[test]
fn owned_keys_reject_dynamic_aliases_and_copy_shared_copy_aggregates() {
    let ty = Type::Option(Box::new(Type::String));
    let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
    let mut codec = NativeRecordCodec::new();
    let native = codec
        .into_native(
            Value::Option {
                value: Some(Rc::new(Value::from_string("text"))),
                element_type: Some(Type::String),
            },
            layout,
        )
        .unwrap();
    let value = codec.from_native(native).unwrap();
    assert!(
        HashKey::from_owned_value(value.clone(), None, false)
            .unwrap_err()
            .contains("shared")
    );
    assert!(HashKey::from_owned_value(value, None, false).is_ok());
    use rils_execution::value::{FieldSlot, IndexedStorage};
    let fields = Rc::new(IndexedStorage {
        elements: RefCell::new(vec![FieldSlot::new(Type::I32, Value::from_i32(7))]),
        element_type: RefCell::new(Some(Type::I32)),
        active_iterators: Cell::new(0),
    });
    let key = HashKey::from_owned_value(Value::Array(fields.clone()), None, false).unwrap();
    fields.elements.borrow_mut()[0].value = Some(Value::from_i32(9));
    assert_eq!(format!("{}", key.into_value().unwrap()), "[7]");
}

#[test]
fn borrowed_iterator_identity_cache_does_not_retain_non_clone_key_payloads() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        let key = no_clone("borrow then move 🦀");
        let original = address(&key);
        insert(kind, &receiver, key, Value::from_i32(7)).unwrap();
        let mut iterator = call_native_owned_symbol(
            symbol(kind, "iter"),
            vec![receiver.clone()],
            &NativeOwnedContext::default(),
        )
        .unwrap()
        .unwrap();
        let identities = match &iterator {
            Value::BorrowedMapIterator(iter) => iter.keys.clone(),
            Value::BorrowedSetIterator(iter) => iter.keys.clone(),
            _ => panic!("expected borrowed compatibility iterator"),
        };
        assert_eq!(identities[0].ty(), &Type::String);
        assert!(into_iterator(source.clone()).is_err());
        let projected = next_builtin(&mut iterator).unwrap().unwrap().unwrap();
        let (key, value) = if kind.ends_with("Map") {
            let Value::Tuple(pair) = projected else {
                panic!("map pair")
            };
            let mut fields = pair.elements.borrow_mut();
            (fields[0].value.take().unwrap(), fields[1].value.take())
        } else {
            (projected, None)
        };
        assert_eq!(address(&key), original);
        let Value::Reference(key_ref) = &key else {
            panic!("key reference")
        };
        assert!(key_ref.read().is_err()); // Actual payload has no Clone implementation.
        assert!(next_builtin(&mut iterator).unwrap().unwrap().is_none());
        drop(iterator);
        assert!(into_iterator(source.clone()).is_err()); // Yielded references retain the guard.
        drop(key);
        if value.is_some() {
            assert!(into_iterator(source.clone()).is_err());
        }
        drop(value);
        let moved = next_key(kind, source);
        assert_eq!(address(&moved), original); // Retained identities cannot impede the move.
        assert_eq!(identities[0].ty(), &Type::String);
    }
}

#[test]
fn borrowed_iterator_creation_reports_entry_conflicts_without_acquiring_a_lease() {
    for kind in KINDS {
        let source = empty(kind);
        let receiver = reference(source.clone(), true);
        insert(kind, &receiver, no_clone("key"), Value::Unit).unwrap();
        macro_rules! check {
            ($collection:expr) => {{
                let entries = $collection.entries.borrow_mut();
                assert!(
                    call_native_owned_symbol(
                        symbol(kind, "iter"),
                        vec![receiver.clone()],
                        &NativeOwnedContext::default(),
                    )
                    .unwrap()
                    .unwrap_err()
                    .contains("mutably accessed")
                );
                assert_eq!($collection.borrowed.get(), 0);
                assert_eq!(entries.len(), 1);
            }};
        }
        match &source {
            Value::HashMap(map) => check!(map),
            Value::BTreeMap(map) => check!(map),
            Value::HashSet(set) => check!(set),
            Value::BTreeSet(set) => check!(set),
            _ => unreachable!(),
        }
        assert_eq!(next_key(kind, source).as_string().unwrap(), "key");
    }
}
