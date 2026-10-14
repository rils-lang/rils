use rils_bytecode::{BytecodeModule, compile};

#[test]
fn declaration_generated_numbers_match_all_backends() {
    let source = include_str!("fixtures/numeric_dispatch.rils");
    assert_eq!(
        rils_runtime::eval_value(source)
            .unwrap()
            .get_cloned::<i32>()
            .unwrap(),
        42
    );
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for module in [module, loaded] {
        assert_eq!(
            module.execute_value().unwrap().get_cloned::<i32>().unwrap(),
            42
        );
    }
}

#[test]
fn generated_numeric_failures_match_all_backends() {
    for (source, message) in [
        (
            include_str!("fixtures/numeric_overflow.rils"),
            "integer overflow",
        ),
        (
            include_str!("fixtures/numeric_negate_overflow.rils"),
            "integer overflow",
        ),
        (
            include_str!("fixtures/numeric_float_zero.rils"),
            "division by zero",
        ),
    ] {
        assert!(
            rils_runtime::eval_value(source)
                .unwrap_err()
                .to_string()
                .contains(message)
        );
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for module in [module, loaded] {
            assert!(
                module
                    .execute_value()
                    .unwrap_err()
                    .to_string()
                    .contains(message)
            );
        }
    }
}
