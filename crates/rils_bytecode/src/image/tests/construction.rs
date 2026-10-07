use super::*;

#[test]
fn option_constructors_reject_missing_layouts_and_nested_inference_types() {
    for (present, source) in [
        (
            false,
            include_str!("../../../tests/fixtures/native_option_constructors/none.rils"),
        ),
        (
            true,
            include_str!("../../../tests/fixtures/native_option_constructors/owned.rils"),
        ),
    ] {
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        for case in 0..6 {
            let mut invalid = loaded.clone();
            let instruction = invalid
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.instructions)
                .find(|instruction| {
                    matches!(
                        (&instruction.instruction, present),
                        (Instruction::BuildOptionNone { .. }, false)
                            | (Instruction::BuildOptionSome { .. }, true)
                    )
                })
                .unwrap();
            let (destination, item_type) = match &mut instruction.instruction {
                Instruction::BuildOptionNone {
                    destination,
                    item_type,
                } => (destination, item_type),
                Instruction::BuildOptionSome {
                    destination,
                    source,
                    item_type,
                } => {
                    if case == 5 {
                        *source = usize::MAX;
                    }
                    (destination, item_type)
                }
                _ => unreachable!(),
            };
            match case {
                0 => *item_type = Type::Unknown,
                1 => *item_type = Type::Option(Box::new(Type::Unknown)),
                2 => *item_type = Type::Result(Box::new(Type::I32), Box::new(Type::Unknown)),
                3 => *item_type = Type::Tuple(vec![Type::I32, Type::Unknown]),
                _ if case != 5 || !present => *destination = usize::MAX,
                _ => {}
            }
            assert!(invalid.verify().is_err(), "Some={present}, case={case}");
            assert!(invalid.to_bytes().is_err(), "Some={present}, case={case}");
            assert!(
                invalid.execute_value().is_err(),
                "Some={present}, case={case}"
            );
        }
        let mut missing_layout = loaded;
        let instruction = missing_layout
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.instructions)
            .find(|instruction| {
                matches!(
                    (&instruction.instruction, present),
                    (Instruction::BuildOptionNone { .. }, false)
                        | (Instruction::BuildOptionSome { .. }, true)
                )
            })
            .unwrap();
        match &mut instruction.instruction {
            Instruction::BuildOptionNone { item_type, .. }
            | Instruction::BuildOptionSome { item_type, .. } => {
                *item_type = Type::named("Unregistered")
            }
            _ => unreachable!(),
        }
        missing_layout.verify().unwrap();
        let error = missing_layout.execute_value().unwrap_err();
        assert!(error.message.contains("Unregistered"), "{error}");
    }
}

#[test]
fn verifier_rejects_invalid_constructor_types_fields_and_variants() {
    let source = include_str!("../../../tests/fixtures/native_records/verifier.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(loaded.execute_value().unwrap().as_i32(), Some(42));
    for case in 0..12 {
        let mut invalid = loaded.clone();
        let instruction = invalid
            .functions
            .iter_mut()
            .flat_map(|function| &mut function.instructions)
            .find(|instruction| match &instruction.instruction {
                Instruction::ConstructRecord { variant, .. } => {
                    (case < 6 && variant.is_none()) || (case == 11 && variant.is_some())
                }
                Instruction::ConstructTupleVariant { .. } => (6..9).contains(&case),
                Instruction::ConstructUnitVariant { .. } => (9..11).contains(&case),
                _ => false,
            })
            .unwrap();
        match &mut instruction.instruction {
            Instruction::ConstructRecord {
                type_id,
                expected,
                variant,
                fields,
                ..
            } => match case {
                0 => *type_id = usize::MAX,
                1 => *expected = Type::named("Other"),
                2 => {
                    *expected = Type::Named {
                        name: "Holder".into(),
                        arguments: vec![Type::Unknown],
                    }
                }
                3 => fields.clear(),
                4 => fields.push(fields[0].clone()),
                5 => fields[0].0 = "unknown".into(),
                _ => *variant = Some("Tuple".into()),
            },
            Instruction::ConstructTupleVariant {
                expected,
                variant,
                fields,
                ..
            } => match case {
                6 => *expected = Type::named("Choice"),
                7 => *variant = "Empty".into(),
                _ => fields.clear(),
            },
            Instruction::ConstructUnitVariant {
                expected, variant, ..
            } => match case {
                9 => {
                    *expected = Type::Named {
                        name: "Choice".into(),
                        arguments: vec![Type::Option(Box::new(Type::Unknown))],
                    }
                }
                _ => *variant = "Record".into(),
            },
            _ => unreachable!(),
        }
        let error = invalid.verify().unwrap_err();
        assert!(error.message.contains("construct"), "case {case}: {error}");
        assert!(invalid.to_bytes().is_err(), "case {case}");
        assert!(invalid.execute_value().is_err(), "case {case}");
    }
}
