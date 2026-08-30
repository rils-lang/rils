use super::*;

pub(super) fn write_constant(writer: &mut Writer, constant: &Constant) -> Result<()> {
    match constant {
        Constant::Unit => writer.u8(0),
        Constant::Bool(value) => {
            writer.u8(1);
            writer.bool(*value);
        }
        Constant::I8(value) => {
            writer.u8(2);
            writer.i8(*value);
        }
        Constant::I16(value) => {
            writer.u8(3);
            writer.i16(*value);
        }
        Constant::I32(value) => {
            writer.u8(4);
            writer.i32(*value);
        }
        Constant::I64(value) => {
            writer.u8(5);
            writer.i64(*value);
        }
        Constant::I128(value) => {
            writer.u8(6);
            writer.i128(*value);
        }
        Constant::Isize(value) => {
            writer.u8(7);
            writer.i64(*value as i64);
        }
        Constant::U8(value) => {
            writer.u8(8);
            writer.u8(*value);
        }
        Constant::U16(value) => {
            writer.u8(9);
            writer.u16(*value);
        }
        Constant::U32(value) => {
            writer.u8(10);
            writer.u32(*value);
        }
        Constant::U64(value) => {
            writer.u8(11);
            writer.u64(*value);
        }
        Constant::U128(value) => {
            writer.u8(12);
            writer.u128(*value);
        }
        Constant::Usize(value) => {
            writer.u8(13);
            writer.u64(*value as u64);
        }
        Constant::F32(value) => {
            writer.u8(14);
            writer.u32(value.to_bits());
        }
        Constant::F64(value) => {
            writer.u8(15);
            writer.u64(value.to_bits());
        }
        Constant::Char(value) => {
            writer.u8(16);
            writer.u32(*value as u32);
        }
        Constant::String(value) => {
            writer.u8(17);
            writer.string(value)?;
        }
    }
    Ok(())
}

pub(super) fn read_constant(reader: &mut Reader<'_>) -> Result<Constant> {
    match reader.u8()? {
        0 => Ok(Constant::Unit),
        1 => Ok(Constant::Bool(reader.bool()?)),
        2 => Ok(Constant::I8(reader.i8()?)),
        3 => Ok(Constant::I16(reader.i16()?)),
        4 => Ok(Constant::I32(reader.i32()?)),
        5 => Ok(Constant::I64(reader.i64()?)),
        6 => Ok(Constant::I128(reader.i128()?)),
        7 => Ok(Constant::Isize(isize::try_from(reader.i64()?).map_err(
            |_| BytecodeFormatError::new("isize constant is out of range"),
        )?)),
        8 => Ok(Constant::U8(reader.u8()?)),
        9 => Ok(Constant::U16(reader.u16()?)),
        10 => Ok(Constant::U32(reader.u32()?)),
        11 => Ok(Constant::U64(reader.u64()?)),
        12 => Ok(Constant::U128(reader.u128()?)),
        13 => Ok(Constant::Usize(usize::try_from(reader.u64()?).map_err(
            |_| BytecodeFormatError::new("usize constant is out of range"),
        )?)),
        14 => Ok(Constant::F32(f32::from_bits(reader.u32()?))),
        15 => Ok(Constant::F64(f64::from_bits(reader.u64()?))),
        16 => Ok(Constant::Char(char::from_u32(reader.u32()?).ok_or_else(
            || BytecodeFormatError::new("invalid char scalar value"),
        )?)),
        17 => Ok(Constant::String(reader.string()?)),
        value => Err(BytecodeFormatError::new(format!(
            "invalid constant tag {value}"
        ))),
    }
}

pub(super) fn write_literal(writer: &mut Writer, literal: &HirLiteral) -> Result<()> {
    let constant = match literal {
        HirLiteral::Unit => Constant::Unit,
        HirLiteral::Bool(v) => Constant::Bool(*v),
        HirLiteral::I8(v) => Constant::I8(*v),
        HirLiteral::I16(v) => Constant::I16(*v),
        HirLiteral::I32(v) => Constant::I32(*v),
        HirLiteral::I64(v) => Constant::I64(*v),
        HirLiteral::I128(v) => Constant::I128(*v),
        HirLiteral::Isize(v) => Constant::Isize(*v),
        HirLiteral::U8(v) => Constant::U8(*v),
        HirLiteral::U16(v) => Constant::U16(*v),
        HirLiteral::U32(v) => Constant::U32(*v),
        HirLiteral::U64(v) => Constant::U64(*v),
        HirLiteral::U128(v) => Constant::U128(*v),
        HirLiteral::Usize(v) => Constant::Usize(*v),
        HirLiteral::F32(v) => Constant::F32(*v),
        HirLiteral::F64(v) => Constant::F64(*v),
        HirLiteral::Char(v) => Constant::Char(*v),
        HirLiteral::String(v) => Constant::String(v.clone()),
    };
    write_constant(writer, &constant)
}

