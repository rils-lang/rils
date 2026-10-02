use super::*;

#[test]
fn signature_metadata_round_trips_and_is_verified() {
    let source = "fn pass(value: i32) -> Option<i32> { Some(value) } pass(1)";
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    let index = module
        .functions
        .iter()
        .position(|function| function.name == "pass")
        .unwrap();
    assert_eq!(
        loaded.functions[index].parameter_types,
        vec![Some(Type::Integer(IntegerType::I32))]
    );
    assert_eq!(
        loaded.functions[index].return_type,
        Some(Type::Option(Box::new(Type::Integer(IntegerType::I32))))
    );

    let mut invalid = loaded.clone();
    invalid.functions[index].parameter_types.clear();
    assert!(invalid.verify().is_err());

    let mut invalid = loaded;
    invalid.functions[index].return_type = Some(Type::BoundVariable {
        name: "T".into(),
        bounds: vec![Type::Slice(Box::new(Type::IntegerVariable(Span {
            source: SourceId::new(u32::MAX - 1),
            start: 0,
            end: 0,
        })))],
    });
    assert!(invalid.verify().is_err());
    assert!(invalid.to_bytes().is_err());
}
