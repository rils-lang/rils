use super::*;

pub(super) fn split_function_name(name: &str) -> Result<(&str, &str), String> {
    let (module, function) = name
        .rsplit_once("::")
        .ok_or_else(|| format!("host function `{name}` must include a module-qualified name"))?;
    validate_module_name(module)?;
    validate_identifier(function, "host function name")?;
    Ok((module, function))
}

pub(super) fn validate_module_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > HOST_MANIFEST_MAX_NAME_BYTES
        || name.split("::").any(|segment| !is_identifier(segment))
    {
        return Err(format!("`{name}` is not a valid host module path"));
    }
    Ok(())
}

pub(super) fn validate_identifier(name: &str, label: &str) -> Result<(), String> {
    if name.len() <= HOST_MANIFEST_MAX_NAME_BYTES && is_identifier(name) {
        Ok(())
    } else {
        Err(format!("`{name}` is not a valid {label}"))
    }
}

pub(super) fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

pub(super) fn validate_enum_raw_value(
    underlying_type: IntegerType,
    raw_value: u128,
) -> Result<(), String> {
    let bits = match underlying_type {
        IntegerType::I8 | IntegerType::U8 => 8,
        IntegerType::I16 | IntegerType::U16 => 16,
        IntegerType::I32 | IntegerType::U32 => 32,
        IntegerType::I64 | IntegerType::U64 => 64,
        IntegerType::I128 | IntegerType::U128 => 128,
        IntegerType::Isize | IntegerType::Usize => {
            return Err("cannot use a platform-sized underlying integer".into());
        }
    };
    if bits < 128 && raw_value >= 1u128 << bits {
        return Err(format!(
            "raw value 0x{raw_value:x} exceeds its {bits}-bit underlying integer"
        ));
    }
    Ok(())
}

pub(super) fn validate_signature(
    signature: &FunctionSignature,
    types: &BTreeMap<String, HostTypeDeclaration>,
) -> Result<(), String> {
    let Some(parameters) = &signature.parameters else {
        return Err("host function signatures must have a fixed parameter list".into());
    };
    for parameter in parameters {
        if !is_portable_host_type(parameter, false, types) {
            return Err(format!(
                "host function parameter type `{parameter}` is not supported by the portable host contract"
            ));
        }
    }
    if !is_portable_host_type(&signature.return_type, true, types) {
        return Err(format!(
            "host function return type `{}` is not supported by the portable host contract",
            signature.return_type
        ));
    }
    Ok(())
}

pub(super) fn is_portable_host_type(
    ty: &Type,
    allow_unit: bool,
    types: &BTreeMap<String, HostTypeDeclaration>,
) -> bool {
    match ty {
        Type::Unit => allow_unit,
        Type::Bool | Type::String | Type::Char => true,
        Type::Integer(_) => true,
        Type::Float(_) => true,
        Type::Named { name, arguments } => {
            arguments.is_empty() && (name == "HostHandle" || types.contains_key(name))
        }
        _ => false,
    }
}

pub(super) fn parse_type(contract: &HostContract, name: &str) -> Result<Type, String> {
    match name {
        "()" => Ok(Type::Unit),
        "bool" => Ok(Type::Bool),
        "i8" => Ok(Type::Integer(IntegerType::I8)),
        "i16" => Ok(Type::Integer(IntegerType::I16)),
        "i32" => Ok(Type::Integer(IntegerType::I32)),
        "i64" => Ok(Type::Integer(IntegerType::I64)),
        "i128" => Ok(Type::Integer(IntegerType::I128)),
        "isize" => Ok(Type::Integer(IntegerType::Isize)),
        "u8" => Ok(Type::Integer(IntegerType::U8)),
        "u16" => Ok(Type::Integer(IntegerType::U16)),
        "u32" => Ok(Type::Integer(IntegerType::U32)),
        "u64" => Ok(Type::Integer(IntegerType::U64)),
        "u128" => Ok(Type::Integer(IntegerType::U128)),
        "usize" => Ok(Type::Integer(IntegerType::Usize)),
        "f32" => Ok(Type::Float(FloatType::F32)),
        "f64" => Ok(Type::Float(FloatType::F64)),
        "char" => Ok(Type::Char),
        "string" => Ok(Type::String),
        "HostHandle" => Ok(Type::named("HostHandle")),
        _ if contract.types.contains_key(name) => Ok(Type::named(name)),
        _ => Err(format!("unsupported host manifest type `{name}`")),
    }
}

pub(super) fn type_name(ty: &Type) -> String {
    match ty {
        Type::Unit => "()".into(),
        Type::Bool => "bool".into(),
        Type::Integer(IntegerType::I8) => "i8".into(),
        Type::Integer(IntegerType::I16) => "i16".into(),
        Type::Integer(IntegerType::I32) => "i32".into(),
        Type::Integer(IntegerType::I64) => "i64".into(),
        Type::Integer(IntegerType::I128) => "i128".into(),
        Type::Integer(IntegerType::Isize) => "isize".into(),
        Type::Integer(IntegerType::U8) => "u8".into(),
        Type::Integer(IntegerType::U16) => "u16".into(),
        Type::Integer(IntegerType::U32) => "u32".into(),
        Type::Integer(IntegerType::U64) => "u64".into(),
        Type::Integer(IntegerType::U128) => "u128".into(),
        Type::Integer(IntegerType::Usize) => "usize".into(),
        Type::Float(FloatType::F32) => "f32".into(),
        Type::Float(FloatType::F64) => "f64".into(),
        Type::Char => "char".into(),
        Type::String => "string".into(),
        Type::Named { name, arguments } if arguments.is_empty() => name.clone(),
        _ => unreachable!("host contract types were validated before serialization"),
    }
}

pub(super) fn function_overload_key(name: &str, signature: &FunctionSignature) -> String {
    format!("{name}\0{}", format_parameter_list(signature))
}

pub(super) fn format_parameter_list(signature: &FunctionSignature) -> String {
    signature
        .parameters
        .as_ref()
        .expect("validated host signatures have fixed parameters")
        .iter()
        .map(type_name)
        .collect::<Vec<_>>()
        .join(", ")
}

pub(super) fn named_type_name(ty: &Type) -> Option<&str> {
    match ty {
        Type::Named { name, arguments } if arguments.is_empty() => Some(name),
        _ => None,
    }
}

pub(super) fn fnv1a128_parts(parts: &[&[u8]]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    parts
        .iter()
        .flat_map(|part| part.iter())
        .fold(OFFSET, |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
        })
}
