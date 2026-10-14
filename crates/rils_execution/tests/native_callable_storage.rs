use std::{cell::RefCell, collections::HashMap, rc::Rc};

use rils_execution::{
    Type, Value,
    environment::{Environment, StorageSlot},
    value::{
        BuiltinBoundMethod, BuiltinMethod, BytecodeFunctionValue, NativeFunction, ReferenceValue,
        native_instance::NativeInstancePlace, storage::TypedStorageContext,
    },
};

fn function(captures: Vec<rils_execution::environment::StorageRef>) -> Rc<BytecodeFunctionValue> {
    Rc::new(BytecodeFunctionValue {
        function: 0,
        name: "test_target".into(),
        parameter_count: 1,
        signature: Type::opaque_function(),
        captures,
        bound_arguments: Vec::new(),
        type_bindings: HashMap::new(),
    })
}

fn tuple(value: Value, ty: Type) -> Value {
    TypedStorageContext::new(&[], &[])
        .construct_tuple(&Type::Tuple(vec![ty]), vec![value])
        .unwrap()
}

fn field(value: Value) -> Value {
    let Value::Dynamic(object) = value else {
        panic!("native tuple expected")
    };
    NativeInstancePlace::new(object)
        .unwrap()
        .field("0")
        .unwrap()
        .take()
        .unwrap()
}

#[test]
fn native_call_handles_retain_target_identity_and_contextual_signature() {
    let target = function(Vec::new());
    let value = Value::from_callable(target.clone());
    assert!(matches!(value, Value::Dynamic(_)));
    assert!(value.is_copy());
    let signature = Type::function(vec![Type::I32], Type::I32);
    let value = tuple(value, signature.clone());
    let value = field(value.clone_owned().unwrap());
    assert_eq!(Type::of_value(&value), Some(signature));
    assert!(Rc::ptr_eq(
        &target,
        &value.as_callable::<BytecodeFunctionValue>().unwrap()
    ));
    assert_eq!(value.to_string(), "<fn test_target>");
}

#[test]
fn native_callable_captures_keep_local_reference_provenance_in_containers() {
    let environment = Environment::global();
    environment.borrow_mut().define(
        "number",
        Value::from(rils_execution::value::native_i32(42)),
        true,
        None,
    );
    let slot = environment.borrow().slot("number").unwrap();
    let reference = Value::from_reference(Rc::new(ReferenceValue::new_storage(slot, true)));
    let mut capture = StorageSlot::uninitialized(false);
    capture.initialize(reference);
    let target = function(vec![Rc::new(RefCell::new(capture))]);
    let value = tuple(
        Value::from_callable(target),
        Type::function(vec![Type::I32], Type::I32),
    );
    assert!(value.contains_reference());
    assert!(value.contains_local_reference(&environment));
    let value = field(value);
    assert!(value.contains_local_reference(&environment));
}

#[test]
fn consuming_call_handles_release_the_wrapper_and_preserve_failed_downcasts() {
    let value = Value::from_callable(Rc::new(BuiltinBoundMethod {
        receiver: Rc::new(Value::from(rils_execution::value::native_string("owned"))),
        method: BuiltinMethod::IteratorIdentity,
    }));
    assert!(!value.is_copy());
    assert!(value.clone_owned().is_err());
    let value = tuple(value, Type::function(Vec::new(), Type::String));
    assert!(!value.is_copy());
    assert!(value.clone_owned().is_err());
    let value = field(value);
    let value = *value
        .into_callable::<NativeFunction>()
        .err()
        .expect("wrong target type");
    let method = value.into_callable::<BuiltinBoundMethod>().unwrap();
    let method = Rc::try_unwrap(method)
        .ok()
        .expect("unique target after consuming handle");
    let receiver = Rc::try_unwrap(method.receiver).unwrap();
    assert_eq!(
        receiver.get_cloned::<String>().ok().as_deref(),
        Some("owned")
    );
}