pub(super) fn read_literal(reader: &mut Reader<'_>) -> Result<HirLiteral> {
    match read_constant(reader)? {
        Constant::Unit => Ok(HirLiteral::Unit),
        Constant::Bool(v) => Ok(HirLiteral::Bool(v)),
        Constant::I8(v) => Ok(HirLiteral::I8(v)),
        Constant::I16(v) => Ok(HirLiteral::I16(v)),
        Constant::I32(v) => Ok(HirLiteral::I32(v)),
        Constant::I64(v) => Ok(HirLiteral::I64(v)),
        Constant::I128(v) => Ok(HirLiteral::I128(v)),
        Constant::Isize(v) => Ok(HirLiteral::Isize(v)),
        Constant::U8(v) => Ok(HirLiteral::U8(v)),
        Constant::U16(v) => Ok(HirLiteral::U16(v)),
        Constant::U32(v) => Ok(HirLiteral::U32(v)),
        Constant::U64(v) => Ok(HirLiteral::U64(v)),
        Constant::U128(v) => Ok(HirLiteral::U128(v)),
        Constant::Usize(v) => Ok(HirLiteral::Usize(v)),
        Constant::F32(v) => Ok(HirLiteral::F32(v)),
        Constant::F64(v) => Ok(HirLiteral::F64(v)),
        Constant::Char(v) => Ok(HirLiteral::Char(v)),
        Constant::String(v) => Ok(HirLiteral::String(v)),
    }
}

pub(super) fn write_pattern(writer: &mut Writer, pattern: &HirPattern, depth: usize) -> Result<()> {
    if depth > MAX_NESTING {
        return Err(BytecodeFormatError::new("pattern nesting exceeds limit"));
    }
    let next = depth + 1;
    match pattern {
        HirPattern::Wildcard => writer.u8(0),
        HirPattern::Binding(local) => {
            writer.u8(1);
            writer.index(*local, "pattern local")?;
        }
        HirPattern::Literal(literal) => {
            writer.u8(2);
            write_literal(writer, literal)?;
        }
        HirPattern::Some(inner) => {
            writer.u8(3);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::None => writer.u8(4),
        HirPattern::Ok(inner) => {
            writer.u8(5);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::Err(inner) => {
            writer.u8(6);
            write_pattern(writer, inner, next)?;
        }
        HirPattern::TupleVariant { path, fields } => {
            writer.u8(7);
            writer.collection(path, |writer, value| writer.string(value))?;
            writer.len(fields.len(), "tuple pattern fields")?;
            for field in fields {
                write_pattern(writer, field, next)?;
            }
        }
        HirPattern::Record { path, fields } => {
            writer.u8(8);
            writer.collection(path, |writer, value| writer.string(value))?;
            writer.len(fields.len(), "record pattern fields")?;
            for (name, field) in fields {
                writer.string(name)?;
                write_pattern(writer, field, next)?;
            }
        }
        HirPattern::Path(path) => {
            writer.u8(9);
            writer.collection(path, |writer, value| writer.string(value))?;
        }
    }
    Ok(())
}

pub(super) fn read_pattern(reader: &mut Reader<'_>) -> Result<HirPattern> {
    reader.nested(|reader| match reader.u8()? {
        0 => Ok(HirPattern::Wildcard),
        1 => Ok(HirPattern::Binding(reader.index()?)),
        2 => Ok(HirPattern::Literal(read_literal(reader)?)),
        3 => Ok(HirPattern::Some(Box::new(read_pattern(reader)?))),
        4 => Ok(HirPattern::None),
        5 => Ok(HirPattern::Ok(Box::new(read_pattern(reader)?))),
        6 => Ok(HirPattern::Err(Box::new(read_pattern(reader)?))),
        7 => Ok(HirPattern::TupleVariant {
            path: reader.collection(Reader::string)?,
            fields: reader.collection(read_pattern)?,
        }),
        8 => {
            let path = reader.collection(Reader::string)?;
            let count = reader.len()?;
            let mut fields = Vec::with_capacity(count);
            for _ in 0..count {
                fields.push((reader.string()?, read_pattern(reader)?));
            }
            Ok(HirPattern::Record { path, fields })
        }
        9 => Ok(HirPattern::Path(reader.collection(Reader::string)?)),
        value => Err(BytecodeFormatError::new(format!(
            "invalid pattern tag {value}"
        ))),
    })
}
