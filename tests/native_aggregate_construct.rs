use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn native_aggregate_literals_and_defaults_preserve_ownership_and_inference() {
    let source = include_str!("fixtures/native_aggregate_construct.rils");
    assert_eq!(
        eval_value(source).unwrap(),
        Value::from(rils::value::native_bool(true))
    );
    let module = compile(source).unwrap();
    assert_eq!(
        module.execute_value().unwrap(),
        Value::from(rils::value::native_bool(true))
    );
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        loaded.execute_value().unwrap(),
        Value::from(rils::value::native_bool(true))
    );
}

#[test]
fn literal_results_are_native_and_do_not_implicitly_grant_copy() {
    let source = include_str!("fixtures/native_aggregate_output.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert!(!value.is_copy());
        let Value::Dynamic(object) = value else {
            panic!("expected native tuple");
        };
        object
            .with(|payload| {
                assert_eq!(
                    payload.view().layout().unwrap().rils_type().to_string(),
                    "(Item, [Item; 0])"
                );
                assert!(!payload.view().field(0).unwrap().layout().unwrap().is_copy());
                assert!(!payload.view().field(1).unwrap().layout().unwrap().is_copy());
            })
            .unwrap();
    }
}

#[test]
fn unresolved_empty_array_type_is_rejected_before_native_allocation() {
    let source = include_str!("fixtures/native_aggregate_unknown.rils");
    assert!(
        eval_value(source)
            .unwrap_err()
            .to_string()
            .contains("infer")
    );
    let error = compile(source).err().unwrap();
    assert!(error.message.contains("infer"), "{error}");
    assert!(error.span.end > error.span.start);
}
