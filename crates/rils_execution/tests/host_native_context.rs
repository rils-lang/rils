use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    environment::Environment,
    runtime_builtins::NativeOwnedContext,
    value::{HostObject, HostType, record_codec, runtime_layouts},
};

fn declaration(name: &str, copy: bool) -> Rc<HostType> {
    Rc::new(HostType {
        name: name.into(),
        base_types: HashSet::new(),
        copy,
        methods: RefCell::default(),
    })
}

#[test]
fn host_declaration_lookup_preserves_identity_through_nested_references_and_fields() {
    use rils_execution::{
        environment::StorageSlot,
        value::{
            ReferenceValue, native_instance::NativeInstancePlace, storage::TypedStorageContext,
        },
    };
    let definition = declaration("Item", false);
    let owner = Rc::new(HostObject {
        type_definition: definition.clone(),
        payload: Rc::new(()),
    });
    let hosts = [definition.clone()];
    let tuple = TypedStorageContext::with_hosts(&[], &[], &hosts)
        .construct_tuple(
            &Type::Tuple(vec![Type::named("Item")]),
            vec![Value::HostObject(owner.clone())],
        )
        .unwrap();
    let Value::Dynamic(tuple) = tuple else {
        panic!("native tuple")
    };
    let projected = Value::from_reference(Rc::new(
        NativeInstancePlace::new(tuple)
            .unwrap()
            .field("0")
            .unwrap()
            .borrow(false, None)
            .unwrap(),
    ));
    for value in [Value::HostObject(owner.clone()), projected] {
        assert!(Rc::ptr_eq(
            &value.host_type_definition().unwrap().unwrap(),
            &definition
        ));
        let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(false)));
        slot.borrow_mut().initialize(value);
        let reference =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
        assert!(Rc::ptr_eq(
            &reference.host_type_definition().unwrap().unwrap(),
            &definition
        ));
        {
            let _writing = slot.borrow_mut();
            assert!(reference.host_type_definition().is_err());
        }
        slot.borrow_mut().clear();
        assert!(reference.host_type_definition().is_err());
    }
}

#[test]
fn host_declarations_preserve_module_identity_aliases_and_registry_ownership() {
    let root = Environment::global();
    let left = Environment::named_module_child(root.clone(), "left");
    let right = Environment::named_module_child(root.clone(), "right");
    let first = declaration("left::Item", false);
    let weak = Rc::downgrade(&first);
    left.borrow_mut().define(
        "Item",
        rils_execution::declaration::Declaration::HostType(first.clone()),
        false,
        None,
    );
    left.borrow_mut().define(
        "Alias",
        rils_execution::declaration::Declaration::HostType(first),
        false,
        None,
    );
    right.borrow_mut().define(
        "Item",
        rils_execution::declaration::Declaration::HostType(declaration("right::Item", true)),
        false,
        None,
    );
    let context = NativeOwnedContext::from_environment(&Environment::child(root.clone()).borrow());
    assert_eq!(context.hosts.len(), 2);
    assert!(
        !context
            .layout(&Type::Option(Box::new(Type::named("left::Item"))))
            .unwrap()
            .is_copy()
    );
    assert!(
        context
            .layout(&Type::Option(Box::new(Type::named("right::Item"))))
            .unwrap()
            .is_copy()
    );
    assert!(context.layout(&Type::named("Item")).is_err());
    assert!(
        NativeOwnedContext::from_environment(&Environment::global().borrow())
            .layout(&Type::named("left::Item"))
            .is_err()
    );
    drop(context);
    drop(left);
    assert!(weak.upgrade().is_some());
    assert_eq!(root.borrow().visible_host_definitions().len(), 2);
    drop(right);
    drop(root);
    assert!(weak.upgrade().is_none());
}

