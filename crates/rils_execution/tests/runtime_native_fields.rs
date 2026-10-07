use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    rc::Rc,
};

use rils_execution::{
    Type, Value,
    environment::{Environment, StorageSlot},
    value::{
        BytecodeFunctionValue, HostFunction, HostObject, HostType, ReferenceValue, record_codec,
        record_layout::RecordLayoutResolver, runtime_layouts::HostLayoutProvider,
        storage::TypedStorageContext,
    },
};
use rils_value::{DynamicLayout, DynamicObject, DynamicType, DynamicValue};

#[test]
fn native_reference_fields_retain_the_lease_aliases_and_lexical_origin() {
    let environment = Environment::global();
    environment
        .borrow_mut()
        .define("number", Value::from_i32(1), true, Some(Type::I32));
    let storage = environment.borrow().slot("number").unwrap();
    let reference = Rc::new(ReferenceValue::new_storage(storage.clone(), true));
    let reference_type = Type::Reference {
        mutable: true,
        inner: Box::new(Type::I32),
    };
    let expected = Type::Option(Box::new(reference_type.clone()));
    let value = Value::Option {
        value: Some(Rc::new(Value::Reference(reference.clone()))),
        element_type: Some(reference_type),
    };
    let value = TypedStorageContext::new(&[], &[])
        .apply_declared(value, &expected)
        .unwrap();
    assert!(matches!(value, Value::Dynamic(_)));
    assert!(value.is_copy());
    assert!(value.contains_reference());
    assert!(value.contains_local_reference(&environment));
    assert!(!value.contains_local_reference(&Environment::global()));
    let copied = value.clone_owned().unwrap();
    drop(reference);
    drop(value);
    let Value::Dynamic(object) = copied else {
        panic!("native Copy retains layout")
    };
    let payload = object.into_value().map_err(|error| error.1).unwrap();
    let child = payload.take_option().unwrap().unwrap();
    let Value::Reference(reference) = record_codec::from_native(child).unwrap() else {
        panic!("reference leaf")
    };
    reference.write(Value::from_i32(42)).unwrap();
    assert_eq!(storage.borrow().read().unwrap(), Value::from_i32(42));
    drop(reference);
    assert_eq!(storage.borrow_mut().take().unwrap(), Value::from_i32(42));
}

#[test]
fn callable_fields_preserve_signature_handler_identity_and_owned_conversion() {
    let callback_type = Type::function(vec![Type::I32], Type::I32);
    let callback = Rc::new(HostFunction {
        name: "increment".into(),
        min_arity: 1,
        max_arity: 1,
        signature: Some(rils_execution::FunctionSignature::fixed(
            vec![Type::I32],
            Type::I32,
        )),
        function: Rc::new(|arguments| Ok(Value::from_i32(arguments[0].as_i32().unwrap() + 1))),
    });
    let layout = RecordLayoutResolver::new(&[])
        .resolve(&callback_type)
        .unwrap();
    assert!(layout.is_copy());
    let payload = record_codec::into_native(Value::HostFunction(callback.clone()), layout).unwrap();
    assert!(
        payload.is_inline(),
        "Copy call targets should fit the inline layout"
    );
    let copied = payload.copy_owned().unwrap();
    drop(payload);
    let Value::HostFunction(restored) = record_codec::from_native(copied).unwrap() else {
        panic!("call target")
    };
    assert!(Rc::ptr_eq(&restored, &callback));
    assert_eq!(
        (restored.function)(&[Value::from_i32(41)]).unwrap(),
        Value::from_i32(42)
    );
    let wrong = RecordLayoutResolver::new(&[])
        .resolve(&Type::function(vec![Type::String], Type::I32))
        .unwrap();
    assert!(record_codec::into_native(Value::HostFunction(callback), wrong).is_err());
}

#[test]
fn host_fields_use_registered_copy_policy_and_drop_the_payload_once() {
    struct Probe(Rc<Cell<usize>>);
    impl Drop for Probe {
        fn drop(&mut self) {
            self.0.set(self.0.get() + 1);
        }
    }
    for copy in [false, true] {
        let drops = Rc::new(Cell::new(0));
        let definition = Rc::new(HostType {
            name: "Host".into(),
            base_types: HashSet::new(),
            copy,
            methods: RefCell::new(HashMap::new()),
        });
        let definitions = [definition.clone()];
        let provider = HostLayoutProvider::new(&definitions);
        let ty = Type::Option(Box::new(Type::named("Host")));
        let mut resolver = RecordLayoutResolver::with_provider(&[], &[], Some(&provider));
        let layout = resolver.resolve(&ty).unwrap();
        assert_eq!(layout.is_copy(), copy);
        let host = Rc::new(HostObject {
            type_definition: definition,
            payload: Rc::new(Probe(drops.clone())),
        });
        let value = Value::Option {
            value: Some(Rc::new(Value::HostObject(host.clone()))),
            element_type: Some(Type::named("Host")),
        };
        let payload = record_codec::into_native(value, layout).unwrap();
        let copied = payload.copy_owned();
        assert_eq!(copied.is_ok(), copy);
        drop(copied);
        let restored = record_codec::from_native(payload).unwrap();
        let Value::Option {
            value: Some(restored_owner),
            ..
        } = restored
        else {
            panic!("host option")
        };
        let Value::HostObject(restored) = restored_owner.as_ref() else {
            panic!("host leaf")
        };
        assert!(Rc::ptr_eq(restored, &host));
        drop(host);
        assert_eq!(drops.get(), 0);
        drop(restored_owner);
        assert_eq!(drops.get(), 1);
    }
}

