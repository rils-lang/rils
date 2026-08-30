use super::*;

pub(crate) fn from_ffi_value(
    value: RilsValue,
    logical_host_type: Option<&LogicalHostType>,
) -> Result<Value, i32> {
    if value.reserved != 0 {
        return Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "reserved value fields must be zero",
            "",
            Span::default(),
        ));
    }
    let require_zero_high = || {
        if value.high == 0 {
            Ok(())
        } else {
            Err(fail(
                RILS_STATUS_INVALID_ARGUMENT,
                "high payload must be zero for this value tag",
                "",
                Span::default(),
            ))
        }
    };
    macro_rules! signed {
        ($variant:ident, $type:ty) => {{
            require_zero_high()?;
            <$type>::try_from(value.low as i64)
                .map(Value::$variant)
                .map_err(|_| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "signed integer payload is out of range",
                        "",
                        Span::default(),
                    )
                })
        }};
    }
    macro_rules! unsigned {
        ($variant:ident, $type:ty) => {{
            require_zero_high()?;
            <$type>::try_from(value.low)
                .map(Value::$variant)
                .map_err(|_| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "unsigned integer payload is out of range",
                        "",
                        Span::default(),
                    )
                })
        }};
    }
    match value.tag {
        RILS_VALUE_UNIT => Ok(Value::Unit),
        RILS_VALUE_BOOL if value.high == 0 && (value.low == 0 || value.low == 1) => {
            Ok(Value::Bool(value.low != 0))
        }
        RILS_VALUE_BOOL => Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            "bool payload must be 0 or 1",
            "",
            Span::default(),
        )),
        RILS_VALUE_I8 => signed!(I8, i8),
        RILS_VALUE_I16 => signed!(I16, i16),
        RILS_VALUE_I32 => signed!(I32, i32),
        RILS_VALUE_I64 => signed!(I64, i64),
        RILS_VALUE_I128 => Ok(Value::I128(
            ((u128::from(value.high) << 64) | u128::from(value.low)) as i128,
        )),
        RILS_VALUE_ISIZE => signed!(Isize, isize),
        RILS_VALUE_U8 => unsigned!(U8, u8),
        RILS_VALUE_U16 => unsigned!(U16, u16),
        RILS_VALUE_U32 => unsigned!(U32, u32),
        RILS_VALUE_U64 => unsigned!(U64, u64),
        RILS_VALUE_U128 => Ok(Value::U128(
            (u128::from(value.high) << 64) | u128::from(value.low),
        )),
        RILS_VALUE_USIZE => unsigned!(Usize, usize),
        RILS_VALUE_F32 => {
            require_zero_high()?;
            let bits = u32::try_from(value.low).map_err(|_| {
                fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "f32 payload is out of range",
                    "",
                    Span::default(),
                )
            })?;
            Ok(Value::F32(f32::from_bits(bits)))
        }
        RILS_VALUE_F64 => {
            require_zero_high()?;
            Ok(Value::F64(f64::from_bits(value.low)))
        }
        RILS_VALUE_CHAR => {
            require_zero_high()?;
            let scalar = u32::try_from(value.low)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| {
                    fail(
                        RILS_STATUS_INVALID_ARGUMENT,
                        "char payload is not a Unicode scalar value",
                        "",
                        Span::default(),
                    )
                })?;
            Ok(Value::Char(scalar))
        }
        RILS_VALUE_STRING => {
            require_zero_high()?;
            take_string(value.low).map(|value| Value::String(value.into()))
        }
        RILS_VALUE_HOST_HANDLE => {
            if logical_host_type
                .is_some_and(|logical| logical.transport != HostTypeTransport::HostHandle)
            {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host handle transport does not match logical type",
                    "",
                    Span::default(),
                ));
            }
            let object_id = i64::from_le_bytes(value.low.to_le_bytes());
            let generation = (value.high >> 32) as u32;
            let type_id = value.high as u32;
            if generation == 0 || type_id == 0 || object_id == 0 {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host handle payload is invalid",
                    "",
                    Span::default(),
                ));
            }
            let handle = rils_runtime::OpaqueHostHandle {
                object_id,
                generation,
                type_id,
            };
            Ok(logical_host_type.map_or_else(
                || rils_runtime::opaque_host_value(handle),
                |logical| {
                    rils_runtime::opaque_host_value_typed(
                        handle,
                        logical.name.clone(),
                        logical.base_types.clone(),
                    )
                },
            ))
        }
        RILS_VALUE_INLINE_VALUE => {
            let Some(logical) = logical_host_type else {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "inline host value requires logical type metadata",
                    "",
                    Span::default(),
                ));
            };
            if logical.transport != HostTypeTransport::InlineValue {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "inline value transport does not match logical type",
                    "",
                    Span::default(),
                ));
            }
            let Some(layout) = logical.value_layout else {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "inline host value logical type has no layout",
                    "",
                    Span::default(),
                ));
            };
            let mut bytes = [0u8; 16];
            bytes[..8].copy_from_slice(&value.low.to_le_bytes());
            bytes[8..].copy_from_slice(&value.high.to_le_bytes());
            if bytes[layout.byte_len()..].iter().any(|byte| *byte != 0) {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "inline host value has non-zero padding bytes",
                    "",
                    Span::default(),
                ));
            }
            Ok(rils_runtime::inline_host_value_typed(
                bytes,
                logical.name.clone(),
            ))
        }
        _ => Err(fail(
            RILS_STATUS_UNSUPPORTED_VALUE,
            format!("unsupported C ABI value tag {}", value.tag),
            "",
            Span::default(),
        )),
    }
}

