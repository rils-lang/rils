#![allow(clippy::mutable_key_type)]
use rils_execution::{
    Type, Value,
    environment::Environment,
    iteration::{IntoIteratorResult, into_iterator, next_builtin},
    value::{
        BTreeMapValue, FieldSlot, HashKey, HashMapValue, ReferenceValue,
        native_instance::NativeInstancePlace, storage::TypedStorageContext,
    },
};
use rils_value::{NativeChildren, NativeObject, NativeType};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

fn map(tree: bool, value: Value) -> Value {
    let ty = Type::of_value(&value).unwrap();
    let entry = (
        HashKey::from_value(&Value::from_i32(7)).unwrap(),
        FieldSlot::new(ty.clone(), value),
    );
    if tree {
        Value::BTreeMap(Rc::new(BTreeMapValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            entries: RefCell::new([entry].into()),
            key_type: RefCell::new(Type::I32),
            value_type: RefCell::new(ty),
        }))
    } else {
        Value::HashMap(Rc::new(HashMapValue {
            option_storage: Default::default(),
            borrowed: Cell::new(0),
            entries: RefCell::new([entry].into()),
            key_type: RefCell::new(Type::I32),
            value_type: RefCell::new(ty),
        }))
    }
}
fn entry(source: Value) -> Value {
    let IntoIteratorResult::Ready(mut iterator) = into_iterator(source).unwrap() else {
        panic!("iterator");
    };
    next_builtin(&mut iterator).unwrap().unwrap().unwrap()
}
fn place(value: Value) -> NativeInstancePlace {
    let Value::Dynamic(object) = value else {
        panic!("native tuple");
    };
    NativeInstancePlace::new(object).unwrap()
}

struct Payload {
    text: String,
    drops: Rc<Cell<usize>>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        self.drops.set(self.drops.get() + 1);
    }
}
#[test]
fn custom_map_values_keep_bytes_methods_and_registration_across_nested_tuples() {
    for tree in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let descriptor = Rc::new(
            NativeType::new::<Payload>(Type::named("Payload")).register_method(
                "length",
                |context| {
                    context.receiver::<Payload, _>(|value| Value::from_usize(value.text.len()))
                },
            ),
        );
        let text = "original".to_owned();
        let address = text.as_ptr();
        let payload = Value::Native(
            NativeObject::new(
                descriptor.clone(),
                Payload {
                    text,
                    drops: drops.clone(),
                },
            )
            .unwrap(),
        );
        let value = entry(map(tree, payload));
        assert!(!value.is_copy());
        let ty = Type::Tuple(vec![Type::of_value(&value).unwrap()]);
        let nested = TypedStorageContext::new(&[], &[])
            .construct_tuple(&ty, vec![value])
            .unwrap();
        let inner = place(nested).field("0").unwrap().take().unwrap();
        let fields = place(inner);
        let field = fields.field("1").unwrap();
        let value = field.take().unwrap();
        let Value::Native(object) = value else {
            panic!("registered native leaf");
        };
        assert!(Rc::ptr_eq(&descriptor, &object.descriptor_handle()));
        assert_eq!(
            object.with::<Payload, _>(|value| value.text.as_ptr()),
            Ok(address)
        );
        assert_eq!(
            object.call("length", &[]).unwrap().unwrap().as_usize(),
            Some(8)
        );
        assert_eq!(drops.get(), 0);
        // Restore a moved field without reading its old payload.
        field.assign(Value::Native(object)).unwrap();
        drop(field);
        drop(fields);
        assert_eq!(drops.get(), 1);
    }
}

