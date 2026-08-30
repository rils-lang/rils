use super::*;

pub(super) fn parse_type_declaration(
    contract: &mut HostContract,
    value: &Value,
    format_version: u32,
) -> Result<(), String> {
    let declaration = expect_object(value, "host type")?;
    ensure_keys(
        declaration,
        &[
            "name",
            "kind",
            "base",
            "layout",
            "transport",
            "flags",
            "variants",
        ],
        "host type",
    )?;
    let name = required_string(declaration, "name", "host type")?;
    let kind = required_string(declaration, "kind", "host type")?;
    let base_type = declaration
        .get("base")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "host type `base` must be a string".to_string())
        })
        .transpose()?;
    match kind {
        "opaque" => {
            if declaration.contains_key("layout") {
                return Err("opaque host type cannot declare `layout`".into());
            }
            let transport = match required_string(declaration, "transport", "host type")? {
                "HostHandle" => HostTypeTransport::HostHandle,
                other => return Err(format!("unsupported opaque host type transport `{other}`")),
            };
            contract.register_type(name, base_type, transport)
        }
        "value" if format_version >= HOST_MANIFEST_V3_JSON_FORMAT_VERSION => {
            if base_type.is_some() {
                return Err("inline host value cannot declare `base`".into());
            }
            if required_string(declaration, "transport", "host type")? != "InlineValue" {
                return Err("value host type must use `InlineValue` transport".into());
            }
            let layout =
                HostValueLayout::parse(required_string(declaration, "layout", "host type")?)?;
            contract.register_value_type(name, layout)
        }
        "value" => Err("host manifest v2 cannot declare inline value types".into()),
        "enum" if format_version >= HOST_MANIFEST_JSON_FORMAT_VERSION => {
            if base_type.is_some() || declaration.contains_key("layout") {
                return Err("host enum type cannot declare `base` or `layout`".into());
            }
            let underlying_type = IntegerType::from_name(required_string(
                declaration,
                "transport",
                "host enum type",
            )?)
            .ok_or_else(|| "host enum transport must be an integer type".to_string())?;
            let flags = required_value(declaration, "flags", "host enum type")?
                .as_bool()
                .ok_or_else(|| "host enum type `flags` must be a boolean".to_string())?;
            let variants = required_array(declaration, "variants", "host enum type")?
                .iter()
                .map(|value| {
                    let variant = expect_object(value, "host enum variant")?;
                    ensure_keys(variant, &["name", "raw"], "host enum variant")?;
                    let name = required_string(variant, "name", "host enum variant")?.to_owned();
                    let raw =
                        parse_hex_u128(required_string(variant, "raw", "host enum variant")?)?;
                    Ok((name, raw))
                })
                .collect::<Result<Vec<_>, String>>()?;
            contract.register_enum_type(name, underlying_type, flags, variants)
        }
        "enum" => Err("host manifest versions before v5 cannot declare enum types".into()),
        other => Err(format!("unsupported host type kind `{other}`")),
    }
}

pub(super) fn parse_module(contract: &mut HostContract, value: &Value) -> Result<(), String> {
    let module = expect_object(value, "host module")?;
    ensure_keys(module, &["name", "version", "functions"], "host module")?;
    let name = required_string(module, "name", "host module")?;
    let version = required_u32(module, "version", "host module")?;
    if contract.modules.contains_key(name) {
        return Err(format!("host module `{name}` is declared more than once"));
    }
    contract.register_module(name, version)?;
    let functions = required_array(module, "functions", "host module")?;
    if contract.functions.len().saturating_add(functions.len()) > HOST_MANIFEST_MAX_FUNCTIONS {
        return Err(format!(
            "host manifest exceeds the {HOST_MANIFEST_MAX_FUNCTIONS} function limit"
        ));
    }
    for function in functions {
        parse_function(contract, name, function)?;
    }
    Ok(())
}

