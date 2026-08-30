use super::*;

#[test]
fn rejects_unresolved_identity_scoped_inference_types() {
    let id = rils_frontend::ExprId {
        source: rils_frontend::SourceId::new(1),
        local: 2,
    };
    for ty in [Type::IntegerInference(id), Type::FloatInference(id)] {
        let error = write_type(&mut Writer(Vec::new()), &ty, 0)
            .expect_err("inference-only types must not enter bytecode");
        assert!(error.message.contains("unresolved inference type"));
    }
}

#[test]
fn round_trip_executes_the_same_module() {
    let module = crate::bytecode::compile(
        r#"
            struct Pair { left: i32, right: i32 }
            struct CounterRange { current: i32, end: i32 }
            enum Choice { None, Some(i32), Named { value: i32 } }
            impl Iterator for CounterRange {
                type Item = i32;
                fn next(&mut self) -> Option<i32> {
                    if self.current < self.end {
                        let value = self.current;
                        let end = self.end;
                        *self = CounterRange { current: value + 1, end: end };
                        Some(value)
                    } else { None }
                }
            }
            fn calculate() -> i32 {
                let values = [1, 2, 3];
                let mut total = 0;
                for value in values { total = total + value; }
                for value in CounterRange { current: 1, end: 4 } {
                    total = total + value;
                }
                let _kind = type_of(total);
                let pair = Pair { left: total, right: 4 };
                match Choice::Named { value: pair.left } {
                    Choice::Named { value } => value + pair.right,
                    _ => 0,
                }
            }
            calculate()
        "#,
    )
    .expect("source compiles");
    let bytes = module.to_bytes().expect("module serializes");
    let loaded = BytecodeModule::from_bytes(&bytes).expect("module loads");
    let value = loaded.execute().expect("module runs");
    assert_eq!(value, crate::Value::I32(16));
}

