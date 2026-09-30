use rils::{BytecodeModule, Value, compile, eval_value};

#[test]
fn concrete_collection_defaults_have_native_storage() {
    for (ty, source) in [
        ("Vec<i32>", "<Vec<i32> as Default>::default()"),
        ("HashSet<string>", "<HashSet<string> as Default>::default()"),
        (
            "HashMap<i32, Option<string>>",
            "<HashMap<i32, Option<string>> as Default>::default()",
        ),
    ] {
        let module = compile(source).expect("compile default expression");
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for (backend, value) in [
            (
                "interpreter",
                eval_value(source).expect("interpret default expression"),
            ),
            (
                "VM",
                module.execute_value().expect("execute default expression"),
            ),
            (
                "loaded VM",
                loaded
                    .execute_value()
                    .expect("execute loaded default expression"),
            ),
        ] {
            let Value::Dynamic(object) = value else {
                panic!("{backend}: {source} should construct native storage, found {value:?}")
            };
            assert_eq!(object.descriptor().layout().rils_type().to_string(), ty);
        }
    }
}

#[test]
fn one_module_can_instantiate_several_native_collection_defaults() {
    let source = r#"
        struct Item { value: i32 }
        let numbers: Vec<i32> = <Vec<i32> as Default>::default();
        let names: Vec<string> = <Vec<string> as Default>::default();
        let items: HashMap<i32, Item> = <HashMap<i32, Item> as Default>::default();
        numbers.is_empty() && names.is_empty() && items.is_empty()
    "#;
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value, Value::Bool(true));
    }
}
