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

#[test]
fn generic_structs_can_carry_local_references() {
    let value = eval(
        r#"
        struct Wrapper<T> { value: T }
        fn run() -> i32 {
            let source = 7;
            let wrapped: Wrapper<&i32> = Wrapper { value: &source };
            *wrapped.value
        }
        run()
        "#,
    )
    .expect("generic struct instances may carry local references");
    assert_eq!(value, Value::I32(7));
}

#[test]
fn hash_maps_can_carry_reference_values_locally() {
    let value = eval(
        r#"
        fn run() -> i32 {
            let mut values: HashMap<string, &i32> = HashMap::new();
            let key = "answer";
            let source = 42;
            values.insert(key.clone(), &source);
            let found = values.get_cloned(&key);
            *found.unwrap()
        }
        run()
        "#,
    )
    .expect("HashMap values may carry local references");
    assert_eq!(value, Value::I32(42));
}
