use rils_bytecode::{BytecodeModule, compile};

#[test]
fn native_collections_retain_nominal_types_through_moves_and_iteration() {
    let source = include_str!("fixtures/native_collection_cutover.rils");
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

#[test]
fn native_only_scalars_and_sums_cross_every_execution_backend() {
    let source = include_str!("fixtures/native_storage_cutover.rils");
    let interpreted = rils_runtime::eval_value(source).unwrap();
    assert_eq!(interpreted.get_cloned::<i32>().ok(), Some(42));
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for module in [module, loaded] {
        let value = module.execute_value().unwrap();
        assert_eq!(value.get_cloned::<i32>().ok(), Some(42));
    }
}

#[test]
fn native_indexed_values_and_contextual_constructors_cross_every_backend() {
    let source = include_str!("fixtures/native_indexed_cutover.rils");
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
