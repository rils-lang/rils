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
fn host_declarations_preserve_module_identity_aliases_and_weak_ownership() {
    let root = Environment::global();
    let left = Environment::named_module_child(root.clone(), "left");
    let right = Environment::named_module_child(root.clone(), "right");
    let first = declaration("left::Item", false);
    let weak = Rc::downgrade(&first);
    left.borrow_mut()
        .define("Item", Value::HostType(first.clone()), false, None);
    left.borrow_mut()
        .define("Alias", Value::HostType(first), false, None);
    right.borrow_mut().define(
        "Item",
        Value::HostType(declaration("right::Item", true)),
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
    assert!(weak.upgrade().is_none());
    assert_eq!(root.borrow().visible_host_definitions().len(), 1);
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
    root.borrow_mut()
        .define("Item", Value::HostType(definition.clone()), false, None);
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
            Value::Option {
                value: Some(Rc::new(Value::HostObject(owner.clone()))),
                element_type: Some(Type::named("Item")),
            },
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
    let Value::Option {
        value: Some(snapshot),
        ..
    } = snapshot
    else {
        panic!("borrowed option")
    };
    let Value::HostObject(snapshot_owner) = snapshot.as_ref() else {
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
        let reference =
            ReferenceValue::new_dynamic_cell(object, true, None, vec![], vec![]).unwrap();
        let snapshot = reference.read().unwrap();
        let Value::HostObject(snapshot_owner) = &snapshot else {
            panic!("registered host receiver")
        };
        assert!(Rc::ptr_eq(snapshot_owner, &owner));
        snapshot.host_payload::<Cell<i32>>().unwrap().set(42);
        assert_eq!(owner.payload.downcast_ref::<Cell<i32>>().unwrap().get(), 42);
        assert_eq!(snapshot.is_copy(), copy);
        let replacement = Rc::new(HostObject {
            type_definition: definition,
            payload: Rc::new(Cell::new(43i32)),
        });
        reference
            .write(Value::HostObject(replacement.clone()))
            .unwrap();
        let Value::HostObject(restored) = reference.read().unwrap() else {
            panic!("replacement receiver")
        };
        assert!(Rc::ptr_eq(&restored, &replacement));
        assert_eq!(
            restored.payload.downcast_ref::<Cell<i32>>().unwrap().get(),
            43
        );
    }
}
