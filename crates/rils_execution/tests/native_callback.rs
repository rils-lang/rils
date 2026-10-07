use rils_execution::{
    Type, Value,
    runtime_builtins::{NativeCallError, NativeOwnedContext, call_native_symbol_with_callback},
};

#[derive(Debug, PartialEq, Eq)]
struct CallbackFailure;

#[test]
fn callback_and_bridge_errors_keep_distinct_types() {
    let symbol = "core::option::option::map";
    let present = native_option(Some(Value::from_i32(1)), Type::I32);
    let failure = call_native_symbol_with_callback(
        symbol,
        vec![present, Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::Option(Box::new(Type::I32))),
        &mut |_, _| Err::<Value, _>(CallbackFailure),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(
        failure,
        NativeCallError::Callback(CallbackFailure)
    ));

    let failure = call_native_symbol_with_callback(
        symbol,
        vec![Value::from_i32(1), Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::Option(Box::new(Type::I32))),
        &mut |_, _| Ok::<_, CallbackFailure>(Value::from_i32(2)),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(failure, NativeCallError::Bridge(_)));
}

#[test]
fn filter_passes_a_shared_reference_and_skips_absent_values() {
    let symbol = "core::option::option::filter";
    let present = native_option(Some(Value::from_i32(4)), Type::I32);
    let mut calls = 0;
    let kept = call_native_symbol_with_callback(
        symbol,
        vec![present, Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::Option(Box::new(Type::I32))),
        &mut |_, arguments| {
            calls += 1;
            let Value::Reference(reference) = &arguments[0] else {
                panic!("filter predicate must receive a shared reference");
            };
            assert!(!reference.mutable);
            assert_eq!(reference.read().unwrap(), Value::from_i32(4));
            Ok::<_, CallbackFailure>(Value::Bool(true))
        },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(&kept, Value::Dynamic(_)));
    assert!(
        rils_execution::value::dynamic_option::take_owned(kept)
            .unwrap()
            .is_some()
    );
    assert_eq!(calls, 1);

    let absent = native_option(None, Type::I32);
    let skipped = call_native_symbol_with_callback(
        symbol,
        vec![absent, Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::Option(Box::new(Type::I32))),
        &mut |_, _| -> Result<Value, CallbackFailure> {
            panic!("absent values must not invoke the predicate")
        },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(&skipped, Value::Dynamic(_)));
    assert!(
        rils_execution::value::dynamic_option::take_owned(skipped)
            .unwrap()
            .is_none()
    );

    let present = native_option(Some(Value::from_i32(4)), Type::I32);
    let failure = call_native_symbol_with_callback(
        symbol,
        vec![present, Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::Option(Box::new(Type::I32))),
        &mut |_, _| Ok::<_, CallbackFailure>(Value::from_i32(1)),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(failure, NativeCallError::Bridge(message) if message.contains("bool")));
}

#[test]
fn free_function_callback_symbols_are_registered() {
    let result = call_native_symbol_with_callback(
        "core::ops::apply_twice",
        vec![Value::from_i32(1), Value::from_i32(0)],
        &NativeOwnedContext::default(),
        Some(&Type::I32),
        &mut |_, values| {
            let value = values[0].as_i32().expect("expected integer");
            Ok::<_, CallbackFailure>(Value::from_i32(value + 1))
        },
    );
    assert!(result.is_some());
    assert_eq!(result.unwrap().unwrap(), Value::from_i32(3));
}

#[test]
fn free_function_bridge_distinguishes_callback_and_arity_errors() {
    let symbol = "core::ops::chain";
    let arguments = [Value::from_i32(0), Value::from_i32(4), Value::from_i32(1)];
    let callback_error = call_native_symbol_with_callback(
        symbol,
        arguments.to_vec(),
        &NativeOwnedContext::default(),
        Some(&Type::I32),
        &mut |_, _| Err::<Value, _>(CallbackFailure),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(
        callback_error,
        NativeCallError::Callback(CallbackFailure)
    ));

    let arity_error = call_native_symbol_with_callback(
        symbol,
        arguments[..2].to_vec(),
        &NativeOwnedContext::default(),
        Some(&Type::I32),
        &mut |_, _| Ok::<_, CallbackFailure>(Value::from_i32(1)),
    )
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(arity_error, NativeCallError::Bridge(message) if message.contains("expects 3 arguments"))
    );
}

fn native_option(value: Option<Value>, item: Type) -> Value {
    NativeOwnedContext::default()
        .storage()
        .construct_option(&Type::Option(Box::new(item)), value)
        .unwrap()
}

