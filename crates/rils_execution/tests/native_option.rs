use rils_execution::{Type, Value, runtime_builtins};

#[test]
fn native_option_is_some_handles_variants_and_invalid_receivers() {
    for is_some in [true, false] {
        let input = option(is_some);
        assert_eq!(
            runtime_builtins::call_native_symbol(
                "core::option::option::is_some",
                std::slice::from_ref(&input)
            ),
            Some(Ok(Value::from(rils_execution::value::native_bool(is_some))))
        );
        assert_eq!(
            runtime_builtins::call_native_symbol("core::option::option::is_none", &[input]),
            Some(Ok(Value::from(rils_execution::value::native_bool(
                !is_some
            ))))
        );
    }
    let error = runtime_builtins::call_native_symbol(
        "core::option::option::is_some",
        &[Value::from(rils_execution::value::native_i32(5))],
    )
    .unwrap()
    .unwrap_err();
    assert!(error.contains("expects Option"));
    let error = runtime_builtins::call_native_symbol("core::option::option::is_some", &[])
        .unwrap()
        .unwrap_err();
    assert!(error.contains("one receiver"));
}

#[test]
fn native_option_symbol_dispatch_uses_the_exported_declaration() {
    let input = option(true);
    assert_eq!(
        runtime_builtins::call_native_symbol("core::option::option::is_some", &[input]),
        Some(Ok(Value::from(rils_execution::value::native_bool(true))))
    );
    assert_eq!(
        runtime_builtins::call_native_symbol("core::option::missing", &[]),
        None
    );
}

fn option(present: bool) -> Value {
    rils_execution::value::storage::TypedStorageContext::new(&[], &[])
        .construct_option(
            &Type::Option(Box::new(Type::I32)),
            present.then(|| Value::from(rils_execution::value::native_i32(5))),
        )
        .unwrap()
}
