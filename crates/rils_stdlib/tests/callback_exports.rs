use rils_stdlib::stdlib::{ops, option::Option as RilsOption, result::Result as RilsResult};

#[test]
fn exported_callbacks_return_plain_rust_values() {
    let mapped = RilsOption::Some(3).map(|value| value + 1);
    assert!(matches!(mapped, RilsOption::Some(4)));

    let mapped = RilsResult::<i32, &str>::Ok(3).map(|value| value + 1);
    assert!(matches!(mapped, RilsResult::Ok(4)));

    assert_eq!(ops::apply_twice(2, |value| value + 1), 4);
    assert_eq!(ops::chain(|value| value + 1, 2, |value| value * 2), 6);
}

#[test]
fn generated_fallible_implementation_propagates_callback_errors() {
    let result = RilsOption::Some(3).__rils_try_map(|_| Err::<i32, _>("callback failed"));
    assert!(matches!(result, Err("callback failed")));

    let result = ops::__rils_try_apply_twice(1, |value| {
        if value == 2 {
            Err("second invocation failed")
        } else {
            Ok(value + 1)
        }
    });
    assert!(matches!(result, Err("second invocation failed")));
}
