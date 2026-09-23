use std::rc::Rc;

use rils_builtins::{BuiltinId, TypePattern, builtin};
use rils_execution::{Value, runtime_builtins};

fn string(value: &str) -> Value {
    Value::String(Rc::from(value))
}

#[test]
fn all_string_members_have_native_bindings() {
    let declaration = builtin("string").unwrap();
    for method in declaration.members {
        let id = method.builtin_id.unwrap();
        let signature = method.signature.unwrap();
        let mut arguments = vec![string(" abc abc ")];
        for parameter in signature.parameters {
            arguments.push(match parameter {
                TypePattern::Usize => Value::Usize(2),
                _ => string("abc"),
            });
        }
        let result = runtime_builtins::call(id, &arguments);
        assert!(result.is_ok(), "{}: {result:?}", method.name);
    }
}

#[test]
fn string_native_methods_preserve_unicode_and_optional_results() {
    assert_eq!(
        runtime_builtins::call(BuiltinId::StringLen, &[string("é")]),
        Ok(Value::Usize(2))
    );
    assert_eq!(
        runtime_builtins::call(BuiltinId::StringToUppercase, &[string("é")]),
        Ok(string("É"))
    );
    assert_eq!(
        runtime_builtins::call(BuiltinId::StringTrim, &[string(" é ")]),
        Ok(string("é"))
    );
    assert_eq!(
        runtime_builtins::call(BuiltinId::StringFind, &[string("éé"), string("é")]),
        Ok(Value::Option {
            value: Some(Rc::new(Value::Usize(0))),
            element_type: Some(rils_execution::Type::USIZE)
        }),
    );
}
