use rils_runtime::{Engine, ExecutionLimits, Value, eval};

#[test]
fn evaluates_owned_values_and_explicit_clones() {
    let value = eval(
        r#"
        let original = "rils";
        let copied = clone(&original);
        original == copied
        "#,
    )
    .expect("valid ownership flow should execute");

    assert_eq!(value, Value::Bool(true));
}

#[test]
fn enforces_configured_execution_limits() {
    let mut engine = Engine::new();
    engine.set_execution_limits(ExecutionLimits::new(1_000, 8));

    let error = engine
        .eval(
            r#"
            fn recurse() {
                recurse()
            }
            recurse()
            "#,
        )
        .expect_err("unbounded recursion must exhaust the call-depth budget");

    let message = error.to_string();
    assert!(
        message.contains("frame limit"),
        "unexpected error: {message}"
    );
}

#[test]
fn supports_local_reference_containers_and_input_reference_returns() {
    let value = eval(
        r#"
        fn identity(value: &i32) -> Option<&i32> {
            Some(value)
        }
        fn run() -> i32 {
            let source = 41;
            let wrapped = identity(&source);
            let pair = (wrapped, [Some(&source)]);
            *pair.0.unwrap()
        }
        run()
        "#,
    )
    .expect("references may be carried by local generic containers");
    assert_eq!(value, Value::I32(41));
}

#[test]
fn rejects_local_reference_return_escape() {
    let error = eval(
        r#"
        fn invalid() -> &i32 {
            let value = 1;
            &value
        }
        invalid()
        "#,
    )
    .expect_err("a local reference must not escape its function");
    assert!(error.to_string().contains("cannot be returned"));
}