#[test]
fn unregistered_and_ambiguous_host_declarations_fail_layout_resolution() {
    let definition = || {
        Rc::new(HostType {
            name: "Host".into(),
            base_types: HashSet::new(),
            copy: true,
            methods: RefCell::default(),
        })
    };
    assert!(
        RecordLayoutResolver::new(&[])
            .resolve(&Type::named("Host"))
            .is_err()
    );
    let definitions = [definition(), definition()];
    let provider = HostLayoutProvider::new(&definitions);
    let error = RecordLayoutResolver::with_provider(&[], &[], Some(&provider))
        .resolve(&Type::named("Host"))
        .err()
        .unwrap();
    assert!(error.contains("ambiguous"));
}

#[test]
fn reference_codec_rejects_mutability_or_referent_type_mismatch() {
    for expected in [
        Type::Reference {
            mutable: true,
            inner: Box::new(Type::I32),
        },
        Type::Reference {
            mutable: false,
            inner: Box::new(Type::String),
        },
    ] {
        let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        storage.borrow_mut().initialize(Value::from_i32(1));
        let reference = Rc::new(ReferenceValue::new_storage(storage, false));
        let layout = RecordLayoutResolver::new(&[]).resolve(&expected).unwrap();
        assert!(record_codec::into_native(Value::Reference(reference), layout).is_err());
    }
}

#[test]
fn raw_native_record_inspects_reference_fields_without_materializing_values() {
    let storage = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
    storage.borrow_mut().initialize(Value::from_string("owned"));
    let reference_type = Type::Reference {
        mutable: false,
        inner: Box::new(Type::String),
    };
    let layout = RecordLayoutResolver::new(&[])
        .resolve(&reference_type)
        .unwrap();
    let record = DynamicLayout::record(
        Type::named("Record"),
        vec![("reference".into(), layout.clone())],
    )
    .unwrap();
    let reference = Rc::new(ReferenceValue::new_storage(storage.clone(), false));
    let child = record_codec::into_native(Value::Reference(reference), layout).unwrap();
    let payload = DynamicValue::record(record.clone(), vec![child]).unwrap();
    let object = DynamicObject::new(Rc::new(DynamicType::new(record)), payload).unwrap();
    let value = Value::Dynamic(object);
    assert!(value.contains_reference());
    assert!(storage.borrow_mut().take().is_err());
    drop(value);
    assert!(storage.borrow_mut().take().is_ok());
}

#[test]
fn native_callable_fields_keep_reference_origins_in_captures_and_bound_arguments() {
    for captured in [false, true] {
        let environment = Environment::global();
        environment.borrow_mut().define(
            "source",
            Value::from_string("owned"),
            true,
            Some(Type::String),
        );
        let source = environment.borrow().slot("source").unwrap();
        let reference =
            Value::Reference(Rc::new(ReferenceValue::new_storage(source.clone(), false)));
        let (captures, bound_arguments) = if captured {
            let mut slot = StorageSlot::uninitialized(false);
            slot.initialize(reference);
            (vec![Rc::new(RefCell::new(slot))], vec![])
        } else {
            (vec![], vec![reference])
        };
        let target = Value::BytecodeFunction(Rc::new(BytecodeFunctionValue {
            function: 0,
            name: "callback".into(),
            parameter_count: 0,
            captures,
            bound_arguments,
            type_bindings: Default::default(),
        }));
        let callback_type = Type::function(vec![], Type::I32);
        let value = Value::Option {
            value: Some(Rc::new(target)),
            element_type: Some(callback_type.clone()),
        };
        let expected = Type::Option(Box::new(callback_type));
        let value = TypedStorageContext::new(&[], &[])
            .apply_declared(value, &expected)
            .unwrap();
        assert!(matches!(value, Value::Dynamic(_)));
        assert!(value.contains_reference());
        assert!(value.contains_local_reference(&environment));
        assert!(!value.contains_local_reference(&Environment::global()));
        let copied = value.clone_owned().unwrap();
        drop(value);
        assert!(copied.contains_local_reference(&environment));
        assert!(source.borrow_mut().take().is_err());
        drop(copied);
        assert!(source.borrow_mut().take().is_ok());
    }
}
