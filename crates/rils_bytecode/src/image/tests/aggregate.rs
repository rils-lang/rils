use super::*;

#[test]
fn aggregate_witnesses_and_registers_are_verified_after_round_trip() {
    for (kind, source) in [
        (
            0,
            include_str!("../../../tests/fixtures/native_aggregate_construct/tuple.rils"),
        ),
        (
            1,
            include_str!("../../../tests/fixtures/native_aggregate_construct/array.rils"),
        ),
        (
            2,
            include_str!("../../../tests/fixtures/native_aggregate_construct/repeat.rils"),
        ),
    ] {
        let module = compile(source).unwrap();
        let loaded = BytecodeModule::from_bytes(&module.to_bytes().unwrap()).unwrap();
        assert!(matches!(loaded.execute_value().unwrap(), Value::Dynamic(_)));
        for case in 0..5 {
            let mut invalid = loaded.clone();
            let instruction = invalid
                .functions
                .iter_mut()
                .flat_map(|function| &mut function.instructions)
                .find(|entry| {
                    matches!(
                        (&entry.instruction, kind),
                        (Instruction::BuildTuple { .. }, 0)
                            | (Instruction::BuildArray { .. }, 1)
                            | (Instruction::BuildRepeatArray { .. }, 2)
                    )
                })
                .unwrap();
            let (expected, destination, input) = match &mut instruction.instruction {
                Instruction::BuildTuple {
                    expected,
                    destination,
                    elements,
                }
                | Instruction::BuildArray {
                    expected,
                    destination,
                    elements,
                } => (expected, destination, &mut elements[0]),
                Instruction::BuildRepeatArray {
                    expected,
                    destination,
                    count,
                    ..
                } => (expected, destination, count),
                _ => unreachable!(),
            };
            match case {
                0 => *expected = Type::Unknown,
                1 => *expected = Type::Bool,
                2 => {
                    *expected = if kind == 0 {
                        Type::Tuple(vec![Type::I32])
                    } else {
                        Type::Array {
                            element: Box::new(Type::Unknown),
                            length: 2,
                        }
                    }
                }
                3 => *destination = usize::MAX,
                _ => *input = usize::MAX,
            }
            assert!(invalid.verify().is_err(), "kind={kind}, case={case}");
            assert!(invalid.to_bytes().is_err());
            assert!(invalid.execute_value().is_err());
        }
    }
}
