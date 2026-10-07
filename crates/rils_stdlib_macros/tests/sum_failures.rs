#[path = "fixtures/sum_failures.rs"]
mod fixture;

#[test]
fn renamed_callbacks_keep_their_declared_body_and_error_type() {
    use fixture::Option;
    assert!(matches!(
        Option::Some("owned".to_owned()).transform(|value| value.len()),
        Option::Some(5)
    ));
    assert!(matches!(
        Option::Some("owned".to_owned()).__rils_try_transform(|value| Ok::<_, usize>(value.len())),
        Ok(Option::Some(5))
    ));
    assert!(matches!(
        Option::<String>::None.__rils_try_transform(|_| Err::<usize, _>(42)),
        Ok(Option::None)
    ));
    assert!(matches!(
        Option::Some("owned".to_owned()).__rils_try_transform(|_| Err::<usize, _>(42)),
        Err(42)
    ));
}

#[test]
fn renamed_sum_methods_generate_fallible_bodies_for_panics_and_explicit_returns() {
    use fixture::Option;

    assert_eq!(Option::Some(42).require("ignored".into()), 42);
    assert_eq!(
        Option::Some(42).__rils_try_require("ignored".into()),
        Ok(42)
    );
    assert_eq!(
        Option::<i32>::None.__rils_try_require("item".into()),
        Err("missing: item".into())
    );
    assert_eq!(Option::Some(42).require_return(), 42);
    assert_eq!(Option::Some(42).__rils_try_require_return(), Ok(42));
    assert_eq!(
        Option::<i32>::None.__rils_try_require_return(),
        Err("missing".into())
    );
    assert_eq!(Option::Some(42).require_empty(), 42);
    assert_eq!(
        Option::<i32>::None.__rils_try_require_empty(),
        Err("explicit panic".into())
    );
}
