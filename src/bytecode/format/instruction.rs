use super::*;

pub(super) fn write_place(writer: &mut Writer, place: &BytecodePlace) -> Result<()> {
    writer.index(place.local, "place local")?;
    writer.len(place.projections.len(), "place projections")?;
    for projection in &place.projections {
        match projection {
            BytecodeProjection::Field(name) => {
                writer.u8(0);
                writer.string(name)?;
            }
            BytecodeProjection::Index(register) => {
                writer.u8(1);
                writer.index(*register, "index register")?;
            }
        }
    }
    Ok(())
}

pub(super) fn read_place(reader: &mut Reader<'_>) -> Result<BytecodePlace> {
    let local = reader.index()?;
    let count = reader.len()?;
    let mut projections = Vec::with_capacity(count);
    for _ in 0..count {
        projections.push(match reader.u8()? {
            0 => BytecodeProjection::Field(reader.string()?),
            1 => BytecodeProjection::Index(reader.index()?),
            value => {
                return Err(BytecodeFormatError::new(format!(
                    "invalid projection tag {value}"
                )));
            }
        });
    }
    Ok(BytecodePlace { local, projections })
}

pub(super) fn write_unary(value: UnaryOp) -> u8 {
    match value {
        UnaryOp::Negate => 0,
        UnaryOp::Not => 1,
        UnaryOp::Dereference => 2,
    }
}
pub(super) fn read_unary(value: u8) -> Result<UnaryOp> {
    match value {
        0 => Ok(UnaryOp::Negate),
        1 => Ok(UnaryOp::Not),
        2 => Ok(UnaryOp::Dereference),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid unary operator {value}"
        ))),
    }
}
pub(super) fn write_binary(value: BinaryOp) -> u8 {
    match value {
        BinaryOp::Add => 0,
        BinaryOp::Subtract => 1,
        BinaryOp::Multiply => 2,
        BinaryOp::Divide => 3,
        BinaryOp::Remainder => 4,
        BinaryOp::Equal => 5,
        BinaryOp::NotEqual => 6,
        BinaryOp::Greater => 7,
        BinaryOp::GreaterEqual => 8,
        BinaryOp::Less => 9,
        BinaryOp::LessEqual => 10,
    }
}
pub(super) fn read_binary(value: u8) -> Result<BinaryOp> {
    match value {
        0 => Ok(BinaryOp::Add),
        1 => Ok(BinaryOp::Subtract),
        2 => Ok(BinaryOp::Multiply),
        3 => Ok(BinaryOp::Divide),
        4 => Ok(BinaryOp::Remainder),
        5 => Ok(BinaryOp::Equal),
        6 => Ok(BinaryOp::NotEqual),
        7 => Ok(BinaryOp::Greater),
        8 => Ok(BinaryOp::GreaterEqual),
        9 => Ok(BinaryOp::Less),
        10 => Ok(BinaryOp::LessEqual),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid binary operator {value}"
        ))),
    }
}

pub(super) fn write_fields(writer: &mut Writer, fields: &[(String, usize)]) -> Result<()> {
    writer.len(fields.len(), "record fields")?;
    for (name, register) in fields {
        writer.string(name)?;
        writer.index(*register, "field register")?;
    }
    Ok(())
}

pub(super) fn read_fields(reader: &mut Reader<'_>) -> Result<Vec<(String, usize)>> {
    let count = reader.len()?;
    let mut fields = Vec::with_capacity(count);
    for _ in 0..count {
        fields.push((reader.string()?, reader.index()?));
    }
    Ok(fields)
}

