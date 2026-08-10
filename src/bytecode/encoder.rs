use super::*;

fn encode_place(place: rils_compiler::mir::MirPlace) -> BytecodePlace {
    BytecodePlace {
        local: place.local,
        projections: place
            .projections
            .into_iter()
            .map(|projection| match projection {
                rils_compiler::mir::MirProjection::Field(field) => BytecodeProjection::Field(field),
                rils_compiler::mir::MirProjection::Index(index) => BytecodeProjection::Index(index),
            })
            .collect(),
    }
}

pub(super) fn encode(program: MirProgram) -> Result<BytecodeModule, CompileError> {
    let types = program.types.into_iter().map(runtime_type).collect();
    let mut imports = Vec::new();
    let mut import_ids = HashMap::new();
    let mut functions = Vec::with_capacity(program.functions.len());
    for function in program.functions {
        functions.push(encode_function(function, &mut imports, &mut import_ids)?);
    }
    let module = BytecodeModule {
        functions,
        types,
        imports,
        iterators: program
            .iterators
            .into_iter()
            .map(|(name, methods)| {
                (
                    name,
                    BytecodeIteratorMethods {
                        into_iter: methods.into_iter,
                        next: methods.next,
                    },
                )
            })
            .collect(),
        entry: program.entry,
    };
    module.verify().map_err(|error| CompileError {
        message: error.message,
        span: error.span,
    })?;
    Ok(module)
}

fn runtime_type(definition: HirTypeDefinition) -> RuntimeType {
    match definition {
        HirTypeDefinition::Struct {
            name,
            generic_parameters,
            fields,
        } => RuntimeType::Struct(Rc::new(StructType {
            name,
            generic_parameters,
            fields,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        })),
        HirTypeDefinition::Enum {
            name,
            generic_parameters,
            variants,
        } => RuntimeType::Enum(Rc::new(EnumType {
            name,
            generic_parameters,
            variants,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        })),
    }
}

