use super::*;

fn storage_instruction(module: &mut BytecodeModule) -> &mut Instruction {
    &mut module
        .functions
        .iter_mut()
        .flat_map(|function| &mut function.instructions)
        .find(|instruction| matches!(instruction.instruction, Instruction::ApplyStorage { .. }))
        .expect("expression requires storage conversion")
        .instruction
}

#[test]
fn storage_operands_round_trip_and_reject_invalid_registers_and_types() {
    let source = include_str!("../../../tests/fixtures/storage/expression.rils");
    let module = compile(source).unwrap();
    let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
    assert_eq!(
        module.execute_value().unwrap().to_string(),
        loaded.execute_value().unwrap().to_string()
    );
    let register_count = loaded.functions[loaded.entry].register_count;
    for case in 0..3 {
        let mut invalid = loaded.clone();
        let Instruction::ApplyStorage {
            destination,
            source,
            expected,
        } = storage_instruction(&mut invalid)
        else {
            unreachable!()
        };
        match case {
            0 => *destination = register_count,
            1 => *source = register_count,
            _ => {
                *expected = Type::Option(Box::new(Type::IntegerVariable(Span {
                    source: SourceId::new(u32::MAX - 1),
                    start: 0,
                    end: 0,
                })))
            }
        }
        let error = invalid.verify().unwrap_err();
        assert!(error.message.contains("storage conversion"), "{error}");
        assert!(invalid.to_bytes().is_err());
        assert!(invalid.execute_value().is_err());
    }
}

#[test]
fn ordinary_direct_calls_keep_their_existing_instruction_path() {
    let module = compile("fn double(value: i32) -> i32 { value + value } double(37)").unwrap();
    assert!(
        !module
            .functions
            .iter()
            .flat_map(|function| &function.instructions)
            .any(|instruction| matches!(instruction.instruction, Instruction::ApplyStorage { .. }))
    );
    assert_eq!(module.execute_value().unwrap().as_i32(), Some(74));
}
