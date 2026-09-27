use std::rc::Rc;

use rils_execution::{
    Type, Value,
    runtime_builtins::{NativeCallError, call_native_symbol_with_callback},
};

#[derive(Debug, PartialEq, Eq)]
struct CallbackFailure;

#[test]
fn callback_and_bridge_errors_keep_distinct_types() {
    let symbol = "core::option::option::map";
    let present = Value::Option {
        value: Some(Rc::new(Value::from_i32(1))),
        element_type: Some(Type::I32),
    };
    let failure =
        call_native_symbol_with_callback(symbol, &[present, Value::from_i32(0)], &mut |_, _| {
            Err::<Value, _>(CallbackFailure)
        })
        .unwrap()
        .unwrap_err();
    assert!(matches!(
        failure,
        NativeCallError::Callback(CallbackFailure)
    ));

    let failure = call_native_symbol_with_callback(
        symbol,
        &[Value::from_i32(1), Value::from_i32(0)],
        &mut |_, _| Ok::<_, CallbackFailure>(Value::from_i32(2)),
    )
    .unwrap()
    .unwrap_err();
    assert!(matches!(failure, NativeCallError::Bridge(_)));
}

#[test]
fn filter_passes_a_shared_reference_and_skips_absent_values() {
    let symbol = "core::option::option::filter";
    let present = Value::Option {
        value: Some(Rc::new(Value::from_i32(4))),
        element_type: Some(Type::I32),
    };
    let mut calls = 0;
    let kept = call_native_symbol_with_callback(
        symbol,
        &[present, Value::from_i32(0)],
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
    assert!(matches!(kept, Value::Option { value: Some(_), .. }));
    assert_eq!(calls, 1);

    let absent = Value::Option {
        value: None,
        element_type: Some(Type::I32),
    };
    let skipped = call_native_symbol_with_callback(
        symbol,
        &[absent, Value::from_i32(0)],
        &mut |_, _| -> Result<Value, CallbackFailure> {
            panic!("absent values must not invoke the predicate")
        },
    )
    .unwrap()
    .unwrap();
    assert!(matches!(skipped, Value::Option { value: None, .. }));

    let present = Value::Option {
        value: Some(Rc::new(Value::from_i32(4))),
        element_type: Some(Type::I32),
    };
    let failure =
        call_native_symbol_with_callback(symbol, &[present, Value::from_i32(0)], &mut |_, _| {
            Ok::<_, CallbackFailure>(Value::from_i32(1))
        })
        .unwrap()
        .unwrap_err();
    assert!(matches!(failure, NativeCallError::Bridge(message) if message.contains("bool")));
}

#[test]
fn free_function_callback_symbols_are_registered() {
    let result = call_native_symbol_with_callback(
        "core::ops::apply_twice",
        &[Value::from_i32(1), Value::from_i32(0)],
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
    let callback_error = call_native_symbol_with_callback(symbol, &arguments, &mut |_, _| {
        Err::<Value, _>(CallbackFailure)
    })
    .unwrap()
    .unwrap_err();
    assert!(matches!(
        callback_error,
        NativeCallError::Callback(CallbackFailure)
    ));

    let arity_error = call_native_symbol_with_callback(symbol, &arguments[..2], &mut |_, _| {
        Ok::<_, CallbackFailure>(Value::from_i32(1))
    })
    .unwrap()
    .unwrap_err();
    assert!(
        matches!(arity_error, NativeCallError::Bridge(message) if message.contains("expects 3 arguments"))
    );
}