struct Children {
    child: Value,
    active: Rc<Cell<bool>>,
    partial: Rc<Cell<bool>>,
}
impl NativeChildren<Value> for Children {
    fn visit_values(&self, visit: &mut dyn FnMut(&Value)) {
        visit(&self.child);
    }
    fn has_active_references(&self) -> bool {
        self.active.get()
    }
    fn is_partially_moved(&self) -> bool {
        self.partial.get()
    }
}
#[test]
fn composed_registered_children_keep_lexical_origin_and_state_checks() {
    for tree in [false, true] {
        let environment = Environment::global();
        environment
            .borrow_mut()
            .define("number", Value::from_i32(1), true, Some(Type::I32));
        let slot = environment.borrow().slot("number").unwrap();
        let reference = Value::Reference(Rc::new(ReferenceValue::new_storage(slot.clone(), true)));
        let active = Rc::new(Cell::new(false));
        let partial = Rc::new(Cell::new(false));
        let descriptor = Rc::new(
            NativeType::new::<Children>(Type::named("Children")).with_children::<Children>(),
        );
        let payload = Value::Native(
            NativeObject::new(
                descriptor.clone(),
                Children {
                    child: reference,
                    active: active.clone(),
                    partial: partial.clone(),
                },
            )
            .unwrap(),
        );
        let value = entry(map(tree, payload));
        assert!(value.contains_reference());
        assert!(value.contains_local_reference(&environment));
        assert!(!value.contains_local_reference(&Environment::global()));
        let fields = place(value.clone());
        active.set(true);
        assert!(value.has_active_references());
        assert!(fields.field("1").unwrap().take().is_err());
        active.set(false);
        partial.set(true);
        assert!(value.is_partially_moved());
        assert!(fields.field("1").unwrap().take().is_err());
        partial.set(false);
        assert!(!value.is_partially_moved());
        let child = fields.field("1").unwrap().take().unwrap();
        assert!(!value.contains_reference());
        assert!(child.contains_local_reference(&environment));
        drop(child);
        drop(fields);
        drop(value);
        assert!(slot.borrow_mut().take().is_ok());
    }
}

#[test]
fn entry_copy_requires_explicit_native_registration() {
    #[derive(Clone, Copy)]
    struct Number(i32);
    for tree in [false, true] {
        for copy in [false, true] {
            let mut descriptor = NativeType::new::<Number>(Type::named("Number"));
            if copy {
                descriptor = descriptor.with_copy::<Number>();
            }
            let value = entry(map(
                tree,
                Value::Native(NativeObject::new(Rc::new(descriptor), Number(42)).unwrap()),
            ));
            assert_eq!(value.is_copy(), copy);
            if copy {
                assert!(value.clone_owned().is_ok());
            }
            let Value::Native(child) = place(value).field("1").unwrap().take().unwrap() else {
                panic!("leaf");
            };
            assert_eq!(child.with::<Number, _>(|value| value.0), Ok(42));
        }
    }
}

#[test]
fn invalid_entry_types_fail_before_the_map_is_drained() {
    for tree in [false, true] {
        for ty in [Type::Unknown, Type::String] {
            let source = map(tree, Value::from_i32(42));
            match &source {
                Value::HashMap(map) => *map.value_type.borrow_mut() = ty,
                Value::BTreeMap(map) => *map.value_type.borrow_mut() = ty,
                _ => unreachable!(),
            }
            assert!(into_iterator(source.clone()).is_err());
            let len = match source {
                Value::HashMap(map) => map.entries.borrow().len(),
                Value::BTreeMap(map) => map.entries.borrow().len(),
                _ => unreachable!(),
            };
            assert_eq!(len, 1);
        }
        let source = map(tree, Value::Unit);
        match &source {
            Value::HashMap(map) => map.entries.borrow_mut().clear(),
            Value::BTreeMap(map) => map.entries.borrow_mut().clear(),
            _ => unreachable!(),
        }
        let IntoIteratorResult::Ready(mut iterator) = into_iterator(source).unwrap() else {
            panic!("iterator");
        };
        assert!(next_builtin(&mut iterator).unwrap().unwrap().is_none());
    }
}

#[test]
fn conflicting_native_registrations_are_rejected_instead_of_losing_operations() {
    let make = |result| {
        Value::Native(
            NativeObject::new(
                Rc::new(
                    NativeType::new::<i32>(Type::named("Custom"))
                        .register_method("method", move |_| Ok(Value::from_i32(result))),
                ),
                1_i32,
            )
            .unwrap(),
        )
    };
    let ty = Type::Tuple(vec![Type::named("Custom"), Type::named("Custom")]);
    let error = TypedStorageContext::new(&[], &[])
        .construct_tuple(&ty, vec![make(1), make(2)])
        .err()
        .unwrap();
    assert!(error.contains("conflicting native registration"));
}
