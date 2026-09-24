use std::rc::Rc;

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
            runtime_builtins::call_native_symbol(
                "core::result::result::is_ok",
                std::slice::from_ref(&input)
            ),
            Some(Ok(Value::Bool(expected_ok.is_some())))
        );
        assert_eq!(
            runtime_builtins::call_native_symbol(
                "core::result::result::is_err",
                std::slice::from_ref(&input)
            ),
            Some(Ok(Value::Bool(expected_err.is_some())))
        );
        assert_eq!(
            runtime_builtins::call_native_symbol(
                "core::result::result::ok",
                std::slice::from_ref(&input)
            ),
            Some(Ok(Value::Option {
                value: expected_ok,
                element_type: Some(Type::I32),
            }))
        );
        assert_eq!(
            runtime_builtins::call_native_symbol("core::result::result::err", &[input]),
            Some(Ok(Value::Option {
                value: expected_err,
                element_type: Some(Type::String),
            }))
        );
    }
}

#[test]
fn native_result_methods_reject_invalid_receivers_and_arities() {
    for symbol in [
        "core::result::result::is_ok",
        "core::result::result::is_err",
        "core::result::result::ok",
        "core::result::result::err",
    ] {
        assert!(
            runtime_builtins::call_native_symbol(symbol, &[Value::I32(5)])
                .unwrap()
                .unwrap_err()
                .contains("expects Result")
        );
        assert!(
            runtime_builtins::call_native_symbol(symbol, &[])
                .unwrap()
                .unwrap_err()
                .contains("one receiver")
        );
    }
}
