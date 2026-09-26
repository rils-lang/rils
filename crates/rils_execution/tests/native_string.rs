use std::rc::Rc;

use rils_builtins::{TypePattern, builtin};
use rils_execution::{Value, runtime_builtins, value::HashKey};

fn string(value: &str) -> Value {
    Value::from_string(value)
}

#[test]
fn string_uses_native_storage_across_clone_display_and_hash_keys() {
    let value = string("héllo");
    let Value::Native(object) = &value else {
        panic!("string must use native storage");
    };
    assert!(!object.is_inline());
    assert!(!value.is_copy());
    assert_eq!(object.descriptor().rils_type().to_string(), "string");
    assert_eq!(value.as_string().as_deref(), Some("héllo"));
    assert_eq!(value.to_string(), "héllo");
    assert_eq!(format!("{value:?}"), "\"héllo\"");
    assert_eq!(value.clone_owned().unwrap(), value);
    assert_eq!(
        HashKey::from_value(&value).unwrap(),
        HashKey::String(Rc::from("héllo"))
    );
    assert!(matches!(
        HashKey::from_value(&value).unwrap().to_value(),
        Value::Native(_)
    ));
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
        assert!(method.builtin_id.is_none());
        assert!(receiver.descriptor().has_method(symbol));
        let signature = method.signature.unwrap();
        let mut arguments = vec![string(" abc abc ")];
        for parameter in signature.parameters {
            arguments.push(match parameter {
                TypePattern::Usize => Value::Usize(2),
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
        Ok(Value::Usize(2))
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
        found.as_option(),
        Some((Some(Value::Usize(0)), rils_execution::Type::USIZE))
    );
}
