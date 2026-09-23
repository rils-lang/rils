use std::rc::Rc;

use rils_builtins::BuiltinId;
use rils_execution::{Type, Value, runtime_builtins};

#[test]
fn native_result_methods_cover_both_variants_and_preserve_option_types() {
    for (value, expected_ok, expected_err) in [
        (
            Ok(Rc::new(Value::I32(7))),
            Some(Rc::new(Value::I32(7))),
            None,
        ),
        (
            Err(Rc::new(Value::String(Rc::from("failure")))),
            None,
            Some(Rc::new(Value::String(Rc::from("failure")))),
        ),
    ] {
        let input = Value::Result {
            value,
            ok_type: Some(Type::I32),
            error_type: Some(Type::String),
        };
        assert_eq!(
            runtime_builtins::call(BuiltinId::ResultIsOk, std::slice::from_ref(&input)),
            Ok(Value::Bool(expected_ok.is_some()))
        );
        assert_eq!(
            runtime_builtins::call(BuiltinId::ResultIsErr, std::slice::from_ref(&input)),
            Ok(Value::Bool(expected_err.is_some()))
        );
        assert_eq!(
            runtime_builtins::call(BuiltinId::ResultOk, std::slice::from_ref(&input)),
            Ok(Value::Option {
                value: expected_ok,
                element_type: Some(Type::I32),
            })
        );
        assert_eq!(
            runtime_builtins::call(BuiltinId::ResultErr, &[input]),
            Ok(Value::Option {
                value: expected_err,
                element_type: Some(Type::String),
            })
        );
    }
}

#[test]
fn native_result_methods_reject_invalid_receivers_and_arities() {
    for id in [
        BuiltinId::ResultIsOk,
        BuiltinId::ResultIsErr,
        BuiltinId::ResultOk,
        BuiltinId::ResultErr,
    ] {
        assert!(
            runtime_builtins::call(id, &[Value::I32(5)])
                .unwrap_err()
                .contains("expects Result")
        );
        assert!(
            runtime_builtins::call(id, &[])
                .unwrap_err()
                .contains("one receiver")
        );
    }
}
