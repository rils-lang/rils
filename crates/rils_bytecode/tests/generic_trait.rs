use rils_bytecode::compile;
use rils_runtime::eval;

#[test]
fn callable_bounds_report_an_explicit_compile_error_until_verified() {
    let source = include_str!("fixtures/function_trait_bound.rils");
    assert!(eval(source).is_ok());
    let error = match compile(source) {
        Ok(_) => panic!("bytecode does not yet verify callable bounds"),
        Err(error) => error,
    };
    assert!(error.message.contains("callable trait bounds"));
    assert_ne!(error.span.start, error.span.end);
}

#[test]
fn bytecode_rejects_manual_callable_trait_implementations() {
    let source = include_str!("fixtures/sealed_callable_impl.rils");
    let error = match compile(source) {
        Ok(_) => panic!("a user type must not implement a callable trait"),
        Err(error) => error,
    };
    assert!(error.message.contains("sealed"), "{error}");
}
