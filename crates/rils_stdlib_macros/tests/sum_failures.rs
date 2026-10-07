#[path = "fixtures/sum_failures.rs"]
mod fixture;

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
