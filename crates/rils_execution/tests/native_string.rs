use std::rc::Rc;

use rils_builtins::{TypePattern, builtin};
use rils_execution::{Value, runtime_builtins};

fn string(value: &str) -> Value {
    Value::String(Rc::from(value))
}

fn call(symbol: &str, arguments: &[Value]) -> Result<Value, String> {
    runtime_builtins::call_native_symbol(symbol, arguments).expect("exported string method")
}

#[test]
fn all_string_members_have_native_bindings() {
    let declaration = builtin("string").unwrap();
    for method in declaration.members {
        let symbol = method.native_symbol.unwrap();
        assert!(method.builtin_id.is_none());
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
    assert_eq!(
        call("core::string::string::find", &[string("éé"), string("é")]),
        Ok(Value::Option {
            value: Some(Rc::new(Value::Usize(0))),
            element_type: Some(rils_execution::Type::USIZE)
        }),
    );
}
