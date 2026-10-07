use super::*;

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