pub(crate) fn to_ffi_value(value: Value, source_name: &str) -> Result<RilsValue, i32> {
    let scalar = |tag, low, high| RilsValue {
        tag,
        low,
        high,
        ..RilsValue::default()
    };
    let value = match value {
        Value::Unit => RilsValue::default(),
        Value::Bool(value) => scalar(RILS_VALUE_BOOL, u64::from(value), 0),
        Value::I8(value) => scalar(RILS_VALUE_I8, value as i64 as u64, 0),
        Value::I16(value) => scalar(RILS_VALUE_I16, value as i64 as u64, 0),
        Value::I32(value) => scalar(RILS_VALUE_I32, value as i64 as u64, 0),
        Value::I64(value) => scalar(RILS_VALUE_I64, value as u64, 0),
        Value::I128(value) => scalar(
            RILS_VALUE_I128,
            value as u128 as u64,
            (value as u128 >> 64) as u64,
        ),
        Value::Isize(value) => scalar(RILS_VALUE_ISIZE, value as i64 as u64, 0),
        Value::U8(value) => scalar(RILS_VALUE_U8, u64::from(value), 0),
        Value::U16(value) => scalar(RILS_VALUE_U16, u64::from(value), 0),
        Value::U32(value) => scalar(RILS_VALUE_U32, u64::from(value), 0),
        Value::U64(value) => scalar(RILS_VALUE_U64, value, 0),
        Value::U128(value) => scalar(RILS_VALUE_U128, value as u64, (value >> 64) as u64),
        Value::Usize(value) => scalar(RILS_VALUE_USIZE, value as u64, 0),
        Value::F32(value) => scalar(RILS_VALUE_F32, u64::from(value.to_bits()), 0),
        Value::F64(value) => scalar(RILS_VALUE_F64, value.to_bits(), 0),
        Value::Char(value) => scalar(RILS_VALUE_CHAR, u64::from(u32::from(value)), 0),
        Value::String(value) => scalar(RILS_VALUE_STRING, insert_string(value.to_string())?, 0),
        Value::HostObject(object) => {
            let value = Value::HostObject(object);
            if let Some(handle) = rils_runtime::opaque_host_handle(&value) {
                scalar(
                    RILS_VALUE_HOST_HANDLE,
                    u64::from_le_bytes(handle.object_id.to_le_bytes()),
                    (u64::from(handle.generation) << 32) | u64::from(handle.type_id),
                )
            } else if let Some(inline) = rils_runtime::inline_host_value(&value) {
                scalar(
                    RILS_VALUE_INLINE_VALUE,
                    u64::from_le_bytes(inline.bytes[..8].try_into().expect("fixed payload")),
                    u64::from_le_bytes(inline.bytes[8..].try_into().expect("fixed payload")),
                )
            } else {
                return Err(fail(
                    RILS_STATUS_INVALID_ARGUMENT,
                    "host object payload is not portable",
                    source_name,
                    Span::default(),
                ));
            }
        }
        other => {
            return Err(fail(
                RILS_STATUS_UNSUPPORTED_VALUE,
                format!(
                    "return type `{}` is not supported by the prototype C ABI",
                    other.type_name()
                ),
                source_name,
                Span::default(),
            ));
        }
    };
    Ok(value)
}

