use super::*;

pub(super) fn write_signature(writer: &mut Writer, signature: &FunctionSignature) -> Result<()> {
    writer.bool(signature.parameters.is_some());
    if let Some(parameters) = &signature.parameters {
        writer.collection(parameters, |writer, value| write_type(writer, value, 0))?;
    }
    write_type(writer, &signature.return_type, 0)
}

pub(super) fn read_signature(reader: &mut Reader<'_>) -> Result<FunctionSignature> {
    let parameters = if reader.bool()? {
        Some(reader.collection(read_type)?)
    } else {
        None
    };
    Ok(FunctionSignature {
        parameters,
        return_type: read_type(reader)?,
    })
}

pub(super) fn write_type(writer: &mut Writer, value: &Type, depth: usize) -> Result<()> {
    if depth > MAX_NESTING {
        return Err(BytecodeFormatError::new("type nesting exceeds limit"));
    }
    let next = depth + 1;
    match value {
        Type::Unit => writer.u8(0),
        Type::Bool => writer.u8(1),
        Type::Integer(kind) => {
            writer.u8(2);
            writer.u8(write_integer_type(*kind));
        }
        Type::Float(kind) => {
            writer.u8(3);
            writer.u8(match kind {
                FloatType::F32 => 0,
                FloatType::F64 => 1,
            });
        }
        Type::IntegerVariable(span) => {
            writer.u8(4);
            writer.span(*span)?;
        }
        Type::FloatVariable(span) => {
            writer.u8(5);
            writer.span(*span)?;
        }
        Type::IntegerInference(_) | Type::FloatInference(_) => {
            return Err(BytecodeFormatError::new(
                "unresolved inference type cannot be serialized",
            ));
        }
        Type::Char => writer.u8(6),
        Type::String => writer.u8(7),
        Type::Tuple(elements) => {
            writer.u8(8);
            writer.len(elements.len(), "tuple type")?;
            for element in elements {
                write_type(writer, element, next)?;
            }
        }
        Type::Array { element, length } => {
            writer.u8(9);
            write_type(writer, element, next)?;
            writer.index(*length, "array length")?;
        }
        Type::Reference { mutable, inner } => {
            writer.u8(10);
            writer.bool(*mutable);
            write_type(writer, inner, next)?;
        }
        Type::Function {
            parameters,
            return_type,
        } => {
            writer.u8(11);
            writer.bool(parameters.is_some());
            if let Some(parameters) = parameters {
                writer.len(parameters.len(), "function parameters")?;
                for parameter in parameters {
                    write_type(writer, parameter, next)?;
                }
            }
            write_type(writer, return_type, next)?;
        }
        Type::Option(inner) => {
            writer.u8(12);
            write_type(writer, inner, next)?;
        }
        Type::Result(ok, error) => {
            writer.u8(13);
            write_type(writer, ok, next)?;
            write_type(writer, error, next)?;
        }
        Type::Named { name, arguments } => {
            writer.u8(14);
            writer.string(name)?;
            writer.len(arguments.len(), "type arguments")?;
            for argument in arguments {
                write_type(writer, argument, next)?;
            }
        }
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => {
            writer.u8(15);
            write_type(writer, base, next)?;
            writer.bool(trait_name.is_some());
            if let Some(trait_name) = trait_name {
                writer.string(trait_name)?;
            }
            writer.string(name)?;
            writer.len(arguments.len(), "associated type arguments")?;
            for argument in arguments {
                write_type(writer, argument, next)?;
            }
        }
        Type::Variable(name) => {
            writer.u8(16);
            writer.string(name)?;
        }
        Type::Unknown => writer.u8(17),
    }
    Ok(())
}

pub(super) fn read_type(reader: &mut Reader<'_>) -> Result<Type> {
    reader.nested(|reader| match reader.u8()? {
        0 => Ok(Type::Unit),
        1 => Ok(Type::Bool),
        2 => Ok(Type::Integer(read_integer_type(reader.u8()?)?)),
        3 => Ok(Type::Float(match reader.u8()? {
            0 => FloatType::F32,
            1 => FloatType::F64,
            value => {
                return Err(BytecodeFormatError::new(format!(
                    "invalid float type {value}"
                )));
            }
        })),
        4 => Ok(Type::IntegerVariable(reader.span()?)),
        5 => Ok(Type::FloatVariable(reader.span()?)),
        6 => Ok(Type::Char),
        7 => Ok(Type::String),
        8 => Ok(Type::Tuple(reader.collection(read_type)?)),
        9 => Ok(Type::Array {
            element: Box::new(read_type(reader)?),
            length: reader.index()?,
        }),
        10 => Ok(Type::Reference {
            mutable: reader.bool()?,
            inner: Box::new(read_type(reader)?),
        }),
        11 => {
            let parameters = if reader.bool()? {
                Some(reader.collection(read_type)?)
            } else {
                None
            };
            Ok(Type::Function {
                parameters,
                return_type: Box::new(read_type(reader)?),
            })
        }
        12 => Ok(Type::Option(Box::new(read_type(reader)?))),
        13 => Ok(Type::Result(
            Box::new(read_type(reader)?),
            Box::new(read_type(reader)?),
        )),
        14 => Ok(Type::Named {
            name: reader.string()?,
            arguments: reader.collection(read_type)?,
        }),
        15 => {
            let base = Box::new(read_type(reader)?);
            let trait_name = if reader.bool()? {
                Some(reader.string()?)
            } else {
                None
            };
            let name = reader.string()?;
            let arguments = reader.collection(read_type)?;
            Ok(Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            })
        }
        16 => Ok(Type::Variable(reader.string()?)),
        17 => Ok(Type::Unknown),
        value => Err(BytecodeFormatError::new(format!(
            "invalid type tag {value}"
        ))),
    })
}