#[test]
fn non_copy_host_borrowed_reads_keep_identity_and_drop_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let root = Environment::global();
    let definition = declaration("Item", false);
    root.borrow_mut().define(
        "Item",
        rils_execution::declaration::Declaration::HostType(definition.clone()),
        false,
        None,
    );
    let drops = Rc::new(Cell::new(0));
    let owner = Rc::new(HostObject {
        type_definition: definition,
        payload: Rc::new(Probe(drops.clone())),
    });
    let context = NativeOwnedContext::from_environment(&root.borrow());
    let expected = Type::Option(Box::new(Type::named("Item")));
    let original = context
        .storage()
        .apply_declared(
            context
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::named("Item"))),
                    Some(Value::HostObject(owner.clone())),
                )
                .unwrap(),
            &expected,
        )
        .unwrap();
    assert!(!original.is_copy());
    let Value::Dynamic(object) = original else {
        panic!("native option")
    };
    let snapshot = object
        .with(|payload| runtime_layouts::clone_borrowed_view(payload.view()))
        .unwrap()
        .unwrap();
    assert!(!snapshot.descriptor().is_copy());
    assert!(snapshot.copy_owned().is_err());
    let snapshot = record_codec::from_native(snapshot).unwrap();
    assert!(matches!(snapshot, Value::Dynamic(_)));
    let snapshot = rils_execution::value::dynamic_option::take_owned(snapshot)
        .unwrap()
        .unwrap();
    let Value::HostObject(snapshot_owner) = &snapshot else {
        panic!("host leaf")
    };
    assert!(Rc::ptr_eq(snapshot_owner, &owner));
    drop(owner);
    drop(object);
    assert_eq!(drops.get(), 0);
    drop(snapshot);
    assert_eq!(drops.get(), 1);
}

#[test]
fn ref_cell_host_reads_and_writes_retain_the_registered_receiver() {
    use rils_execution::{runtime_builtins::call_native_owned_symbol, value::ReferenceValue};

    for copy in [false, true] {
        let definition = declaration("Item", copy);
        let context = NativeOwnedContext {
            hosts: vec![definition.clone()],
            ..Default::default()
        };
        let owner = Rc::new(HostObject {
            type_definition: definition.clone(),
            payload: Rc::new(Cell::new(1i32)),
        });
        let symbol = rils_builtins::builtin_member("RefCell", "new")
            .unwrap()
            .native_symbol
            .unwrap();
        let value =
            call_native_owned_symbol(symbol, vec![Value::HostObject(owner.clone())], &context)
                .unwrap()
                .unwrap();
        let Value::Dynamic(object) = value else {
            panic!("native RefCell")
        };
        let reference = ReferenceValue::new_dynamic_cell(&Value::Dynamic(object), true).unwrap();
        assert_eq!(reference.read().is_ok(), copy);
        let reference = Rc::new(reference);
        let borrowed = Value::from_reference(reference.clone());
        borrowed
            .with_native_view(|view| {
                view.with_rust::<Rc<HostObject>, _>(|observed| {
                    assert!(Rc::ptr_eq(observed, &owner));
                    observed
                        .payload
                        .downcast_ref::<Cell<i32>>()
                        .unwrap()
                        .set(42);
                })
            })
            .unwrap()
            .unwrap();
        assert_eq!(owner.payload.downcast_ref::<Cell<i32>>().unwrap().get(), 42);
        let replacement = Rc::new(HostObject {
            type_definition: definition,
            payload: Rc::new(Cell::new(43i32)),
        });
        reference
            .write(Value::HostObject(replacement.clone()))
            .unwrap();
        assert_eq!(reference.read().is_ok(), copy);
        borrowed
            .with_native_view(|view| {
                view.with_rust::<Rc<HostObject>, _>(|restored| {
                    assert!(Rc::ptr_eq(restored, &replacement));
                    assert_eq!(
                        restored.payload.downcast_ref::<Cell<i32>>().unwrap().get(),
                        43
                    );
                })
            })
            .unwrap()
            .unwrap();
    }
}