pub(crate) fn current_error_message() -> String {
    LAST_ERROR.with(|error| error.borrow().message.clone())
}

pub(crate) fn portable_type_from_tag(tag: u32, allow_unit: bool) -> Result<Type, String> {
    match tag {
        RILS_VALUE_UNIT if allow_unit => Ok(Type::Unit),
        RILS_VALUE_BOOL => Ok(Type::Bool),
        RILS_VALUE_I8 => Ok(Type::Integer(IntegerType::I8)),
        RILS_VALUE_I16 => Ok(Type::Integer(IntegerType::I16)),
        RILS_VALUE_I32 => Ok(Type::Integer(IntegerType::I32)),
        RILS_VALUE_I64 => Ok(Type::Integer(IntegerType::I64)),
        RILS_VALUE_I128 => Ok(Type::Integer(IntegerType::I128)),
        RILS_VALUE_ISIZE => Ok(Type::Integer(IntegerType::Isize)),
        RILS_VALUE_U8 => Ok(Type::Integer(IntegerType::U8)),
        RILS_VALUE_U16 => Ok(Type::Integer(IntegerType::U16)),
        RILS_VALUE_U32 => Ok(Type::Integer(IntegerType::U32)),
        RILS_VALUE_U64 => Ok(Type::Integer(IntegerType::U64)),
        RILS_VALUE_U128 => Ok(Type::Integer(IntegerType::U128)),
        RILS_VALUE_USIZE => Ok(Type::Integer(IntegerType::Usize)),
        RILS_VALUE_F32 => Ok(Type::Float(FloatType::F32)),
        RILS_VALUE_F64 => Ok(Type::Float(FloatType::F64)),
        RILS_VALUE_CHAR => Ok(Type::Char),
        RILS_VALUE_STRING => Ok(Type::String),
        RILS_VALUE_HOST_HANDLE => Ok(Type::named("HostHandle")),
        _ => Err(format!(
            "value tag {tag} is not supported by the portable host contract"
        )),
    }
}

pub(crate) fn logical_type_from_transport(
    contract: &HostContract,
    transport_tag: u32,
    logical_type: &str,
    allow_unit: bool,
) -> Result<Type, String> {
    if logical_type.is_empty() {
        return portable_type_from_tag(transport_tag, allow_unit);
    }
    let declaration = contract
        .host_type(logical_type)
        .ok_or_else(|| format!("host type `{logical_type}` is not declared"))?;
    let expected_tag = match declaration.transport {
        HostTypeTransport::HostHandle => RILS_VALUE_HOST_HANDLE,
        HostTypeTransport::InlineValue => RILS_VALUE_INLINE_VALUE,
        HostTypeTransport::Enum => declaration
            .enum_definition
            .as_ref()
            .map(|definition| portable_integer_tag(definition.underlying_type))
            .ok_or_else(|| format!("host enum `{logical_type}` has no enum metadata"))?,
    };
    if transport_tag != expected_tag {
        return Err(format!(
            "host type `{logical_type}` requires transport tag {expected_tag}, found {transport_tag}"
        ));
    }
    Ok(Type::named(logical_type))
}

pub(crate) fn logical_host_type(
    contract: &HostContract,
    name: &str,
) -> Result<LogicalHostType, String> {
    let declaration = contract
        .host_type(name)
        .ok_or_else(|| format!("host type `{name}` is not declared"))?;
    Ok(LogicalHostType {
        name: name.to_owned(),
        base_types: contract.type_lineage(name)?,
        transport: declaration.transport,
        value_layout: declaration.value_layout,
    })
}

