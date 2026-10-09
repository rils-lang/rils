//! Compatibility receivers keep source guards until the operation completes.
#![allow(clippy::mutable_key_type)]
use rils_execution::{
    Type, Value,
    environment::StorageSlot,
    runtime_builtins::{NativeOwnedContext, call_native_owned_symbol, call_native_symbol},
    value::{
        BTreeMapValue, BTreeSetValue, FieldSlot, HashKey, HashMapValue, HashSetValue,
        IndexedStorage, MapCollection, ReferenceValue, native_ops,
    },
};
use rils_value::{NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

const KINDS: [&str; 4] = ["HashMap", "BTreeMap", "HashSet", "BTreeSet"];
type GuardCheck = Rc<dyn Fn() -> bool>;

fn empty(kind: &str) -> Value {
    match kind {
        "HashMap" => Value::HashMap(Rc::new(HashMapValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::String),
            value_type: RefCell::new(Type::String),
        })),
        "BTreeMap" => Value::BTreeMap(Rc::new(BTreeMapValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(Default::default()),
            key_type: RefCell::new(Type::String),
            value_type: RefCell::new(Type::String),
        })),
        "HashSet" => Value::HashSet(Rc::new(HashSetValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::String),
        })),
        "BTreeSet" => Value::BTreeSet(Rc::new(BTreeSetValue {
            borrowed: Cell::new(0),
            entries: RefCell::new(Default::default()),
            element_type: RefCell::new(Type::String),
        })),
        _ => unreachable!(),
    }
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
    call_native_symbol(symbol(kind, method), arguments)
        .unwrap_or_else(|| panic!("missing {kind}::{method} dispatch"))
}
fn insert(kind: &str, source: &Value, key: Value, value: Value) {
    let (receiver, _) = receiver(source.clone(), 0);
    let mut arguments = vec![receiver, key];
    if kind.ends_with("Map") {
        arguments.push(value);
    }
    call_native_owned_symbol(
        symbol(kind, "insert"),
        arguments,
        &NativeOwnedContext::default(),
    )
    .unwrap()
    .unwrap();
}
fn receiver(source: Value, shape: usize) -> (Value, GuardCheck) {
    match shape {
        0 => {
            let mut slot = StorageSlot::uninitialized(true);
            slot.initialize(source);
            let slot = Rc::new(RefCell::new(slot));
            let weak = Rc::downgrade(&slot);
            (
                Value::Reference(Rc::new(ReferenceValue::new_storage(slot, true))),
                Rc::new(move || weak.upgrade().unwrap().try_borrow_mut().is_err()),
            )
        }
        1 => {
            let sequence = Rc::new(IndexedStorage {
                active_iterators: Cell::new(0),
                element_type: RefCell::new(None),
                elements: RefCell::new(vec![FieldSlot::new(
                    Type::of_value(&source).unwrap(),
                    source,
                )]),
            });
            let weak = Rc::downgrade(&sequence);
            (
                Value::Reference(Rc::new(
                    ReferenceValue::new_indexed_element(sequence, 0, true).unwrap(),
                )),
                Rc::new(move || weak.upgrade().unwrap().elements.try_borrow_mut().is_err()),
            )
        }
        2 => {
            let key = HashKey::from_owned_value(Value::from_i32(7), None, false).unwrap();
            let identity = key.identity();
            let ty = Type::of_value(&source).unwrap();
            let outer = Rc::new(HashMapValue {
                borrowed: Cell::new(0),
                key_type: RefCell::new(Type::I32),
                value_type: RefCell::new(ty.clone()),
                entries: RefCell::new([(key, FieldSlot::new(ty, source))].into()),
            });
            let weak = Rc::downgrade(&outer);
            (
                Value::Reference(Rc::new(
                    ReferenceValue::new_map_value(MapCollection::Hash(outer), identity, None)
                        .unwrap(),
                )),
                Rc::new(move || weak.upgrade().unwrap().entries.try_borrow_mut().is_err()),
            )
        }
        _ => unreachable!(),
    }
}
fn clone_probe(check: GuardCheck, copies: Rc<Cell<usize>>) -> Value {
    type Text = rils_stdlib::stdlib::string::String;
    let descriptor =
        NativeType::new::<Text>(Type::String).register_method(native_ops::CLONE, move |context| {
            assert!(check(), "source guard must be held while Clone runs");
            copies.set(copies.get() + 1);
            let copy = context.receiver::<Text, _>(Clone::clone)?;
            Ok(Value::Native(context.new_object(copy)?))
        });
    Value::Native(NativeObject::new(Rc::new(descriptor), Text::from("probe".to_owned())).unwrap())
}
macro_rules! with_entries {
    ($source:expr, $entries:ident, $body:block) => {
        match $source {
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

#[test]
fn cloning_members_hold_storage_indexed_and_map_projection_source_guards() {
    for kind in KINDS {
        for shape in 0..3 {
            let source = empty(kind);
            let (receiver, check) = receiver(source.clone(), shape);
            let copies = Rc::new(Cell::new(0));
            let probe = clone_probe(check.clone(), copies.clone());
            if kind.ends_with("Map") {
                insert(kind, &source, Value::from_string("key"), probe);
                call(
                    kind,
                    "get_cloned",
                    &[receiver.clone(), Value::from_string("key")],
                )
                .unwrap();
            } else {
                insert(kind, &source, probe, Value::Unit);
                call(kind, "union", &[receiver.clone(), receiver.clone()]).unwrap();
            }
            assert_eq!(copies.get(), 1, "{kind}, shape {shape}");
            assert!(!check(), "source guard must be released after the call");
        }
    }
}

#[test]
fn set_operations_hold_both_distinct_receiver_guards() {
    for kind in ["HashSet", "BTreeSet"] {
        let left = empty(kind);
        let right = empty(kind);
        let (left_ref, left_check) = receiver(left.clone(), 0);
        let (right_ref, right_check) = receiver(right.clone(), 1);
        let check: GuardCheck = Rc::new(move || left_check() && right_check());
        let copies = Rc::new(Cell::new(0));
        insert(kind, &left, clone_probe(check, copies.clone()), Value::Unit);
        call(kind, "union", &[left_ref.clone(), right_ref.clone()]).unwrap();
        assert_eq!(copies.get(), 1);
        for method in ["is_subset", "is_superset", "is_disjoint"] {
            call(kind, method, &[left_ref.clone(), right_ref.clone()]).unwrap();
        }
    }
}

#[test]
fn entry_conflicts_and_readonly_mutations_fail_without_changing_collections() {
    for kind in KINDS {
        let source = empty(kind);
        insert(
            kind,
            &source,
            Value::from_string("key"),
            Value::from_string("value"),
        );
        let (receiver, _) = receiver(source.clone(), 0);
        let contains = if kind.ends_with("Map") {
            "contains_key"
        } else {
            "contains"
        };
        with_entries!(&source, entries, {
            let guard = entries.borrow_mut();
            for method in [contains, "remove"] {
                assert!(
                    call(kind, method, &[receiver.clone(), Value::from_string("key")])
                        .unwrap_err()
                        .contains("accessed")
                );
            }
            assert_eq!(guard.len(), 1);
        });
        let Value::Reference(reference) = &receiver else {
            unreachable!()
        };
        let readonly = Value::Reference(Rc::new(reference.reborrow(false).unwrap()));
        assert!(
            call(
                kind,
                "remove",
                &[readonly.clone(), Value::from_string("key")]
            )
            .is_err()
        );
        assert!(call(kind, "remove", &[source.clone(), Value::from_string("key")]).is_err());
        with_entries!(&source, entries, {
            assert_eq!(entries.borrow().len(), 1);
        });
        call(kind, "remove", &[receiver, Value::from_string("key")]).unwrap();
        with_entries!(&source, entries, {
            assert!(entries.borrow().is_empty());
        });
    }
}

#[test]
fn result_type_conflicts_are_checked_before_map_removal() {
    for kind in ["HashMap", "BTreeMap"] {
        let source = empty(kind);
        insert(
            kind,
            &source,
            Value::from_string("key"),
            Value::from_string("value"),
        );
        let (receiver, _) = receiver(source.clone(), 0);
        let value_type = match &source {
            Value::HashMap(map) => &map.value_type,
            Value::BTreeMap(map) => &map.value_type,
            _ => unreachable!(),
        };
        let guard = value_type.borrow_mut();
        assert!(
            call(
                kind,
                "remove",
                &[receiver.clone(), Value::from_string("key")]
            )
            .unwrap_err()
            .contains("accessed")
        );
        with_entries!(&source, entries, {
            assert_eq!(entries.borrow().len(), 1);
        });
        drop(guard);
        call(kind, "remove", &[receiver, Value::from_string("key")]).unwrap();
    }
}

#[test]
fn multiple_mutable_references_can_still_operate_on_the_same_collection() {
    for kind in KINDS {
        let source = empty(kind);
        insert(
            kind,
            &source,
            Value::from_string("key"),
            Value::from_string("value"),
        );
        let mut slot = StorageSlot::uninitialized(true);
        slot.initialize(source.clone());
        let slot = Rc::new(RefCell::new(slot));
        let first = Value::Reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        let second = Value::Reference(Rc::new(ReferenceValue::new_storage(slot, true)));
        let contains = if kind.ends_with("Map") {
            "contains_key"
        } else {
            "contains"
        };
        assert_eq!(
            call(kind, contains, &[first.clone(), Value::from_string("key")]),
            Ok(Value::Bool(true))
        );
        call(kind, "remove", &[second, Value::from_string("key")]).unwrap();
        assert_eq!(
            call(kind, contains, &[first, Value::from_string("key")]),
            Ok(Value::Bool(false))
        );
    }
}