fn encode_function(
    program: MirFunction,
    imports: &mut Vec<BytecodeImport>,
    import_ids: &mut HashMap<String, usize>,
) -> Result<BytecodeFunction, CompileError> {
    let mut offsets = Vec::with_capacity(program.blocks.len());
    let mut offset = 0;
    for block in &program.blocks {
        offsets.push(offset);
        offset += block.instructions.len() + usize::from(block.terminator.is_some());
    }
    let mut instructions = Vec::with_capacity(offset);
    for block in program.blocks {
        for instruction in block.instructions {
            instructions.push(SpannedInstruction {
                instruction: match instruction.instruction {
                    MirInstruction::LoadConstant {
                        destination,
                        constant,
                    } => Instruction::LoadConstant {
                        destination,
                        constant,
                    },
                    MirInstruction::LoadFunction {
                        destination,
                        function,
                    } => Instruction::LoadFunction {
                        destination,
                        function,
                    },
                    MirInstruction::BindMethod {
                        destination,
                        function,
                        receiver,
                    } => Instruction::BindMethod {
                        destination,
                        function,
                        receiver,
                    },
                    MirInstruction::BorrowTemporary {
                        destination,
                        source,
                        mutable,
                    } => Instruction::BorrowTemporary {
                        destination,
                        source,
                        mutable,
                    },
                    MirInstruction::Reborrow {
                        destination,
                        source,
                        mutable,
                    } => Instruction::Reborrow {
                        destination,
                        source,
                        mutable,
                    },
                    MirInstruction::CreateClosure {
                        destination,
                        function,
                        captures,
                    } => Instruction::CreateClosure {
                        destination,
                        function,
                        captures,
                    },
                    MirInstruction::TakeLocal { destination, local } => {
                        Instruction::TakeLocal { destination, local }
                    }
                    MirInstruction::TakePlace { destination, place } => Instruction::TakePlace {
                        destination,
                        place: encode_place(place),
                    },
                    MirInstruction::StoreLocal { local, source } => {
                        Instruction::StoreLocal { local, source }
                    }
                    MirInstruction::InitLocal { local, source } => {
                        Instruction::InitLocal { local, source }
                    }
                    MirInstruction::DropLocal { local } => Instruction::DropLocal { local },
                    MirInstruction::BorrowLocal {
                        destination,
                        local,
                        mutable,
                    } => Instruction::BorrowLocal {
                        destination,
                        local,
                        mutable,
                    },
                    MirInstruction::BorrowPlace {
                        destination,
                        place,
                        mutable,
                    } => Instruction::BorrowPlace {
                        destination,
                        place: encode_place(place),
                        mutable,
                    },
                    MirInstruction::Dereference {
                        destination,
                        source,
                    } => Instruction::Dereference {
                        destination,
                        source,
                    },
                    MirInstruction::StoreDereference { reference, source } => {
                        Instruction::StoreDereference { reference, source }
                    }
                    MirInstruction::StorePlace { place, source } => Instruction::StorePlace {
                        place: encode_place(place),
                        source,
                    },
                    MirInstruction::IntoIterator {
                        destination,
                        source,
                    } => Instruction::IntoIterator {
                        destination,
                        source,
                    },
                    MirInstruction::Move {
                        destination,
                        source,
                    } => Instruction::Move {
                        destination,
                        source,
                    },
                    MirInstruction::Unary {
                        destination,
                        operator,
                        operand,
                    } => Instruction::Unary {
                        destination,
                        operator,
                        operand,
                    },
                    MirInstruction::Binary {
                        destination,
                        left,
                        operator,
                        right,
                    } => Instruction::Binary {
                        destination,
                        left,
                        operator,
                        right,
                    },
                    MirInstruction::Call {
                        destination,
                        function,
                        arguments,
                    } => Instruction::Call {
                        destination,
                        function,
                        arguments,
                    },
                    MirInstruction::CallValue {
                        destination,
                        callee,
                        arguments,
                    } => Instruction::CallValue {
                        destination,
                        callee,
                        arguments,
                    },
                    MirInstruction::CallImport {
                        destination,
                        name,
                        signature,
                        capability,
                        arguments,
                    } => {
                        let import = if let Some(import) = import_ids.get(&name).copied() {
                            let existing = &imports[import];
                            if existing.signature != signature || existing.capability != capability
                            {
                                return Err(CompileError::unsupported(
                                    format!("inconsistent import declaration for `{name}`"),
                                    instruction.span,
                                ));
                            }
                            import
                        } else {
                            let import = imports.len();
                            imports.push(BytecodeImport {
                                name: name.clone(),
                                signature,
                                abi_version: BYTECODE_HOST_ABI_VERSION,
                                capability,
                            });
                            import_ids.insert(name, import);
                            import
                        };
                        Instruction::CallImport {
                            destination,
                            import,
                            arguments,
                        }
                    }
                    MirInstruction::ConstructRecord {
                        destination,
                        type_id,
                        variant,
                        fields,
                    } => Instruction::ConstructRecord {
                        destination,
                        type_id,
                        variant,
                        fields,
                    },
                    MirInstruction::ConstructTupleVariant {
                        destination,
                        type_id,
                        variant,
                        fields,
                    } => Instruction::ConstructTupleVariant {
                        destination,
                        type_id,
                        variant,
                        fields,
                    },
                    MirInstruction::ConstructUnitVariant {
                        destination,
                        type_id,
                        variant,
                    } => Instruction::ConstructUnitVariant {
                        destination,
                        type_id,
                        variant,
                    },
                    MirInstruction::BuildTuple {
                        destination,
                        elements,
                    } => Instruction::BuildTuple {
                        destination,
                        elements,
                    },
                    MirInstruction::BuildArray {
                        destination,
                        elements,
                    } => Instruction::BuildArray {
                        destination,
                        elements,
                    },
                    MirInstruction::BuildRepeatArray {
                        destination,
                        value,
                        count,
                    } => Instruction::BuildRepeatArray {
                        destination,
                        value,
                        count,
                    },
                    MirInstruction::BuildRange {
                        destination,
                        start,
                        end,
                    } => Instruction::BuildRange {
                        destination,
                        start,
                        end,
                    },
                    MirInstruction::BuildOptionNone { destination } => {
                        Instruction::BuildOptionNone { destination }
                    }
                    MirInstruction::BuildOptionSome {
                        destination,
                        source,
                    } => Instruction::BuildOptionSome {
                        destination,
                        source,
                    },
                    MirInstruction::BuildResultOk {
                        destination,
                        source,
                    } => Instruction::BuildResultOk {
                        destination,
                        source,
                    },
                    MirInstruction::BuildResultErr {
                        destination,
                        source,
                    } => Instruction::BuildResultErr {
                        destination,
                        source,
                    },
                    MirInstruction::TryResult {
                        destination,
                        source,
                    } => Instruction::TryResult {
                        destination,
                        source,
                    },
                    MirInstruction::MatchPattern {
                        destination,
                        source,
                        pattern,
                    } => Instruction::MatchPattern {
                        destination,
                        source,
                        pattern,
                    },
                    MirInstruction::BindPattern { source, pattern } => {
                        Instruction::BindPattern { source, pattern }
                    }
                },
                span: instruction.span,
            });
        }
        let Some(terminator) = block.terminator else {
            continue;
        };
        instructions.push(SpannedInstruction {
            instruction: match terminator.terminator {
                MirTerminator::Goto(block) => Instruction::Jump {
                    target: offsets[block],
                },
                MirTerminator::Branch {
                    condition,
                    then_block,
                    else_block,
                } => Instruction::Branch {
                    condition,
                    then_target: offsets[then_block],
                    else_target: offsets[else_block],
                },
                MirTerminator::IteratorNext {
                    iterator,
                    destination,
                    some_block,
                    none_block,
                } => Instruction::IteratorNext {
                    iterator,
                    destination,
                    some_target: offsets[some_block],
                    none_target: offsets[none_block],
                },
                MirTerminator::Return(source) => Instruction::Return { source },
                MirTerminator::MatchFail => Instruction::MatchFail,
            },
            span: terminator.span,
        });
    }
    let constants = program
        .constants
        .into_iter()
        .map(|constant| match constant {
            HirLiteral::Unit => Constant::Unit,
            HirLiteral::Bool(value) => Constant::Bool(value),
            HirLiteral::I8(value) => Constant::I8(value),
            HirLiteral::I16(value) => Constant::I16(value),
            HirLiteral::I32(value) => Constant::I32(value),
            HirLiteral::I64(value) => Constant::I64(value),
            HirLiteral::I128(value) => Constant::I128(value),
            HirLiteral::Isize(value) => Constant::Isize(value),
            HirLiteral::U8(value) => Constant::U8(value),
            HirLiteral::U16(value) => Constant::U16(value),
            HirLiteral::U32(value) => Constant::U32(value),
            HirLiteral::U64(value) => Constant::U64(value),
            HirLiteral::U128(value) => Constant::U128(value),
            HirLiteral::Usize(value) => Constant::Usize(value),
            HirLiteral::F32(value) => Constant::F32(value),
            HirLiteral::F64(value) => Constant::F64(value),
            HirLiteral::Char(value) => Constant::Char(value),
            HirLiteral::String(value) => Constant::String(value),
        })
        .collect();
    Ok(BytecodeFunction {
        name: program.name,
        exported: program.exported,
        constants,
        instructions,
        register_count: program.register_count,
        local_count: program.local_count,
        local_mutability: program.local_mutability,
        parameter_count: program.parameter_count,
        capture_count: program.capture_count,
        span: program.span,
    })
}