pub(crate) fn portable_tag_from_type(
    contract: &HostContract,
    ty: &Type,
    allow_unit: bool,
) -> Result<u32, String> {
    match ty {
        Type::Unit if allow_unit => Ok(RILS_VALUE_UNIT),
        Type::Bool => Ok(RILS_VALUE_BOOL),
        Type::Integer(IntegerType::I8) => Ok(RILS_VALUE_I8),
        Type::Integer(IntegerType::I16) => Ok(RILS_VALUE_I16),
        Type::Integer(IntegerType::I32) => Ok(RILS_VALUE_I32),
        Type::Integer(IntegerType::I64) => Ok(RILS_VALUE_I64),
        Type::Integer(IntegerType::I128) => Ok(RILS_VALUE_I128),
        Type::Integer(IntegerType::Isize) => Ok(RILS_VALUE_ISIZE),
        Type::Integer(IntegerType::U8) => Ok(RILS_VALUE_U8),
        Type::Integer(IntegerType::U16) => Ok(RILS_VALUE_U16),
        Type::Integer(IntegerType::U32) => Ok(RILS_VALUE_U32),
        Type::Integer(IntegerType::U64) => Ok(RILS_VALUE_U64),
        Type::Integer(IntegerType::U128) => Ok(RILS_VALUE_U128),
        Type::Integer(IntegerType::Usize) => Ok(RILS_VALUE_USIZE),
        Type::Float(FloatType::F32) => Ok(RILS_VALUE_F32),
        Type::Float(FloatType::F64) => Ok(RILS_VALUE_F64),
        Type::Char => Ok(RILS_VALUE_CHAR),
        Type::String => Ok(RILS_VALUE_STRING),
        Type::Named { name, arguments } if name == "HostHandle" && arguments.is_empty() => {
            Ok(RILS_VALUE_HOST_HANDLE)
        }
        Type::Named { name, arguments } if arguments.is_empty() => contract
            .host_type(name)
            .ok_or_else(|| format!("host manifest type `{name}` is not declared"))
            .map(|declaration| match declaration.transport {
                HostTypeTransport::HostHandle => RILS_VALUE_HOST_HANDLE,
                HostTypeTransport::InlineValue => RILS_VALUE_INLINE_VALUE,
                HostTypeTransport::Enum => declaration
                    .enum_definition
                    .as_ref()
                    .map(|definition| portable_integer_tag(definition.underlying_type))
                    .expect("validated host enum declarations have metadata"),
            }),
        _ => Err(format!(
            "host manifest type `{ty}` is not supported by the current C dispatcher ABI"
        )),
    }
}

pub(crate) fn portable_integer_tag(integer: IntegerType) -> u32 {
    match integer {
        IntegerType::I8 => RILS_VALUE_I8,
        IntegerType::I16 => RILS_VALUE_I16,
        IntegerType::I32 => RILS_VALUE_I32,
        IntegerType::I64 => RILS_VALUE_I64,
        IntegerType::I128 => RILS_VALUE_I128,
        IntegerType::Isize => RILS_VALUE_ISIZE,
        IntegerType::U8 => RILS_VALUE_U8,
        IntegerType::U16 => RILS_VALUE_U16,
        IntegerType::U32 => RILS_VALUE_U32,
        IntegerType::U64 => RILS_VALUE_U64,
        IntegerType::U128 => RILS_VALUE_U128,
        IntegerType::Usize => RILS_VALUE_USIZE,
    }
}

pub(crate) fn validate_c_dispatcher_contract(contract: &HostContract) -> Result<(), String> {
    if contract.host_abi_version() != rils_runtime::BYTECODE_HOST_ABI_VERSION {
        return Err(format!(
            "host manifest ABI {} is incompatible with runtime ABI {}",
            contract.host_abi_version(),
            rils_runtime::BYTECODE_HOST_ABI_VERSION
        ));
    }
    for function in contract.functions() {
        let parameters = function
            .signature
            .parameters
            .as_ref()
            .expect("host contract signatures are fixed");
        for parameter in parameters {
            portable_tag_from_type(contract, parameter, false)?;
        }
        portable_tag_from_type(contract, &function.signature.return_type, true)?;
    }
    Ok(())
}

