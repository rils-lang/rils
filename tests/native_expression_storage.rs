use rils::{BytecodeModule, Type, Value, compile, eval_value};

fn native_sum_types(value: &Value) -> Vec<String> {
    match value {
        Value::Dynamic(object) => {
            let ty = object.descriptor().layout().rils_type();
            assert!(matches!(ty, Type::Option(_) | Type::Result(_, _)));
            vec![ty.to_string()]
        }
        Value::Tuple(sequence) | Value::Array(sequence) => sequence
            .elements
            .borrow()
            .iter()
            .flat_map(|slot| native_sum_types(slot.value.as_ref().unwrap()))
            .collect(),
        other => panic!("expression retained legacy sum storage: {other:?}"),
    }
}

#[test]
fn owned_expression_results_use_inferred_native_declarations() {
    for (source, expected) in [
        (
            include_str!("fixtures/native_expression_storage/option.rils"),
            vec!["Option<Item>"],
        ),
        (
            include_str!("fixtures/native_expression_storage/result.rils"),
            vec!["Result<Item, i32>"],
        ),
        (
            include_str!("fixtures/native_expression_storage/array.rils"),
            vec!["Result<Item, i32>"; 2],
        ),
        (
            include_str!("fixtures/native_expression_storage/nested.rils"),
            vec!["Option<Item>", "Result<Item, i32>", "Result<Item, i32>"],
        ),
        (
            include_str!("fixtures/native_expression_storage/none.rils"),
            vec!["Option<Item>"],
        ),
        (
            include_str!("fixtures/native_expression_storage/match.rils"),
            vec!["Result<Item, i32>"; 2],
        ),
        (
            include_str!("fixtures/native_expression_storage/modules.rils"),
            vec!["Option<model::Item>"; 3],
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        let values = [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ];
        for value in &values {
            assert_eq!(native_sum_types(value), expected, "{source}: {value}");
        }
        assert_eq!(values[0].to_string(), values[1].to_string());
        assert_eq!(values[1].to_string(), values[2].to_string());
    }
}

#[test]
fn temporary_sum_consumers_move_non_clone_payloads() {
    let source = include_str!("fixtures/native_expression_storage/consumers.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value.to_string(), "(37, true)");
    }
}

#[test]
fn empty_array_results_keep_the_declared_element_type() {
    let source = include_str!("fixtures/native_expression_storage/empty.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    let expected = Type::Array {
        element: Box::new(Type::Option(Box::new(Type::named("Item")))),
        length: 0,
    };
    for value in [
        eval_value(source).unwrap(),
        module.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(Type::of_value(&value), Some(expected.clone()));
    }
}