pub(super) fn parse_function(
    contract: &mut HostContract,
    module_name: &str,
    value: &Value,
) -> Result<(), String> {
    let function = expect_object(value, "host function")?;
    ensure_keys(
        function,
        &[
            "id",
            "name",
            "parameters",
            "return",
            "capability",
            "call_kind",
            "thread_affinity",
            "receiver",
        ],
        "host function",
    )?;
    let function_id = required_function_id(function)?;
    let name = required_string(function, "name", "host function")?;
    validate_identifier(name, "host function name")?;
    let parameters = required_array(function, "parameters", "host function")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "host function parameter types must be strings".to_string())
                .and_then(|name| parse_type(contract, name))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_type = parse_type(
        contract,
        required_string(function, "return", "host function")?,
    )?;
    let capability = required_string(function, "capability", "host function")?;
    let call_kind = match required_string(function, "call_kind", "host function")? {
        "direct" => HostCallKind::Direct,
        other => {
            return Err(format!(
                "unsupported host call kind `{other}` in manifest v1"
            ));
        }
    };
    let thread_affinity = match required_string(function, "thread_affinity", "host function")? {
        "main_thread" => HostThreadAffinity::MainThread,
        other => {
            return Err(format!(
                "unsupported host thread affinity `{other}` in manifest v1"
            ));
        }
    };
    let receiver = function
        .get("receiver")
        .map(|value| {
            value
                .as_str()
                .ok_or_else(|| "host function receiver must be a string".to_string())
                .and_then(|receiver| match receiver {
                    "self" => Ok(HostReceiver::Value),
                    "&self" => Ok(HostReceiver::Ref),
                    "&mut self" => Ok(HostReceiver::RefMut),
                    other => Err(format!("unsupported host function receiver `{other}`")),
                })
        })
        .transpose()?;
    contract.register_function_with_options_and_receiver(
        function_id,
        format!("{module_name}::{name}"),
        FunctionSignature::fixed(parameters, return_type),
        capability,
        call_kind,
        thread_affinity,
        receiver,
    )
}

pub(super) fn expect_object<'a>(
    value: &'a Value,
    label: &str,
) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be a JSON object"))
}

pub(super) fn ensure_keys(
    object: &Map<String, Value>,
    allowed: &[&str],
    label: &str,
) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unknown {label} field `{key}`"));
    }
    Ok(())
}

pub(super) fn required_value<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is missing `{key}`"))
}

pub(super) fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a str, String> {
    required_value(object, key, label)?
        .as_str()
        .ok_or_else(|| format!("{label} `{key}` must be a string"))
}

pub(super) fn required_u64(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u64, String> {
    required_value(object, key, label)?
        .as_u64()
        .ok_or_else(|| format!("{label} `{key}` must be an unsigned integer"))
}

pub(super) fn required_u32(
    object: &Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<u32, String> {
    u32::try_from(required_u64(object, key, label)?)
        .map_err(|_| format!("{label} `{key}` exceeds u32"))
}

pub(super) fn required_function_id(object: &Map<String, Value>) -> Result<u64, String> {
    let value = required_string(object, "id", "host function")?;
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| "host function `id` must use a `0x` hexadecimal string".to_string())?;
    if digits.is_empty()
        || digits.len() > 16
        || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("host function `id` must contain 1 to 16 hexadecimal digits".into());
    }
    u64::from_str_radix(digits, 16)
        .map_err(|_| "host function `id` is outside the u64 range".to_string())
}

pub(super) fn parse_hex_u128(value: &str) -> Result<u128, String> {
    let digits = value
        .strip_prefix("0x")
        .ok_or_else(|| "host enum raw value must use a `0x` hexadecimal string".to_string())?;
    if digits.is_empty()
        || digits.len() > 32
        || !digits.bytes().all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("host enum raw value must contain 1 to 32 hexadecimal digits".into());
    }
    u128::from_str_radix(digits, 16)
        .map_err(|_| "host enum raw value is outside the u128 range".to_string())
}

pub(super) fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a [Value], String> {
    required_value(object, key, label)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{label} `{key}` must be an array"))
}