pub(crate) fn copy_callback_error(error: RilsSlice) -> String {
    if error.length == 0 {
        return "host dispatcher returned an error".into();
    }
    if error.data.is_null() {
        return "host dispatcher returned an invalid error slice".into();
    }
    // SAFETY: The dispatcher contract keeps this slice readable until the callback returns.
    let bytes = unsafe { slice::from_raw_parts(error.data, error.length) };
    std::str::from_utf8(bytes).map_or_else(
        |_| "host dispatcher returned a non-UTF-8 error message".into(),
        str::to_owned,
    )
}

pub(crate) fn to_ffi_host_argument(
    value: &Value,
    expected: &Type,
    contract: &HostContract,
) -> Result<RilsValue, i32> {
    if let Type::Named { name, arguments } = expected
        && arguments.is_empty()
        && let Some(definition) = contract
            .host_type(name)
            .and_then(|declaration| declaration.enum_definition.as_ref())
    {
        let raw = rils_runtime::host_enum_raw(value, name, definition)
            .map_err(|message| fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default()))?;
        return Ok(enum_raw_to_ffi(raw, definition.underlying_type));
    }
    to_ffi_value(value.clone(), "")
}

pub(crate) fn enum_raw_to_ffi(raw: u128, underlying: IntegerType) -> RilsValue {
    let (tag, low, high) = match underlying {
        IntegerType::I8 => (RILS_VALUE_I8, (raw as u8 as i8 as i64) as u64, 0),
        IntegerType::I16 => (RILS_VALUE_I16, (raw as u16 as i16 as i64) as u64, 0),
        IntegerType::I32 => (RILS_VALUE_I32, (raw as u32 as i32 as i64) as u64, 0),
        IntegerType::I64 => (RILS_VALUE_I64, raw as u64, 0),
        IntegerType::I128 => (RILS_VALUE_I128, raw as u64, (raw >> 64) as u64),
        IntegerType::Isize => (RILS_VALUE_ISIZE, (raw as usize as isize as i64) as u64, 0),
        IntegerType::U8 => (RILS_VALUE_U8, raw as u8 as u64, 0),
        IntegerType::U16 => (RILS_VALUE_U16, raw as u16 as u64, 0),
        IntegerType::U32 => (RILS_VALUE_U32, raw as u32 as u64, 0),
        IntegerType::U64 => (RILS_VALUE_U64, raw as u64, 0),
        IntegerType::U128 => (RILS_VALUE_U128, raw as u64, (raw >> 64) as u64),
        IntegerType::Usize => (RILS_VALUE_USIZE, raw as usize as u64, 0),
    };
    RilsValue {
        tag,
        reserved: 0,
        low,
        high,
    }
}

pub(crate) fn from_ffi_host_enum(
    value: RilsValue,
    type_name: &str,
    definition: &rils_runtime::HostEnumDefinition,
) -> Result<Value, i32> {
    let expected_tag = portable_integer_tag(definition.underlying_type);
    if value.tag != expected_tag {
        return Err(fail(
            RILS_STATUS_INVALID_ARGUMENT,
            format!(
                "host enum `{type_name}` requires value tag {expected_tag}, found {}",
                value.tag
            ),
            "",
            Span::default(),
        ));
    }
    let integer = from_ffi_value(value, None)?;
    let raw = match integer {
        Value::I8(value) => value as u8 as u128,
        Value::I16(value) => value as u16 as u128,
        Value::I32(value) => value as u32 as u128,
        Value::I64(value) => value as u64 as u128,
        Value::I128(value) => value as u128,
        Value::Isize(value) => value as usize as u128,
        Value::U8(value) => u128::from(value),
        Value::U16(value) => u128::from(value),
        Value::U32(value) => u128::from(value),
        Value::U64(value) => u128::from(value),
        Value::U128(value) => value,
        Value::Usize(value) => value as u128,
        _ => unreachable!("host enum transport tag was checked as an integer"),
    };
    rils_runtime::host_enum_value(type_name, definition, raw)
        .map_err(|message| fail(RILS_STATUS_INVALID_ARGUMENT, message, "", Span::default()))
}
