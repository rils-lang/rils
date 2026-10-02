use rils::{BytecodeModule, Value, compile, eval, eval_value};
use rils_execution::value::EnumPayload;

#[test]
fn declared_nominal_variants_use_composed_storage_in_both_backends() {
    for (source, expected_type) in [
        (
            "struct Item { value: i32 } let wrapped: Option<Item> = Some(Item { value: 7 }); wrapped",
            "Option<Item>",
        ),
        (
            "struct Item { value: i32 } let outcome: Result<Item, string> = Ok(Item { value: 7 }); outcome",
            "Result<Item, string>",
        ),
        (
            "enum Choice { Empty, Item(i32) } let outcome: Result<Choice, string> = Ok(Choice::Item(7)); outcome",
            "Result<Choice, string>",
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for (stage, value) in [
            ("interpreter", eval_value(source).unwrap()),
            ("VM", compiled.execute_value().unwrap()),
            ("loaded VM", loaded.execute_value().unwrap()),
        ] {
            let Value::Dynamic(object) = value else {
                panic!("{expected_type} should use native storage in {stage}");
            };
            assert_eq!(
                object.descriptor().layout().rils_type().to_string(),
                expected_type,
                "{stage}"
            );
        }
    }
}

#[test]
fn nominal_option_payload_moves_out_without_a_legacy_snapshot() {
    let source = "struct Item { value: i32 } let wrapped: Option<Item> = Some(Item { value: 7 }); let item = wrapped.unwrap(); item.value";
    assert_eq!(eval_value(source).unwrap().as_i32(), Some(7));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap().as_i32(),
        Some(7)
    );
}

#[test]
fn nominal_result_payload_moves_out() {
    for (source, expected) in [
        (
            "struct Item { value: i32 } let outcome: Result<Item, string> = Ok(Item { value: 9 }); let item = outcome.unwrap(); item.value",
            9,
        ),
        (
            "struct Item { value: i32 } let outcome: Result<string, Item> = Err(Item { value: 11 }); let item = outcome.unwrap_err(); item.value",
            11,
        ),
    ] {
        assert_eq!(eval_value(source).unwrap().as_i32(), Some(expected));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap().as_i32(),
            Some(expected)
        );
    }
}

#[test]
fn host_borrows_nominal_option_layout_without_materializing_fields() {
    let source =
        "struct Item { value: i32 } let wrapped: Option<Item> = Some(Item { value: 7 }); wrapped";
    for value in [
        eval(source).unwrap(),
        compile(source).unwrap().execute().unwrap(),
    ] {
        value
            .with_native_view(|view| {
                assert!(view.option_is_some().unwrap());
                assert_eq!(
                    view.option_item()
                        .unwrap()
                        .field(0)
                        .unwrap()
                        .layout()
                        .unwrap()
                        .rils_type()
                        .to_string(),
                    "i32"
                );
            })
            .unwrap();
    }
}

#[test]
fn nominal_option_tag_methods_borrow_without_cloning_payload() {
    let source = "struct Item { value: i32 } let wrapped: Option<Item> = Some(Item { value: 7 }); wrapped.is_some()";
    assert!(matches!(eval_value(source).unwrap(), Value::Bool(true)));
    assert!(matches!(
        compile(source).unwrap().execute_value().unwrap(),
        Value::Bool(true)
    ));
}

#[test]
fn nominal_option_in_declared_record_field_keeps_its_layout() {
    let field_source = "struct Item { value: i32 } struct Holder { payload: Option<Item> } let holder = Holder { payload: Some(Item { value: 13 }) }; holder.payload";
    for (stage, value) in [
        ("interpreter", eval_value(field_source).unwrap()),
        (
            "VM",
            compile(field_source).unwrap().execute_value().unwrap(),
        ),
    ] {
        assert!(matches!(value, Value::Dynamic(_)), "{stage}");
    }
    let source = "struct Item { value: i32 } struct Holder { payload: Option<Item> } let holder = Holder { payload: Some(Item { value: 13 }) }; let item = holder.payload.unwrap(); item.value";
    assert_eq!(eval_value(source).unwrap().as_i32(), Some(13));
    assert_eq!(
        compile(source).unwrap().execute_value().unwrap().as_i32(),
        Some(13)
    );
}

#[test]
fn nominal_option_in_enum_record_field_moves_out() {
    for source in [
        "struct Item { value: i32 } enum Holder { Wrapped { payload: Option<Item> } } let holder = Holder::Wrapped { payload: Some(Item { value: 17 }) }; match holder { Holder::Wrapped { payload } => { let item = payload.unwrap(); item.value } }",
        "struct Item { value: i32 } enum Holder<T> { Wrapped { payload: Option<T> } } let holder: Holder<Item> = Holder::Wrapped { payload: Some(Item { value: 17 }) }; match holder { Holder::Wrapped { payload } => { let item = payload.unwrap(); item.value } }",
    ] {
        assert_eq!(eval_value(source).unwrap().as_i32(), Some(17));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap().as_i32(),
            Some(17)
        );
    }
}

