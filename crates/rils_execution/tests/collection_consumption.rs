// Compatibility keys compare only their immutable identity.
#![allow(clippy::mutable_key_type)]
use rils_execution::{
    RilsValue, Type, Value,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    value::{
        BTreeMapValue, BTreeSetValue, FieldSlot, HashKey, HashMapValue, HashSetValue,
        MapCollection, ReferenceValue, SetCollection,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, Copy, Debug)]
enum Kind {
    HashMap,
    BTreeMap,
    HashSet,
    BTreeSet,
}
const KINDS: [Kind; 4] = [Kind::HashMap, Kind::BTreeMap, Kind::HashSet, Kind::BTreeSet];

fn collection(kind: Kind, key: HashKey, value: Value) -> Value {
    let key_type = RefCell::new(key.ty());
    let value_type = Type::of_value(&value).unwrap();
    let slot = FieldSlot::new(value_type.clone(), value);
    match kind {
        Kind::HashMap => Value::HashMap(Rc::new(HashMapValue {
            borrowed: Cell::new(0),
            entries: RefCell::new([(key, slot)].into()),
            key_type,
            value_type: RefCell::new(value_type),
        })),
        Kind::BTreeMap => Value::BTreeMap(Rc::new(BTreeMapValue {
            borrowed: Cell::new(0),
            entries: RefCell::new([(key, slot)].into()),
            key_type,
            value_type: RefCell::new(value_type),
        })),
        Kind::HashSet => Value::HashSet(Rc::new(HashSetValue {
            borrowed: Cell::new(0),
            entries: RefCell::new([key].into()),
            element_type: key_type,
        })),
        Kind::BTreeSet => Value::BTreeSet(Rc::new(BTreeSetValue {
            borrowed: Cell::new(0),
            entries: RefCell::new([key].into()),
            element_type: key_type,
        })),
    }
}
macro_rules! with_entries {
    ($value:expr, $entries:ident, $body:expr) => {
        match $value {
            Value::HashMap(value) => {
                let $entries = &value.entries;
                $body
            }
            Value::BTreeMap(value) => {
                let $entries = &value.entries;
                $body
            }
            Value::HashSet(value) => {
                let $entries = &value.entries;
                $body
            }
            Value::BTreeSet(value) => {
                let $entries = &value.entries;
                $body
            }
            _ => unreachable!(),
        }
    };
}
fn iterator(value: Value) -> Value {
    match into_iterator(value).unwrap() {
        IntoIteratorResult::Ready(value) => value,
        IntoIteratorResult::UserDefined(_) => panic!("expected built-in collection"),
    }
}
fn next(iterator: &mut Value) -> Option<Value> {
    next_builtin(iterator).unwrap().unwrap()
}
fn split_entry(kind: Kind, item: Value) -> (Value, Option<Value>) {
    if matches!(kind, Kind::HashMap | Kind::BTreeMap) {
        let Value::Tuple(entry) = item else {
            panic!("expected map entry")
        };
        let mut fields = entry.elements.borrow_mut();
        (fields[0].value.take().unwrap(), fields[1].value.take())
    } else {
        (item, None)
    }
}
fn string_address(value: Value) -> usize {
    RilsValue::new(value)
        .with_ref::<String, _>(|text| text.as_ptr() as usize)
        .unwrap()
}
fn project_key(value: &Value, key: HashKey) -> Value {
    let reference = match value {
        Value::HashMap(map) => {
            ReferenceValue::new_map_key(MapCollection::Hash(map.clone()), key, None)
        }
        Value::BTreeMap(map) => {
            ReferenceValue::new_map_key(MapCollection::BTree(map.clone()), key, None)
        }
        Value::HashSet(set) => {
            ReferenceValue::new_set_item(SetCollection::Hash(set.clone()), key, None)
        }
        Value::BTreeSet(set) => {
            ReferenceValue::new_set_item(SetCollection::BTree(set.clone()), key, None)
        }
        _ => unreachable!(),
    };
    Value::Reference(Rc::new(reference.unwrap()))
}

#[test]
fn consuming_collections_moves_unique_string_buffers_and_empties_the_source() {
    for kind in KINDS {
        let key = HashKey::from_value(&Value::from_string("owned key 🦀")).unwrap();
        let address = key
            .with_ref::<String, _>(|text| text.as_ptr() as usize)
            .unwrap();
        let source = collection(kind, key, Value::from_string("owned value"));
        let mut iter = iterator(source.clone());
        with_entries!(&source, entries, assert!(entries.borrow().is_empty()));
        let (key, value) = split_entry(kind, next(&mut iter).unwrap());
        assert_eq!(string_address(key), address, "{kind:?}");
        if let Some(value) = value {
            assert_eq!(value.as_string().unwrap(), "owned value");
        }
        assert!(next(&mut iter).is_none());
    }
}

#[test]
fn shared_keys_keep_an_independent_string_when_the_iterator_is_consumed() {
    for kind in KINDS {
        let key = HashKey::from_value(&Value::from_string("shared key")).unwrap();
        let address = key
            .with_ref::<String, _>(|text| text.as_ptr() as usize)
            .unwrap();
        let mut iter = iterator(collection(kind, key.clone(), Value::Unit));
        let (moved, _) = split_entry(kind, next(&mut iter).unwrap());
        assert_ne!(string_address(moved), address);
        assert_eq!(key.to_value().as_string().unwrap(), "shared key");
    }
}

