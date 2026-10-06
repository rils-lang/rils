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
fn separate_files_preserve_field_declarations_and_aliases() {
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native_assignment_storage/files/main.rils");
    let compiled = rils::compile_file(&entry).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        rils::Engine::new().eval_file_value(&entry).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(value.as_i32(), Some(37));
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
    for source in [
        include_str!("fixtures/native_assignment_storage/modules.rils"),
        include_str!("fixtures/native_assignment_storage/module_field.rils"),
        include_str!("fixtures/native_assignment_storage/module_none.rils"),
        include_str!("fixtures/native_assignment_storage/alias_return.rils"),
    ] {
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
}

#[test]
fn moved_fields_reuse_native_declarations_without_cloning_payloads() {
    for (source, expected) in [
        (
            include_str!("fixtures/native_assignment_storage/moved_field_none.rils"),
            "None",
        ),
        (
            include_str!("fixtures/native_assignment_storage/moved_field_result.rils"),
            "Err(37)",
        ),
        (
            include_str!("fixtures/native_assignment_storage/moved_nested_field.rils"),
            "Some(Item { value: 37 })",
        ),
        (
            include_str!("fixtures/native_assignment_storage/moved_tuple_field.rils"),
            "Some(Item { value: 37 })",
        ),
        (
            include_str!("fixtures/native_assignment_storage/moved_field_reference.rils"),
            "Some(Item { value: 37 })",
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert!(
                matches!(&value, Value::Dynamic(_)),
                "{source}: expected native sum, found {value:?}"
            );
            assert_native_sum(&value);
            assert_eq!(value.to_string(), expected, "{source}");
        }
    }
}

#[test]
fn typed_indexed_initializers_recursively_compose_nominal_sums() {
    for source in [
        include_str!("fixtures/native_assignment_storage/typed_tuple.rils"),
        include_str!("fixtures/native_assignment_storage/typed_array.rils"),
        include_str!("fixtures/native_assignment_storage/typed_nested.rils"),
        include_str!("fixtures/native_assignment_storage/typed_parameter_return.rils"),
        include_str!("fixtures/native_assignment_storage/typed_alias.rils"),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_native_sum(&value);
            assert!(value.to_string().contains("37"), "{source}: {value}");
        }
    }
}

#[test]
fn inferred_bindings_compose_native_storage_without_annotations() {
    for source in [
        include_str!("fixtures/native_assignment_storage/inferred_option.rils"),
        include_str!("fixtures/native_assignment_storage/inferred_indexed.rils"),
        include_str!("fixtures/native_assignment_storage/inferred_result.rils"),
        include_str!("fixtures/native_assignment_storage/inferred_modules.rils"),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_native_sum(&value);
            assert!(value.to_string().contains("37"), "{source}: {value}");
        }
    }
}

fn native_leaf_types(value: &Value) -> Vec<String> {
    match value {
        Value::Dynamic(object) => vec![object.descriptor().layout().rils_type().to_string()],
        Value::Tuple(sequence) => sequence
            .elements
            .borrow()
            .iter()
            .flat_map(|slot| native_leaf_types(slot.value.as_ref().unwrap()))
            .collect(),
        other => panic!("expected native nominal sums, found {other:?}"),
    }
}

#[test]
fn module_declarations_retain_identity_and_private_layouts_without_imports() {
    for (source, expected) in [
        (
            include_str!("fixtures/native_assignment_storage/module_identity.rils"),
            vec!["Option<left::Item>", "Option<right::Item>"],
        ),
        (
            include_str!("fixtures/native_assignment_storage/module_enum_identity.rils"),
            vec!["Option<left::Item>", "Option<right::Item>"],
        ),
        (
            include_str!("fixtures/native_assignment_storage/module_private_layout.rils"),
            vec!["Option<model::Item>"],
        ),
        (
            include_str!("fixtures/native_assignment_storage/module_forward_layout.rils"),
            vec!["Option<model::Item>"],
        ),
        (
            include_str!("fixtures/native_assignment_storage/module_trait_identity.rils"),
            vec!["Option<model::Item>"],
        ),
    ] {
        let compiled = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
        for value in [
            eval_value(source).unwrap(),
            compiled.execute_value().unwrap(),
            loaded.execute_value().unwrap(),
        ] {
            assert_eq!(native_leaf_types(&value), expected, "{source}: {value}");
            assert!(value.to_string().contains("37"), "{source}: {value}");
        }
    }
}

#[test]
fn separate_files_use_canonical_module_layouts_without_importing_types() {
    let entry = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/native_assignment_storage/identity_files/main.rils");
    let compiled = rils::compile_file(&entry).unwrap();
    let loaded = BytecodeModule::from_bytes(&compiled.to_bytes().unwrap()).unwrap();
    for value in [
        rils::Engine::new().eval_file_value(&entry).unwrap(),
        compiled.execute_value().unwrap(),
        loaded.execute_value().unwrap(),
    ] {
        assert_eq!(
            native_leaf_types(&value),
            ["Option<left::Item>", "Option<right::Item>"]
        );
        assert!(value.to_string().contains("37"));
    }
}
