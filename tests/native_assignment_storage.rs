use rils::{BytecodeModule, Value, compile, eval_value};

fn assert_native_sum(value: &Value) {
    match value {
        Value::Dynamic(object) => {
            assert!(matches!(
                object.descriptor().layout().rils_type(),
                rils::Type::Option(_) | rils::Type::Result(_, _)
            ));
        }
        Value::Tuple(sequence) | Value::Array(sequence) => {
            for slot in sequence.elements.borrow().iter() {
                assert_native_sum(slot.value.as_ref().unwrap());
            }
        }
        value => panic!("expected native sum, found {value:?}"),
    }
}

#[test]
fn assignments_preserve_native_sum_layouts_and_move_nominal_payloads() {
    for source in [
        include_str!("fixtures/native_assignment_storage/local.rils"),
        include_str!("fixtures/native_assignment_storage/none.rils"),
        include_str!("fixtures/native_assignment_storage/error.rils"),
        include_str!("fixtures/native_assignment_storage/reinitialize.rils"),
        include_str!("fixtures/native_assignment_storage/field.rils"),
        include_str!("fixtures/native_assignment_storage/reference.rils"),
        include_str!("fixtures/native_assignment_storage/field_reference.rils"),
        include_str!("fixtures/native_assignment_storage/tuple.rils"),
        include_str!("fixtures/native_assignment_storage/array.rils"),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_native_sum(&value);
            if source == include_str!("fixtures/native_assignment_storage/none.rils") {
                assert_eq!(value.to_string(), "None");
            } else {
                assert!(value.to_string().contains("37"), "{source}: {value}");
            }
        }
    }
}

#[test]
fn replacement_preserves_the_producers_nominal_declaration() {
    let source = include_str!("fixtures/native_assignment_storage/modules.rils");
    let compiled = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        eval_value(source).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value.as_i32(), Some(37));
    }
}