pub(super) fn write_instruction(writer: &mut Writer, value: &SpannedInstruction) -> Result<()> {
    writer.span(value.span)?;
    let i = &value.instruction;
    match i {
        Instruction::LoadConstant {
            destination,
            constant,
        } => {
            writer.u8(0);
            writer.index(*destination, "destination")?;
            writer.index(*constant, "constant")?;
        }
        Instruction::LoadFunction {
            destination,
            function,
        } => {
            writer.u8(1);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
        }
        Instruction::BindMethod {
            destination,
            function,
            receiver,
        } => {
            writer.u8(2);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.index(*receiver, "receiver")?;
        }
        Instruction::BorrowTemporary {
            destination,
            source,
            mutable,
        } => {
            writer.u8(3);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.bool(*mutable);
        }
        Instruction::Reborrow {
            destination,
            source,
            mutable,
        } => {
            writer.u8(4);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.bool(*mutable);
        }
        Instruction::CreateClosure {
            destination,
            function,
            captures,
        } => {
            writer.u8(5);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.indices(captures)?;
        }
        Instruction::TakeLocal { destination, local } => {
            writer.u8(6);
            writer.index(*destination, "destination")?;
            writer.index(*local, "local")?;
        }
        Instruction::TakePlace { destination, place } => {
            writer.u8(7);
            writer.index(*destination, "destination")?;
            write_place(writer, place)?;
        }
        Instruction::StoreLocal { local, source } => {
            writer.u8(8);
            writer.index(*local, "local")?;
            writer.index(*source, "source")?;
        }
        Instruction::InitLocal { local, source } => {
            writer.u8(9);
            writer.index(*local, "local")?;
            writer.index(*source, "source")?;
        }
        Instruction::DropLocal { local } => {
            writer.u8(10);
            writer.index(*local, "local")?;
        }
        Instruction::BorrowLocal {
            destination,
            local,
            mutable,
        } => {
            writer.u8(11);
            writer.index(*destination, "destination")?;
            writer.index(*local, "local")?;
            writer.bool(*mutable);
        }
        Instruction::BorrowPlace {
            destination,
            place,
            mutable,
        } => {
            writer.u8(12);
            writer.index(*destination, "destination")?;
            write_place(writer, place)?;
            writer.bool(*mutable);
        }
        Instruction::Dereference {
            destination,
            source,
        } => {
            writer.u8(13);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::StoreDereference { reference, source } => {
            writer.u8(14);
            writer.index(*reference, "reference")?;
            writer.index(*source, "source")?;
        }
        Instruction::StorePlace { place, source } => {
            writer.u8(15);
            write_place(writer, place)?;
            writer.index(*source, "source")?;
        }
        Instruction::IntoIterator {
            destination,
            source,
        } => {
            writer.u8(16);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::Move {
            destination,
            source,
        } => {
            writer.u8(17);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::Unary {
            destination,
            operator,
            operand,
        } => {
            writer.u8(18);
            writer.index(*destination, "destination")?;
            writer.u8(write_unary(*operator));
            writer.index(*operand, "operand")?;
        }
        Instruction::Cast {
            destination,
            source,
            target,
        } => {
            writer.u8(42);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            writer.u8(write_integer_type(*target));
        }
        Instruction::Binary {
            destination,
            left,
            operator,
            right,
        } => {
            writer.u8(19);
            writer.index(*destination, "destination")?;
            writer.index(*left, "left")?;
            writer.u8(write_binary(*operator));
            writer.index(*right, "right")?;
        }
        Instruction::IntegerBinary {
            destination,
            left,
            operator,
            right,
            integer,
        } => {
            writer.u8(44);
            writer.index(*destination, "destination")?;
            writer.index(*left, "left")?;
            writer.u8(write_binary(*operator));
            writer.index(*right, "right")?;
            writer.u8(write_integer_type(*integer));
        }
        Instruction::Call {
            destination,
            function,
            arguments,
        } => {
            writer.u8(20);
            writer.index(*destination, "destination")?;
            writer.index(*function, "function")?;
            writer.indices(arguments)?;
        }
        Instruction::CallValue {
            destination,
            callee,
            arguments,
        } => {
            writer.u8(21);
            writer.index(*destination, "destination")?;
            writer.index(*callee, "callee")?;
            writer.indices(arguments)?;
        }
        Instruction::CallImport {
            destination,
            import,
            arguments,
        } => {
            writer.u8(22);
            writer.index(*destination, "destination")?;
            writer.index(*import, "import")?;
            writer.indices(arguments)?;
        }
        Instruction::CallRuntime {
            destination,
            builtin,
            arguments,
        } => {
            writer.u8(45);
            writer.index(*destination, "destination")?;
            writer.u32(builtin.as_raw());
            writer.indices(arguments)?;
        }
        Instruction::CallIntrinsic {
            destination,
            intrinsic,
            target,
            arguments,
        } => {
            writer.u8(43);
            writer.index(*destination, "destination")?;
            writer.u32(intrinsic.as_raw());
            writer.bool(target.is_some());
            if let Some(target) = target {
                writer.u8(write_integer_type(*target));
            }
            writer.indices(arguments)?;
        }
        Instruction::ConstructRecord {
            destination,
            type_id,
            variant,
            fields,
        } => {
            writer.u8(23);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.bool(variant.is_some());
            if let Some(v) = variant {
                writer.string(v)?;
            }
            write_fields(writer, fields)?;
        }
        Instruction::ConstructTupleVariant {
            destination,
            type_id,
            variant,
            fields,
        } => {
            writer.u8(24);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.string(variant)?;
            writer.indices(fields)?;
        }
        Instruction::ConstructUnitVariant {
            destination,
            type_id,
            variant,
        } => {
            writer.u8(25);
            writer.index(*destination, "destination")?;
            writer.index(*type_id, "type")?;
            writer.string(variant)?;
        }
        Instruction::BuildTuple {
            destination,
            elements,
        } => {
            writer.u8(26);
            writer.index(*destination, "destination")?;
            writer.indices(elements)?;
        }
        Instruction::BuildArray {
            destination,
            elements,
        } => {
            writer.u8(27);
            writer.index(*destination, "destination")?;
            writer.indices(elements)?;
        }
        Instruction::BuildRepeatArray {
            destination,
            value,
            count,
        } => {
            writer.u8(28);
            writer.index(*destination, "destination")?;
            writer.index(*value, "value")?;
            writer.index(*count, "repeat count")?;
        }
        Instruction::BuildRange {
            destination,
            start,
            end,
        } => {
            writer.u8(29);
            writer.index(*destination, "destination")?;
            writer.index(*start, "start")?;
            writer.index(*end, "end")?;
        }
        Instruction::BuildOptionNone { destination } => {
            writer.u8(30);
            writer.index(*destination, "destination")?;
        }
        Instruction::BuildOptionSome {
            destination,
            source,
        } => {
            writer.u8(31);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::BuildResultOk {
            destination,
            source,
        } => {
            writer.u8(32);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::BuildResultErr {
            destination,
            source,
        } => {
            writer.u8(33);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::TryResult {
            destination,
            source,
        } => {
            writer.u8(34);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
        }
        Instruction::MatchPattern {
            destination,
            source,
            pattern,
        } => {
            writer.u8(35);
            writer.index(*destination, "destination")?;
            writer.index(*source, "source")?;
            write_pattern(writer, pattern, 0)?;
        }
        Instruction::BindPattern { source, pattern } => {
            writer.u8(36);
            writer.index(*source, "source")?;
            write_pattern(writer, pattern, 0)?;
        }
        Instruction::Jump { target } => {
            writer.u8(37);
            writer.index(*target, "jump target")?;
        }
        Instruction::Branch {
            condition,
            then_target,
            else_target,
        } => {
            writer.u8(38);
            writer.index(*condition, "condition")?;
            writer.index(*then_target, "then target")?;
            writer.index(*else_target, "else target")?;
        }
        Instruction::IteratorNext {
            iterator,
            destination,
            some_target,
            none_target,
        } => {
            writer.u8(39);
            writer.index(*iterator, "iterator")?;
            writer.index(*destination, "destination")?;
            writer.index(*some_target, "some target")?;
            writer.index(*none_target, "none target")?;
        }
        Instruction::Return { source } => {
            writer.u8(40);
            writer.index(*source, "source")?;
        }
        Instruction::MatchFail => writer.u8(41),
    }
    Ok(())
}

pub(super) fn read_instruction(reader: &mut Reader<'_>) -> Result<SpannedInstruction> {
    let span = reader.span()?;
    let instruction = match reader.u8()? {
        0 => Instruction::LoadConstant {
            destination: reader.index()?,
            constant: reader.index()?,
        },
        1 => Instruction::LoadFunction {
            destination: reader.index()?,
            function: reader.index()?,
        },
        2 => Instruction::BindMethod {
            destination: reader.index()?,
            function: reader.index()?,
            receiver: reader.index()?,
        },
        3 => Instruction::BorrowTemporary {
            destination: reader.index()?,
            source: reader.index()?,
            mutable: reader.bool()?,
        },
        4 => Instruction::Reborrow {
            destination: reader.index()?,
            source: reader.index()?,
            mutable: reader.bool()?,
        },
        5 => Instruction::CreateClosure {
            destination: reader.index()?,
            function: reader.index()?,
            captures: reader.indices()?,
        },
        6 => Instruction::TakeLocal {
            destination: reader.index()?,
            local: reader.index()?,
        },
        7 => Instruction::TakePlace {
            destination: reader.index()?,
            place: read_place(reader)?,
        },
        8 => Instruction::StoreLocal {
            local: reader.index()?,
            source: reader.index()?,
        },
        9 => Instruction::InitLocal {
            local: reader.index()?,
            source: reader.index()?,
        },
        10 => Instruction::DropLocal {
            local: reader.index()?,
        },
        11 => Instruction::BorrowLocal {
            destination: reader.index()?,
            local: reader.index()?,
            mutable: reader.bool()?,
        },
        12 => Instruction::BorrowPlace {
            destination: reader.index()?,
            place: read_place(reader)?,
            mutable: reader.bool()?,
        },
        13 => Instruction::Dereference {
            destination: reader.index()?,
            source: reader.index()?,
        },
        14 => Instruction::StoreDereference {
            reference: reader.index()?,
            source: reader.index()?,
        },
        15 => Instruction::StorePlace {
            place: read_place(reader)?,
            source: reader.index()?,
        },
        16 => Instruction::IntoIterator {
            destination: reader.index()?,
            source: reader.index()?,
        },
        17 => Instruction::Move {
            destination: reader.index()?,
            source: reader.index()?,
        },
        18 => Instruction::Unary {
            destination: reader.index()?,
            operator: read_unary(reader.u8()?)?,
            operand: reader.index()?,
        },
        19 => Instruction::Binary {
            destination: reader.index()?,
            left: reader.index()?,
            operator: read_binary(reader.u8()?)?,
            right: reader.index()?,
        },
        20 => Instruction::Call {
            destination: reader.index()?,
            function: reader.index()?,
            arguments: reader.indices()?,
        },
        21 => Instruction::CallValue {
            destination: reader.index()?,
            callee: reader.index()?,
            arguments: reader.indices()?,
        },
        22 => Instruction::CallImport {
            destination: reader.index()?,
            import: reader.index()?,
            arguments: reader.indices()?,
        },
        23 => {
            let destination = reader.index()?;
            let type_id = reader.index()?;
            let variant = if reader.bool()? {
                Some(reader.string()?)
            } else {
                None
            };
            Instruction::ConstructRecord {
                destination,
                type_id,
                variant,
                fields: read_fields(reader)?,
            }
        }
        24 => Instruction::ConstructTupleVariant {
            destination: reader.index()?,
            type_id: reader.index()?,
            variant: reader.string()?,
            fields: reader.indices()?,
        },
        25 => Instruction::ConstructUnitVariant {
            destination: reader.index()?,
            type_id: reader.index()?,
            variant: reader.string()?,
        },
        26 => Instruction::BuildTuple {
            destination: reader.index()?,
            elements: reader.indices()?,
        },
        27 => Instruction::BuildArray {
            destination: reader.index()?,
            elements: reader.indices()?,
        },
        28 => Instruction::BuildRepeatArray {
            destination: reader.index()?,
            value: reader.index()?,
            count: reader.index()?,
        },
        29 => Instruction::BuildRange {
            destination: reader.index()?,
            start: reader.index()?,
            end: reader.index()?,
        },
        30 => Instruction::BuildOptionNone {
            destination: reader.index()?,
        },
        31 => Instruction::BuildOptionSome {
            destination: reader.index()?,
            source: reader.index()?,
        },
        32 => Instruction::BuildResultOk {
            destination: reader.index()?,
            source: reader.index()?,
        },
        33 => Instruction::BuildResultErr {
            destination: reader.index()?,
            source: reader.index()?,
        },
        34 => Instruction::TryResult {
            destination: reader.index()?,
            source: reader.index()?,
        },
        35 => Instruction::MatchPattern {
            destination: reader.index()?,
            source: reader.index()?,
            pattern: read_pattern(reader)?,
        },
        36 => Instruction::BindPattern {
            source: reader.index()?,
            pattern: read_pattern(reader)?,
        },
        37 => Instruction::Jump {
            target: reader.index()?,
        },
        38 => Instruction::Branch {
            condition: reader.index()?,
            then_target: reader.index()?,
            else_target: reader.index()?,
        },
        39 => Instruction::IteratorNext {
            iterator: reader.index()?,
            destination: reader.index()?,
            some_target: reader.index()?,
            none_target: reader.index()?,
        },
        40 => Instruction::Return {
            source: reader.index()?,
        },
        41 => Instruction::MatchFail,
        42 => Instruction::Cast {
            destination: reader.index()?,
            source: reader.index()?,
            target: read_integer_type(reader.u8()?)?,
        },
        43 => {
            let destination = reader.index()?;
            let raw_intrinsic = reader.u32()?;
            let intrinsic = rils_builtins::BuiltinId::from_raw(raw_intrinsic);
            if rils_builtins::intrinsic(intrinsic).is_none() {
                return Err(BytecodeFormatError::new(format!(
                    "invalid intrinsic built-in ID {raw_intrinsic:#x}"
                )));
            }
            let target = reader
                .bool()?
                .then(|| read_integer_type(reader.u8()?))
                .transpose()?;
            Instruction::CallIntrinsic {
                destination,
                intrinsic,
                target,
                arguments: reader.indices()?,
            }
        }
        44 => Instruction::IntegerBinary {
            destination: reader.index()?,
            left: reader.index()?,
            operator: read_binary(reader.u8()?)?,
            right: reader.index()?,
            integer: read_integer_type(reader.u8()?)?,
        },
        45 => {
            let destination = reader.index()?;
            let raw_builtin = reader.u32()?;
            let builtin = rils_builtins::BuiltinId::from_raw(raw_builtin);
            if !builtin.has_direct_runtime_call() {
                return Err(BytecodeFormatError::new(format!(
                    "invalid runtime built-in ID {raw_builtin:#x}"
                )));
            }
            Instruction::CallRuntime {
                destination,
                builtin,
                arguments: reader.indices()?,
            }
        }
        value => {
            return Err(BytecodeFormatError::new(format!(
                "invalid instruction opcode {value}"
            )));
        }
    };
    Ok(SpannedInstruction { instruction, span })
}