#[test]
fn round_trip_preserves_source_ids_and_rejects_unknown_span_sources() {
    let source_id = SourceId::new(9);
    let tokens = crate::lexer::lex_with_source_id("1 / 0", source_id).unwrap();
    let program = crate::parser::parse(tokens).unwrap();
    let module = crate::bytecode::compile_program_with_host_and_sources(
        &program,
        &crate::HostContract::new(),
        vec![SourceFile {
            id: source_id,
            name: "math.rils".into(),
        }],
    )
    .unwrap();
    let mut bytes = module.to_bytes().unwrap();
    let loaded = BytecodeModule::from_bytes(&bytes).unwrap();
    let error = loaded.execute().unwrap_err();
    assert_eq!(error.span.source, source_id);
    assert_eq!(loaded.source_name(source_id), Some("math.rils"));

    let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
    let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
    let functions_entry = (0..section_count)
        .find_map(|index| {
            let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
            (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()) == SECTION_FUNCTIONS)
                .then_some(start)
        })
        .unwrap();
    let functions_offset = u32::from_le_bytes(
        bytes[functions_entry + 4..functions_entry + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let mut reader = Reader::new(&bytes[functions_offset..]);
    let _function_count = reader.index().unwrap();
    let _name = reader.string().unwrap();
    let _exported = reader.bool().unwrap();
    let _constants = reader.collection(read_constant).unwrap();
    let _instruction_count = reader.index().unwrap();
    let instruction_source_offset = functions_offset + reader.position;
    bytes[instruction_source_offset..instruction_source_offset + 4]
        .copy_from_slice(&999_u32.to_le_bytes());
    let checksum = crc32(&bytes[directory_end..]);
    bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
    let error = match BytecodeModule::from_bytes(&bytes) {
        Ok(_) => panic!("unknown span source should be rejected"),
        Err(error) => error,
    };
    assert!(error.message.contains("unknown source"));
}

#[test]
fn rejects_corrupted_payload() {
    let module = crate::bytecode::compile("1 + 2").expect("source compiles");
    let mut bytes = module.to_bytes().expect("module serializes");
    *bytes.last_mut().expect("payload exists") ^= 0xff;
    let error = BytecodeModule::from_bytes(&bytes)
        .err()
        .expect("corruption rejected");
    assert!(error.message.contains("checksum"));
}

#[test]
fn rejects_invalid_instruction_after_checksum_is_updated() {
    let module = crate::bytecode::compile("1 + 2").expect("source compiles");
    let mut bytes = module.to_bytes().expect("module serializes");
    let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
    let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
    let functions_entry = (0..section_count)
        .find_map(|index| {
            let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
            (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()) == SECTION_FUNCTIONS)
                .then_some(start)
        })
        .unwrap();
    let functions_offset = u32::from_le_bytes(
        bytes[functions_entry + 4..functions_entry + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    // Skip function count, name, exported flag, constants, then overwrite the first opcode.
    let mut reader = Reader::new(&bytes[functions_offset..]);
    let _function_count = reader.index().unwrap();
    let _name = reader.string().unwrap();
    let _exported = reader.bool().unwrap();
    let constants = reader.collection(read_constant).unwrap();
    assert!(!constants.is_empty());
    let _instruction_count = reader.index().unwrap();
    let _span = reader.span().unwrap();
    let opcode_offset = functions_offset + reader.position;
    bytes[opcode_offset] = 0xff;
    let checksum = crc32(&bytes[directory_end..]);
    bytes[28..32].copy_from_slice(&checksum.to_le_bytes());
    let error = BytecodeModule::from_bytes(&bytes)
        .err()
        .expect("invalid opcode rejected");
    assert!(error.message.contains("opcode"));
}

#[test]
fn intrinsic_instructions_store_and_validate_u32_builtin_ids() {
    let instruction = SpannedInstruction {
        instruction: Instruction::CallIntrinsic {
            destination: 1,
            intrinsic: rils_builtins::BuiltinId::IntegerCheckedAdd,
            target: Some(IntegerType::I32),
            arguments: vec![2, 3],
        },
        span: Span::default(),
    };
    let mut writer = Writer::default();
    write_instruction(&mut writer, &instruction).unwrap();
    let mut bytes = writer.finish();

    // Span (20), opcode (1), and destination (4) precede the stable ID.
    let id_offset = 25;
    assert_eq!(
        u32::from_le_bytes(bytes[id_offset..id_offset + 4].try_into().unwrap()),
        0x0B10
    );
    let decoded = read_instruction(&mut Reader::new(&bytes)).unwrap();
    assert!(matches!(
        decoded.instruction,
        Instruction::CallIntrinsic {
            intrinsic: rils_builtins::BuiltinId::IntegerCheckedAdd,
            ..
        }
    ));

    bytes[id_offset..id_offset + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    let error = read_instruction(&mut Reader::new(&bytes))
        .err()
        .expect("unknown built-in ID should be rejected");
    assert!(error.message.contains("invalid intrinsic built-in ID"));
}

#[test]
fn runtime_instructions_store_and_validate_u32_builtin_ids() {
    let instruction = SpannedInstruction {
        instruction: Instruction::CallRuntime {
            destination: 1,
            builtin: rils_builtins::BuiltinId::VecPush,
            arguments: vec![2, 3],
        },
        span: Span::default(),
    };
    let mut writer = Writer::default();
    write_instruction(&mut writer, &instruction).unwrap();
    let mut bytes = writer.finish();

    // Span (20), opcode (1), and destination (4) precede the stable ID.
    let id_offset = 25;
    assert_eq!(
        u32::from_le_bytes(bytes[id_offset..id_offset + 4].try_into().unwrap()),
        0x0200
    );
    let decoded = read_instruction(&mut Reader::new(&bytes)).unwrap();
    assert!(matches!(
        decoded.instruction,
        Instruction::CallRuntime {
            builtin: rils_builtins::BuiltinId::VecPush,
            ..
        }
    ));

    bytes[id_offset..id_offset + 4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    let error = read_instruction(&mut Reader::new(&bytes))
        .err()
        .expect("unknown runtime built-in ID should be rejected");
    assert!(error.message.contains("invalid runtime built-in ID"));
}

#[test]
fn rejects_excessive_register_allocation_before_execution() {
    let module = crate::bytecode::compile("1 + 2").expect("source compiles");
    let mut bytes = module.to_bytes().expect("module serializes");
    let section_count = u16::from_le_bytes(bytes[22..24].try_into().unwrap()) as usize;
    let directory_end = HEADER_LEN + section_count * DIRECTORY_ENTRY_LEN;
    let functions_entry = (0..section_count)
        .find_map(|index| {
            let start = HEADER_LEN + index * DIRECTORY_ENTRY_LEN;
            (u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()) == SECTION_FUNCTIONS)
                .then_some(start)
        })
        .unwrap();
    let functions_offset = u32::from_le_bytes(
        bytes[functions_entry + 4..functions_entry + 8]
            .try_into()
            .unwrap(),
    ) as usize;
    let mut reader = Reader::new(&bytes[functions_offset..]);
    let _function_count = reader.index().unwrap();
    let _name = reader.string().unwrap();
    let _exported = reader.bool().unwrap();
    let _constants = reader.collection(read_constant).unwrap();
    let _instructions = reader.collection(read_instruction).unwrap();
    let register_count_offset = functions_offset + reader.position;
    bytes[register_count_offset..register_count_offset + 4]
        .copy_from_slice(&((MAX_REGISTERS_PER_FUNCTION as u32) + 1).to_le_bytes());
    let checksum = crc32(&bytes[directory_end..]);
    bytes[28..32].copy_from_slice(&checksum.to_le_bytes());

    let error = BytecodeModule::from_bytes(&bytes)
        .err()
        .expect("excessive register allocation rejected");
    assert!(error.message.contains("register count"));
}
