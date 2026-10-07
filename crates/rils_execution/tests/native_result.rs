use rils_execution::{
    Type, Value,
    runtime_builtins::{self, NativeOwnedContext},
};

#[test]
fn native_result_methods_cover_both_variants_and_preserve_option_types() {
    let context = NativeOwnedContext::default();
    let result_type = Type::Result(Box::new(Type::I32), Box::new(Type::String));
    for success in [true, false] {
        let input = || {
            context
                .storage()
                .construct_result(
                    &result_type,
                    if success {
                        Ok(Value::from_i32(7))
                    } else {
                        Err(Value::from_string("failure"))
                    },
                )
                .unwrap()
        };
        let query_input = input();
        assert_eq!(
            runtime_builtins::call_native_symbol(
                "core::result::result::is_ok",
                std::slice::from_ref(&query_input)
            ),
            Some(Ok(Value::Bool(success)))
        );
        assert_eq!(
            runtime_builtins::call_native_symbol(
                "core::result::result::is_err",
                std::slice::from_ref(&query_input)
            ),
            Some(Ok(Value::Bool(!success)))
        );
        assert_eq!(
            runtime_builtins::call_native_owned_symbol(
                "core::result::result::ok",
                vec![input()],
                &context,
            ),
            Some(Ok(context
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::I32)),
                    success.then(|| Value::from_i32(7)),
                )
                .unwrap()))
        );
        assert_eq!(
            runtime_builtins::call_native_owned_symbol(
                "core::result::result::err",
                vec![input()],
                &context
            ),
            Some(Ok(context
                .storage()
                .construct_option(
                    &Type::Option(Box::new(Type::String)),
                    (!success).then(|| Value::from_string("failure")),
                )
                .unwrap()))
        );
    }
}

#[test]
fn native_result_methods_reject_invalid_receivers_and_arities() {
    let context = NativeOwnedContext::default();
    for symbol in [
        "core::result::result::is_ok",
        "core::result::result::is_err",
        "core::result::result::ok",
        "core::result::result::err",
    ] {
        let call = |arguments: Vec<Value>| {
            if runtime_builtins::requires_owned_native_call(symbol) {
                runtime_builtins::call_native_owned_symbol(symbol, arguments, &context)
            } else {
                runtime_builtins::call_native_symbol(symbol, &arguments)
            }
        };
        assert!(
            call(vec![Value::from_i32(5)])
                .unwrap()
                .unwrap_err()
                .contains("expects Result")
        );
        let error = call(vec![]).unwrap().unwrap_err();
        if runtime_builtins::requires_owned_native_call(symbol) {
            assert!(error.contains("1 arguments"));
        } else {
            assert!(error.contains("one receiver"));
        }
    }
}