#[test]
fn nominal_sum_in_enum_tuple_field_uses_concrete_storage() {
    for (source, expected_type) in [
        (
            "struct Item { value: i32 } enum Holder<T> { Wrapped(Option<T>) } Holder::Wrapped(Some(Item { value: 19 }))",
            "Item",
        ),
        (
            "struct Item { value: i32 } enum Holder { Wrapped(Result<Item, string>) } Holder::Wrapped(Ok(Item { value: 19 }))",
            "",
        ),
        (
            "struct Item { value: i32 } enum Holder<T> { Wrapped(Result<T, string>) } Holder::Wrapped(Ok(Item { value: 19 }))",
            "Item",
        ),
    ] {
        for (stage, value) in [
            ("interpreter", eval_value(source).unwrap()),
            ("VM", compile(source).unwrap().execute_value().unwrap()),
        ] {
            let Value::Enum(instance) = value else {
                panic!("expected enum value in {stage}");
            };
            if !expected_type.is_empty() {
                assert_eq!(
                    instance.type_arguments[0].to_string(),
                    expected_type,
                    "{stage}"
                );
            }
            let EnumPayload::Tuple(fields) = &instance.payload else {
                panic!("expected tuple payload in {stage}");
            };
            assert!(matches!(fields[0], Value::Dynamic(_)), "{stage}");
        }
    }
}

#[test]
fn nominal_sum_moves_out_of_enum_tuple_pattern() {
    for source in [
        "struct Item { value: i32 } enum Holder<T> { Wrapped(Option<T>) } let holder = Holder::Wrapped(Some(Item { value: 19 })); match holder { Holder::Wrapped(payload) => { let item = payload.unwrap(); item.value } }",
        "struct Item { value: i32 } enum Holder { Wrapped(Result<Item, string>) } let holder = Holder::Wrapped(Ok(Item { value: 19 })); match holder { Holder::Wrapped(payload) => { let item = payload.unwrap(); item.value } }",
        "struct Item { value: i32 } enum Holder<T> { Wrapped(Result<T, string>) } let holder = Holder::Wrapped(Ok(Item { value: 19 })); match holder { Holder::Wrapped(payload) => { let item = payload.unwrap(); item.value } }",
    ] {
        assert_eq!(eval_value(source).unwrap().as_i32(), Some(19));
        assert_eq!(
            compile(source).unwrap().execute_value().unwrap().as_i32(),
            Some(19)
        );
    }
}

#[test]
fn concrete_function_parameters_compose_nominal_sums() {
    for source in [
        "struct Item { value: i32 } fn pass(value: Option<Item>) -> Option<Item> { value } pass(Some(Item { value: 23 }))",
        "struct Item { value: i32 } fn pass(value: Result<Item, string>) -> Result<Item, string> { value } pass(Ok(Item { value: 23 }))",
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for (stage, value) in [
            ("interpreter", eval_value(source).unwrap()),
            ("VM", compiled.execute_value().unwrap()),
            ("loaded VM", loaded.execute_value().unwrap()),
        ] {
            assert!(matches!(value, Value::Dynamic(_)), "{stage}");
        }
    }
}

#[test]
fn generic_function_parameters_resolve_nominal_sum_layouts() {
    for source in [
        "struct Item { value: i32 } fn pass<T>(value: Option<T>) -> Option<T> { value } pass(Some(Item { value: 29 }))",
        "struct Item { value: i32 } fn pass<T>(value: Result<T, string>) -> Result<T, string> { value } pass(Ok(Item { value: 29 }))",
        "struct Item { value: i32 } fn pass<T>(value: Result<Option<T>, string>) -> Result<Option<T>, string> { value } pass(Ok(Some(Item { value: 29 })))",
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for (stage, value) in [
            ("interpreter", eval_value(source).unwrap()),
            ("VM", compiled.execute_value().unwrap()),
            ("loaded VM", loaded.execute_value().unwrap()),
        ] {
            assert!(matches!(value, Value::Dynamic(_)), "{stage}: {source}");
        }
    }
}

#[test]
fn nominal_sum_formatting_matches_legacy_value() {
    for (definition, legacy_source, typed_source) in [
        (
            "struct Item { value: i32 }",
            "Some(Item { value: 7 })",
            "let wrapped: Option<Item> = Some(Item { value: 7 }); wrapped",
        ),
        (
            "struct Item { value: i32 }",
            "Ok(Item { value: 7 })",
            "let outcome: Result<Item, string> = Ok(Item { value: 7 }); outcome",
        ),
        (
            "enum Choice { Empty, Item(i32) }",
            "Ok(Choice::Item(7))",
            "let outcome: Result<Choice, string> = Ok(Choice::Item(7)); outcome",
        ),
    ] {
        let legacy = eval_value(&format!("{definition} {legacy_source}")).unwrap();
        let typed = eval_value(&format!("{definition} {typed_source}")).unwrap();
        assert_eq!(typed.to_string(), legacy.to_string());
        assert_eq!(format!("{typed:?}"), format!("{legacy:?}"));
    }
}