#[test]
fn callback_transport_transfers_unique_native_owners() {
    let context = NativeOwnedContext::default();
    let expected = Type::Option(Box::new(Type::String));
    let receiver = native_option(Some(Value::from_string("owned")), Type::String);
    let result = call_native_symbol_with_callback(
        "core::option::option::map",
        vec![receiver, Value::Unit],
        &context,
        Some(&expected),
        &mut |_, arguments| {
            let value = arguments.into_iter().next().unwrap();
            let Value::Native(object) = value else {
                panic!("native string expected")
            };
            let string = object
                .into_rust::<rils_stdlib::stdlib::string::String>()
                .map_err(|failure| failure.1)?;
            assert_eq!(std::string::String::from(string), "owned");
            Ok::<_, String>(Value::from_string("returned"))
        },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(&result, Value::Dynamic(_)));
    let item = rils_execution::value::dynamic_option::take_owned(result)
        .unwrap()
        .unwrap();
    let Value::Native(object) = item else {
        panic!("native string expected")
    };
    assert_eq!(
        std::string::String::from(
            object
                .into_rust::<rils_stdlib::stdlib::string::String>()
                .map_err(|failure| failure.1)
                .unwrap()
        ),
        "returned"
    );

    let result = call_native_symbol_with_callback(
        "core::ops::apply_twice",
        vec![Value::from_string("free"), Value::Unit],
        &context,
        Some(&Type::String),
        &mut |_, arguments| {
            let Value::Native(object) = arguments.into_iter().next().unwrap() else {
                panic!("native string expected")
            };
            let string = object
                .into_rust::<rils_stdlib::stdlib::string::String>()
                .map_err(|failure| failure.1)?;
            Ok::<_, String>(Value::from_string(std::string::String::from(string)))
        },
    )
    .unwrap()
    .unwrap();
    let Value::Native(object) = result else {
        panic!("native string expected")
    };
    assert_eq!(
        std::string::String::from(
            object
                .into_rust::<rils_stdlib::stdlib::string::String>()
                .map_err(|failure| failure.1)
                .unwrap()
        ),
        "free"
    );
}

#[test]
fn skipped_callbacks_require_complete_valid_output_layouts() {
    let context = NativeOwnedContext::default();
    for expected in [
        None,
        Some(Type::Option(Box::new(Type::Unknown))),
        Some(Type::Option(Box::new(Type::named("Missing")))),
    ] {
        let failure = call_native_symbol_with_callback(
            "core::option::option::map",
            vec![native_option(None, Type::I32), Value::Unit],
            &context,
            expected.as_ref(),
            &mut |_, _| -> Result<Value, CallbackFailure> { panic!("must not invoke callback") },
        )
        .unwrap()
        .unwrap_err();
        assert!(matches!(failure, NativeCallError::Bridge(_)));
    }
    let expected = Type::Option(Box::new(Type::String));
    let result = call_native_symbol_with_callback(
        "core::option::option::map",
        vec![native_option(None, Type::I32), Value::Unit],
        &context,
        Some(&expected),
        &mut |_, _| -> Result<Value, CallbackFailure> { panic!("must not invoke callback") },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(&result, Value::Dynamic(_)));
    assert_eq!(Type::of_value(&result), Some(expected));
    assert!(
        rils_execution::value::dynamic_option::take_owned(result)
            .unwrap()
            .is_none()
    );
}

#[test]
fn shared_non_copy_receivers_and_wrong_callback_results_are_rejected() {
    let context = NativeOwnedContext::default();
    let expected = Type::Option(Box::new(Type::String));
    let retained = native_option(Some(Value::from_string("retained")), Type::String);
    let error = call_native_symbol_with_callback(
        "core::option::option::map",
        vec![retained.clone(), Value::Unit],
        &context,
        Some(&expected),
        &mut |_, _| -> Result<Value, CallbackFailure> { panic!("must not invoke callback") },
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(error, NativeCallError::Bridge(message) if message.contains("shared")));
    assert_eq!(
        retained
            .as_option()
            .unwrap()
            .0
            .unwrap()
            .as_string()
            .as_deref(),
        Some("retained")
    );
    let error = call_native_symbol_with_callback(
        "core::option::option::map",
        vec![
            native_option(Some(Value::from_i32(1)), Type::I32),
            Value::Unit,
        ],
        &context,
        Some(&expected),
        &mut |_, _| Ok::<_, CallbackFailure>(Value::Bool(true)),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(error, NativeCallError::Bridge(message) if message.contains("string")));
}

#[test]
fn retaining_a_predicate_borrow_prevents_transfer_of_its_non_copy_owner() {
    let context = NativeOwnedContext::default();
    let expected = Type::Option(Box::new(Type::String));
    let mut retained = None;
    let error = call_native_symbol_with_callback(
        "core::option::option::filter",
        vec![
            native_option(Some(Value::from_string("borrowed")), Type::String),
            Value::Unit,
        ],
        &context,
        Some(&expected),
        &mut |_, arguments| {
            retained = arguments.into_iter().next();
            Ok::<_, CallbackFailure>(Value::Bool(true))
        },
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(error, NativeCallError::Bridge(message) if message.contains("shared")));
    let Value::Reference(reference) = retained.unwrap() else {
        panic!("predicate reference")
    };
    assert_eq!(
        reference.read().unwrap().as_string().as_deref(),
        Some("borrowed")
    );
}