#[test]
fn native_composite_keys_keep_the_original_storage_when_consumed() {
    use rils_execution::value::{
        record_codec::NativeRecordCodec, record_layout::RecordLayoutResolver,
    };
    use rils_stdlib::stdlib::string::String as NativeString;
    let ty = Type::Option(Box::new(Type::String));
    let layout = RecordLayoutResolver::new(&[]).resolve(&ty).unwrap();
    for kind in [Kind::HashMap, Kind::HashSet] {
        let mut codec = NativeRecordCodec::new();
        let payload = codec
            .into_native(
                Value::Option {
                    value: Some(Rc::new(Value::from_string("nested owned key"))),
                    element_type: Some(Type::String),
                },
                layout.clone(),
            )
            .unwrap();
        let key = HashKey::from_value(&codec.from_native(payload).unwrap()).unwrap();
        let locator = key.clone();
        let source = collection(kind, key, Value::Unit);
        let address = |value: Value| {
            RilsValue::new(value)
                .with_native_view(|view| {
                    view.option_item()?
                        .leaf()?
                        .with_rust::<NativeString, _>(|text| text.as_ref().as_ptr() as usize)
                })
                .unwrap()
                .unwrap()
        };
        let original = address(project_key(&source, locator));
        let mut iter = iterator(source);
        let (key, _) = split_entry(kind, next(&mut iter).unwrap());
        assert!(matches!(key, Value::Dynamic(_)));
        assert_eq!(Type::of_value(&key), Some(ty.clone()));
        assert!(!key.is_copy());
        assert_eq!(address(key), original, "{kind:?}");
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
fn non_clone_map_values_drop_once_on_yield_and_on_abandoned_iteration() {
    for kind in [Kind::HashMap, Kind::BTreeMap] {
        for consume in [false, true] {
            let drops = Rc::new(Cell::new(0));
            let source = collection(
                kind,
                HashKey::from_value(&Value::from_i32(7)).unwrap(),
                probe(&drops),
            );
            let second = FieldSlot::new(Type::named("Probe"), probe(&drops));
            match &source {
                Value::HashMap(map) => {
                    map.entries
                        .borrow_mut()
                        .insert(HashKey::from_value(&Value::from_i32(8)).unwrap(), second);
                }
                Value::BTreeMap(map) => {
                    map.entries
                        .borrow_mut()
                        .insert(HashKey::from_value(&Value::from_i32(8)).unwrap(), second);
                }
                _ => unreachable!(),
            }
            let mut iter = iterator(source);
            assert_eq!(drops.get(), 0);
            let item = consume.then(|| next(&mut iter).unwrap());
            drop(iter);
            assert_eq!(drops.get(), if consume { 1 } else { 2 });
            drop(item);
            assert_eq!(drops.get(), 2);
        }
    }
}

#[test]
fn consumption_rejects_access_conflicts_and_active_projections_without_draining() {
    for kind in KINDS {
        let source = collection(
            kind,
            HashKey::from_value(&Value::from_i32(7)).unwrap(),
            Value::Unit,
        );
        with_entries!(&source, entries, {
            let guard = entries.borrow();
            assert!(
                into_iterator(source.clone())
                    .err()
                    .unwrap()
                    .contains("accessed")
            );
            assert_eq!(guard.len(), 1);
            drop(guard);
            let guard = entries.borrow_mut();
            assert!(
                into_iterator(source.clone())
                    .err()
                    .unwrap()
                    .contains("accessed")
            );
            assert_eq!(guard.len(), 1);
        });
        let reference = project_key(&source, HashKey::from_value(&Value::from_i32(7)).unwrap());
        assert!(into_iterator(source.clone()).is_err());
        with_entries!(&source, entries, assert_eq!(entries.borrow().len(), 1));
        drop(reference);
        assert!(next(&mut iterator(source)).is_some());
    }
}

#[test]
fn partially_moved_maps_fail_before_transferring_entries() {
    for kind in [Kind::HashMap, Kind::BTreeMap] {
        let source = collection(
            kind,
            HashKey::from_value(&Value::from_i32(7)).unwrap(),
            Value::from_string("value"),
        );
        let take = |source: &Value| match source {
            Value::HashMap(map) => map
                .entries
                .borrow_mut()
                .get_mut(&HashKey::from_value(&Value::from_i32(7)).unwrap())
                .unwrap()
                .value
                .take(),
            Value::BTreeMap(map) => map
                .entries
                .borrow_mut()
                .get_mut(&HashKey::from_value(&Value::from_i32(7)).unwrap())
                .unwrap()
                .value
                .take(),
            _ => unreachable!(),
        };
        drop(take(&source));
        assert!(
            into_iterator(source.clone())
                .err()
                .unwrap()
                .contains("partially moved")
        );
        with_entries!(&source, entries, assert_eq!(entries.borrow().len(), 1));
    }
}