pub(super) fn write_integer_type(value: IntegerType) -> u8 {
    match value {
        IntegerType::I8 => 0,
        IntegerType::I16 => 1,
        IntegerType::I32 => 2,
        IntegerType::I64 => 3,
        IntegerType::I128 => 4,
        IntegerType::Isize => 5,
        IntegerType::U8 => 6,
        IntegerType::U16 => 7,
        IntegerType::U32 => 8,
        IntegerType::U64 => 9,
        IntegerType::U128 => 10,
        IntegerType::Usize => 11,
    }
}

pub(super) fn read_integer_type(value: u8) -> Result<IntegerType> {
    match value {
        0 => Ok(IntegerType::I8),
        1 => Ok(IntegerType::I16),
        2 => Ok(IntegerType::I32),
        3 => Ok(IntegerType::I64),
        4 => Ok(IntegerType::I128),
        5 => Ok(IntegerType::Isize),
        6 => Ok(IntegerType::U8),
        7 => Ok(IntegerType::U16),
        8 => Ok(IntegerType::U32),
        9 => Ok(IntegerType::U64),
        10 => Ok(IntegerType::U128),
        11 => Ok(IntegerType::Usize),
        _ => Err(BytecodeFormatError::new(format!(
            "invalid integer type {value}"
        ))),
    }
}

pub(super) fn write_generic_parameter(
    writer: &mut Writer,
    parameter: &GenericParameter,
) -> Result<()> {
    writer.string(&parameter.name)?;
    writer.collection(&parameter.bounds, |writer, value| writer.string(value))?;
    writer.span(parameter.span)
}

pub(super) fn read_generic_parameter(reader: &mut Reader<'_>) -> Result<GenericParameter> {
    Ok(GenericParameter {
        name: reader.string()?,
        bounds: reader.collection(Reader::string)?,
        span: reader.span()?,
    })
}

pub(super) fn write_named_field(writer: &mut Writer, field: &NamedField) -> Result<()> {
    writer.string(&field.name)?;
    write_type(writer, &field.type_annotation, 0)?;
    writer.span(field.span)
}

pub(super) fn read_named_field(reader: &mut Reader<'_>) -> Result<NamedField> {
    Ok(NamedField {
        name: reader.string()?,
        type_annotation: read_type(reader)?,
        span: reader.span()?,
    })
}

pub(super) fn write_enum_variant(writer: &mut Writer, variant: &EnumVariant) -> Result<()> {
    match variant {
        EnumVariant::Unit { name, span } => {
            writer.u8(0);
            writer.string(name)?;
            writer.span(*span)?;
        }
        EnumVariant::Tuple { name, fields, span } => {
            writer.u8(1);
            writer.string(name)?;
            writer.collection(fields, |writer, value| write_type(writer, value, 0))?;
            writer.span(*span)?;
        }
        EnumVariant::Record { name, fields, span } => {
            writer.u8(2);
            writer.string(name)?;
            writer.collection(fields, write_named_field)?;
            writer.span(*span)?;
        }
    }
    Ok(())
}

pub(super) fn read_enum_variant(reader: &mut Reader<'_>) -> Result<EnumVariant> {
    match reader.u8()? {
        0 => Ok(EnumVariant::Unit {
            name: reader.string()?,
            span: reader.span()?,
        }),
        1 => Ok(EnumVariant::Tuple {
            name: reader.string()?,
            fields: reader.collection(read_type)?,
            span: reader.span()?,
        }),
        2 => Ok(EnumVariant::Record {
            name: reader.string()?,
            fields: reader.collection(read_named_field)?,
            span: reader.span()?,
        }),
        value => Err(BytecodeFormatError::new(format!(
            "invalid enum variant tag {value}"
        ))),
    }
}

pub(super) fn write_runtime_type(writer: &mut Writer, runtime_type: &RuntimeType) -> Result<()> {
    match runtime_type {
        RuntimeType::Struct(value) => {
            writer.u8(0);
            writer.string(&value.name)?;
            writer.collection(&value.generic_parameters, write_generic_parameter)?;
            writer.collection(&value.fields, write_named_field)?;
        }
        RuntimeType::Enum(value) => {
            writer.u8(1);
            writer.string(&value.name)?;
            writer.collection(&value.generic_parameters, write_generic_parameter)?;
            writer.collection(&value.variants, write_enum_variant)?;
        }
    }
    Ok(())
}

pub(super) fn read_runtime_type(reader: &mut Reader<'_>) -> Result<RuntimeType> {
    match reader.u8()? {
        0 => Ok(RuntimeType::Struct(Rc::new(StructType {
            name: reader.string()?,
            generic_parameters: reader.collection(read_generic_parameter)?,
            fields: reader.collection(read_named_field)?,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        }))),
        1 => Ok(RuntimeType::Enum(Rc::new(EnumType {
            name: reader.string()?,
            generic_parameters: reader.collection(read_generic_parameter)?,
            variants: reader.collection(read_enum_variant)?,
            methods: RefCell::new(HashMap::new()),
            trait_methods: RefCell::new(HashMap::new()),
            implemented_traits: RefCell::new(HashSet::new()),
            associated_types: RefCell::new(HashMap::new()),
        }))),
        value => Err(BytecodeFormatError::new(format!(
            "invalid runtime type tag {value}"
        ))),
    }
}
