use rils_bytecode::{BytecodeModule, compile};

#[test]
fn imported_declarations_and_callable_values_cross_all_backends() {
    let source = include_str!("fixtures/declaration_bindings.rils");
    assert_eq!(
        rils_runtime::eval_value(source)
            .unwrap()
            .get_cloned::<i32>()
            .ok(),
        Some(42)
    );
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for module in [module, loaded] {
        assert_eq!(
            module.execute_value().unwrap().get_cloned::<i32>().ok(),
            Some(42)
        );
    }
}
