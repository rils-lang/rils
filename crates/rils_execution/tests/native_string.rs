use rils_builtins::{TypePattern, builtin};
use rils_execution::{Value, runtime_builtins};

fn string(value: &str) -> Value {
    Value::from(rils_execution::value::native_string(value))
}

#[test]
fn generated_string_bridge_borrows_native_and_projected_receivers() {
    use rils_execution::{
        Type,
        environment::StorageSlot,
        value::{
            ReferenceValue, native_instance::NativeInstancePlace, storage::TypedStorageContext,
        },
    };
    use std::{cell::RefCell, rc::Rc};
    let context = TypedStorageContext::new(&[], &[]);
    let Value::Dynamic(tuple) = context
        .construct_tuple(
            &Type::Tuple(vec![Type::String, Type::String]),
            vec![string("éclair"), string("moved")],
        )
        .unwrap()
    else {
        panic!("native tuple")
    };
    let place = NativeInstancePlace::new(tuple.clone()).unwrap();
    drop(place.field("1").unwrap().take().unwrap());
    let projected = Value::from_reference(Rc::new(
        place.field("0").unwrap().borrow(false, None).unwrap(),
    ));
    let native_leaf = context
        .construct_option(
            &Type::Option(Box::new(Type::String)),
            Some(string("éclair")),
        )
        .unwrap();
    let native_leaf = rils_execution::value::dynamic_option::take_owned(native_leaf)
        .unwrap()
        .unwrap();
    for receiver in [string("éclair"), native_leaf, projected] {
        assert_eq!(
            call("core::string::string::len", std::slice::from_ref(&receiver))
                .unwrap()
                .get_cloned::<usize>()
                .unwrap(),
            7
        );
        assert!(
            call(
                "core::string::string::contains",
                &[receiver.clone(), string("é")]
            )
            .unwrap()
            .get_cloned::<bool>()
            .unwrap()
        );
        let slot = Rc::new(RefCell::new(StorageSlot::uninitialized(true)));
        slot.borrow_mut().initialize(receiver);
        let reference =
            Value::from_reference(Rc::new(ReferenceValue::new_storage(slot.clone(), false)));
        assert_eq!(
            call(
                "core::string::string::len",
                std::slice::from_ref(&reference)
            )
            .unwrap()
            .get_cloned::<usize>()
            .unwrap(),
            7
        );
        assert_eq!(reference.get_cloned::<String>().unwrap(), "éclair");
        let _writing = slot.borrow_mut();
        let error = call(
            "core::string::string::len",
            std::slice::from_ref(&reference),
        )
        .unwrap_err();
        assert!(error.contains("mutably accessed"), "{error}");
    }
    assert!(Value::Dynamic(tuple).is_partially_moved());
}

#[test]
fn string_uses_native_storage_across_clone_and_display() {
    let value = string("héllo");
    let Value::Native(object) = &value else {
        panic!("string must use native storage");
    };
    assert!(!object.is_inline());
    assert!(!value.is_copy());
    assert_eq!(object.descriptor().rils_type().to_string(), "string");
    assert_eq!(value.get_cloned::<String>().ok().as_deref(), Some("héllo"));
    assert_eq!(value.to_string(), "héllo");
    assert_eq!(format!("{value:?}"), "\"héllo\"");
    assert_eq!(value.clone_owned().unwrap(), value);
}

fn call(symbol: &str, arguments: &[Value]) -> Result<Value, String> {
    runtime_builtins::call_native_symbol(symbol, arguments).expect("exported string method")
}

#[test]
fn all_string_members_have_native_bindings() {
    let declaration = builtin("string").unwrap();
    let Value::Native(receiver) = string(" abc abc ") else {
        panic!("string receiver must use native storage");
    };
    for method in declaration.members {
        let symbol = method.native_symbol.unwrap();
        assert!(receiver.descriptor().has_method(symbol));
        let signature = method.signature.unwrap();
        let mut arguments = vec![string(" abc abc ")];
        for parameter in signature.parameters {
            arguments.push(match parameter {
                TypePattern::Usize => Value::from(rils_execution::value::native_usize(2)),
                _ => string("abc"),
            });
        }
        let result = call(symbol, &arguments);
        assert!(result.is_ok(), "{}: {result:?}", method.name);
    }
}

#[test]
fn string_native_methods_preserve_unicode_and_optional_results() {
    assert_eq!(
        call("core::string::string::len", &[string("é")]),
        Ok(Value::from(rils_execution::value::native_usize(2)))
    );
    assert_eq!(
        call("core::string::string::to_uppercase", &[string("é")]),
        Ok(string("É"))
    );
    assert_eq!(
        call("core::string::string::trim", &[string(" é ")]),
        Ok(string("é"))
    );
    assert!(matches!(
        call("core::string::string::trim", &[string(" é ")]).unwrap(),
        Value::Native(_)
    ));
    let found = call("core::string::string::find", &[string("éé"), string("é")]).unwrap();
    let Value::Dynamic(object) = &found else {
        panic!("string find must return native Option<usize>");
    };
    assert!(object.is_inline());
    assert_eq!(
        object.descriptor().layout().rils_type(),
        &rils_execution::Type::Option(Box::new(rils_execution::Type::USIZE))
    );
    assert_eq!(
        found
            .with_native_view(|view| view.option_item()?.with_rust::<usize, _>(|value| *value))
            .unwrap()
            .unwrap(),
        0
    );
}
