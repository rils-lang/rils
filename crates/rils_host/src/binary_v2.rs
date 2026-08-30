use std::collections::{BTreeSet, HashMap};

use rils_syntax::{FloatType, FunctionSignature, IntegerType, Type};

use super::*;

const TYPE_ENTRY_SIZE: usize = 12;
const FUNCTION_ENTRY_SIZE: usize = 36;
const ENUM_VARIANT_ENTRY_SIZE: usize = 24;
const NO_STRING_INDEX: u32 = u32::MAX;
const NAMED_TYPE_BIT: u32 = 0x8000_0000;

pub(super) fn encode(contract: &HostContract) -> Result<Vec<u8>, String> {
    encode_version(contract, HOST_MANIFEST_FORMAT_VERSION)
}

pub(super) fn encode_legacy_v2(contract: &HostContract) -> Result<Vec<u8>, String> {
    if contract
        .types
        .values()
        .any(|declaration| declaration.value_layout.is_some())
    {
        return Err("host manifest v2 cannot encode inline value types".into());
    }
    encode_version(contract, HOST_MANIFEST_V2_FORMAT_VERSION)
}

pub(super) fn encode_legacy_v3(contract: &HostContract) -> Result<Vec<u8>, String> {
    encode_version(contract, HOST_MANIFEST_V3_FORMAT_VERSION)
}

pub(super) fn encode_legacy_v4(contract: &HostContract) -> Result<Vec<u8>, String> {
    if contract
        .types
        .values()
        .any(|declaration| declaration.enum_definition.is_some())
    {
        return Err("host manifest v4 cannot encode enum types".into());
    }
    encode_version(contract, HOST_MANIFEST_V4_FORMAT_VERSION)
}

fn encode_version(contract: &HostContract, format_version: u32) -> Result<Vec<u8>, String> {
    if format_version < HOST_MANIFEST_FORMAT_VERSION
        && contract
            .types
            .values()
            .any(|declaration| declaration.enum_definition.is_some())
    {
        return Err(format!(
            "host manifest v{format_version} cannot encode enum types"
        ));
    }
    let mut string_set = BTreeSet::<String>::new();
    for declaration in contract.types.values() {
        string_set.insert(declaration.name.clone());
        if let Some(base_type) = declaration.base_type.as_deref() {
            string_set.insert(base_type.to_owned());
        }
        if let Some(layout) = declaration.value_layout {
            string_set.insert(layout.canonical_name());
        }
        if let Some(enum_definition) = declaration.enum_definition.as_ref() {
            string_set.extend(enum_definition.variants.keys().cloned());
        }
    }
    for module in contract.modules.values() {
        string_set.insert(module.name.clone());
    }
    for function in contract.functions.values() {
        string_set.insert(function.name.clone());
        string_set.insert(function.capability.clone());
    }
    let strings = string_set.into_iter().collect::<Vec<_>>();
    let string_indices = strings
        .iter()
        .enumerate()
        .map(|(index, value)| (value.as_str(), index as u32))
        .collect::<HashMap<_, _>>();
    let type_indices = contract
        .types
        .keys()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index as u32))
        .collect::<HashMap<_, _>>();
    let module_indices = contract
        .modules
        .keys()
        .enumerate()
        .map(|(index, name)| (name.as_str(), index as u32))
        .collect::<HashMap<_, _>>();

    let mut payload = Vec::new();
    for value in &strings {
        push_u32(
            &mut payload,
            u32::try_from(value.len()).map_err(|_| "host manifest string is too long")?,
        );
        payload.extend_from_slice(value.as_bytes());
    }
    push_u32(&mut payload, contract.types.len() as u32);
    for declaration in contract.types.values() {
        push_u32(&mut payload, string_indices[declaration.name.as_str()]);
        let layout_name = declaration
            .value_layout
            .map(HostValueLayout::canonical_name);
        let relation = declaration.base_type.as_deref().or(layout_name.as_deref());
        push_u32(
            &mut payload,
            relation.map_or(NO_STRING_INDEX, |value| string_indices[value]),
        );
        if let Some(enum_definition) = declaration.enum_definition.as_ref() {
            payload.push(encode_integer_type(enum_definition.underlying_type));
            payload.push(2);
            payload.extend_from_slice(&u16::from(enum_definition.flags).to_le_bytes());
        } else {
            payload.push(declaration.transport.as_tag());
            payload.push(u8::from(declaration.value_layout.is_some()));
            payload.extend_from_slice(&0u16.to_le_bytes());
        }
    }
    for module in contract.modules.values() {
        push_u32(&mut payload, string_indices[module.name.as_str()]);
        push_u32(&mut payload, module.version);
    }

    let mut parameter_types = Vec::with_capacity(contract.parameter_count * 4);
    for function in contract.functions.values() {
        let (module, _) = split_function_name(&function.name)?;
        let parameters = function
            .signature
            .parameters
            .as_ref()
            .expect("registered host signatures have fixed parameters");
        push_u64(&mut payload, function.function_id);
        push_u32(&mut payload, string_indices[function.name.as_str()]);
        push_u32(&mut payload, module_indices[module]);
        push_u32(&mut payload, string_indices[function.capability.as_str()]);
        push_u32(
            &mut payload,
            u32::try_from(parameter_types.len() / 4)
                .map_err(|_| "host manifest parameter table is too large")?,
        );
        push_u32(
            &mut payload,
            u32::try_from(parameters.len()).map_err(|_| "host function has too many parameters")?,
        );
        push_u32(
            &mut payload,
            encode_type_ref(
                &function.signature.return_type,
                &type_indices,
                true,
                format_version,
            )?,
        );
        payload.push(match function.call_kind {
            HostCallKind::Direct => 0,
        });
        payload.push(match function.thread_affinity {
            HostThreadAffinity::MainThread => 0,
        });
        payload.push(function.receiver.map_or(0, HostReceiver::as_tag));
        payload.push(0);
        for parameter in parameters {
            push_u32(
                &mut parameter_types,
                encode_type_ref(parameter, &type_indices, false, format_version)?,
            );
        }
    }
    payload.extend_from_slice(&parameter_types);
    if format_version >= HOST_MANIFEST_FORMAT_VERSION {
        let enum_variant_count = contract
            .types
            .values()
            .filter_map(|declaration| declaration.enum_definition.as_ref())
            .map(|definition| definition.variants.len())
            .sum::<usize>();
        push_u32(
            &mut payload,
            u32::try_from(enum_variant_count)
                .map_err(|_| "host enum variant table exceeds the u32 format limit")?,
        );
        for (type_index, declaration) in contract.types.values().enumerate() {
            let Some(enum_definition) = declaration.enum_definition.as_ref() else {
                continue;
            };
            for (name, raw) in &enum_definition.variants {
                push_u32(&mut payload, type_index as u32);
                push_u32(&mut payload, string_indices[name.as_str()]);
                push_u64(&mut payload, *raw as u64);
                push_u64(&mut payload, (*raw >> 64) as u64);
            }
        }
    }

    if payload.len().saturating_add(HOST_MANIFEST_HEADER_SIZE) > HOST_MANIFEST_MAX_BYTES {
        return Err(format!(
            "binary host manifest exceeds the {HOST_MANIFEST_MAX_BYTES} byte limit"
        ));
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| "binary host manifest payload exceeds the u32 format limit")?;
    let mut manifest = Vec::with_capacity(HOST_MANIFEST_HEADER_SIZE + payload.len());
    manifest.extend_from_slice(&HOST_MANIFEST_MAGIC);
    push_u32(&mut manifest, format_version);
    push_u32(&mut manifest, HOST_MANIFEST_HEADER_SIZE as u32);
    push_u32(&mut manifest, contract.host_abi_version);
    push_u32(&mut manifest, contract.contract_version);
    push_u32(&mut manifest, contract.modules.len() as u32);
    push_u32(&mut manifest, contract.functions.len() as u32);
    push_u32(&mut manifest, strings.len() as u32);
    push_u32(&mut manifest, contract.parameter_count as u32);
    push_u32(&mut manifest, payload_len);
    push_u32(&mut manifest, HOST_MANIFEST_HASH_ALGORITHM_ID);
    debug_assert_eq!(manifest.len(), 48);
    let hash = fnv1a128_parts(&[&manifest, &payload]);
    manifest.extend_from_slice(&hash.to_le_bytes());
    manifest.extend_from_slice(&payload);
    Ok(manifest)
}

pub(super) fn decode(bytes: &[u8]) -> Result<HostContract, String> {
    if bytes.len() > HOST_MANIFEST_MAX_BYTES {
        return Err(format!(
            "binary host manifest exceeds the {HOST_MANIFEST_MAX_BYTES} byte limit"
        ));
    }
    if bytes.len() < HOST_MANIFEST_HEADER_SIZE {
        return Err("binary host manifest is shorter than its fixed header".into());
    }
    let mut header = BinaryReader::new(bytes);
    if header.read_exact(8)? != HOST_MANIFEST_MAGIC {
        return Err("invalid binary host manifest magic".into());
    }
    let format_version = header.read_u32()?;
    if format_version != HOST_MANIFEST_V2_FORMAT_VERSION
        && format_version != HOST_MANIFEST_V3_FORMAT_VERSION
        && format_version != HOST_MANIFEST_V4_FORMAT_VERSION
        && format_version != HOST_MANIFEST_FORMAT_VERSION
    {
        return Err(format!(
            "unsupported binary host manifest format version {format_version}"
        ));
    }
    let header_size = header.read_u32()? as usize;
    if header_size != HOST_MANIFEST_HEADER_SIZE {
        return Err(format!(
            "unsupported binary host manifest header size {header_size}"
        ));
    }
    let host_abi_version = header.read_u32()?;
    let contract_version = header.read_u32()?;
    if host_abi_version == 0 || contract_version == 0 {
        return Err("host ABI and contract versions must be non-zero".into());
    }
    let module_count = header.read_u32()? as usize;
    let function_count = header.read_u32()? as usize;
    let string_count = header.read_u32()? as usize;
    let parameter_count = header.read_u32()? as usize;
    let payload_len = header.read_u32()? as usize;
    let hash_algorithm = header.read_u32()?;
    validate_header_counts(module_count, function_count, parameter_count)?;
    if hash_algorithm != HOST_MANIFEST_HASH_ALGORITHM_ID {
        return Err(format!(
            "unsupported binary host manifest hash algorithm {hash_algorithm}"
        ));
    }
    let expected_hash = u128::from_le_bytes(
        header
            .read_exact(16)?
            .try_into()
            .expect("manifest hash has a fixed width"),
    );
    let expected_len = HOST_MANIFEST_HEADER_SIZE
        .checked_add(payload_len)
        .ok_or_else(|| "binary host manifest length overflow".to_string())?;
    if bytes.len() != expected_len {
        return Err(format!(
            "binary host manifest length mismatch: header declares {expected_len} bytes, input has {}",
            bytes.len()
        ));
    }
    let actual_hash = fnv1a128_parts(&[&bytes[..48], &bytes[HOST_MANIFEST_HEADER_SIZE..]]);
    if expected_hash != actual_hash {
        return Err(format!(
            "host contract hash mismatch: manifest has `{expected_hash:032x}`, computed `{actual_hash:032x}`"
        ));
    }

    let mut payload = BinaryReader::new(&bytes[HOST_MANIFEST_HEADER_SIZE..]);
    let strings = read_strings(&mut payload, string_count)?;
    let type_count = payload.read_u32()? as usize;
    if type_count > HOST_MANIFEST_MAX_TYPES {
        return Err(format!(
            "host manifest exceeds the {HOST_MANIFEST_MAX_TYPES} type limit"
        ));
    }
    if string_count
        > module_count
            .saturating_add(function_count.saturating_mul(2))
            .saturating_add(type_count.saturating_mul(2))
            .saturating_add(if format_version >= HOST_MANIFEST_FORMAT_VERSION {
                HOST_MANIFEST_MAX_ENUM_VARIANTS
            } else {
                0
            })
    {
        return Err("binary host manifest string count exceeds the canonical maximum".into());
    }
    let minimum_table_len = type_count
        .checked_mul(TYPE_ENTRY_SIZE)
        .and_then(|size| {
            size.checked_add(module_count.checked_mul(HOST_MANIFEST_MODULE_ENTRY_SIZE)?)
        })
        .and_then(|size| size.checked_add(function_count.checked_mul(FUNCTION_ENTRY_SIZE)?))
        .and_then(|size| size.checked_add(parameter_count.checked_mul(4)?))
        .and_then(|size| {
            size.checked_add(usize::from(format_version >= HOST_MANIFEST_FORMAT_VERSION) * 4)
        })
        .ok_or_else(|| "binary host manifest table size overflow".to_string())?;
    if payload.remaining() < minimum_table_len {
        return Err("binary host manifest payload is too short for its declared tables".into());
    }

    let mut used_strings = vec![false; string_count];
    let mut raw_types = Vec::with_capacity(type_count);
    for _ in 0..type_count {
        let name_index = payload.read_u32()? as usize;
        let base_index = payload.read_u32()?;
        let transport_tag = payload.read_u8()?;
        let kind = payload.read_u8()?;
        let reserved = u16::from_le_bytes(
            payload
                .read_exact(2)?
                .try_into()
                .expect("u16 has a fixed width"),
        );
        if kind > u8::from(format_version >= HOST_MANIFEST_FORMAT_VERSION) + 1
            || (kind != 2 && reserved != 0)
            || (kind == 2 && reserved & !1 != 0)
            || (format_version == HOST_MANIFEST_V2_FORMAT_VERSION && kind != 0)
        {
            return Err("binary host type contains unsupported kind or reserved flags".into());
        }
        let name = indexed_string(&strings, name_index, "type name")?.to_owned();
        used_strings[name_index] = true;
        let relation = if base_index == NO_STRING_INDEX {
            None
        } else {
            let index = base_index as usize;
            let base = indexed_string(&strings, index, "base type")?.to_owned();
            used_strings[index] = true;
            Some(base)
        };
        let (base_type, value_layout, enum_definition, transport) = match kind {
            0 => (
                relation,
                None,
                None,
                HostTypeTransport::from_tag(transport_tag)?,
            ),
            1 => {
                let layout = relation
                    .as_deref()
                    .ok_or_else(|| "binary inline host value is missing its layout".to_string())
                    .and_then(HostValueLayout::parse)?;
                (
                    None,
                    Some(layout),
                    None,
                    HostTypeTransport::from_tag(transport_tag)?,
                )
            }
            2 => {
                if relation.is_some() {
                    return Err("binary host enum cannot declare a type relation".into());
                }
                (
                    None,
                    None,
                    Some(HostEnumDefinition {
                        underlying_type: decode_integer_type(transport_tag)?,
                        flags: reserved & 1 != 0,
                        variants: Default::default(),
                    }),
                    HostTypeTransport::Enum,
                )
            }
            _ => unreachable!("host type kind was validated"),
        };
        if raw_types
            .last()
            .is_some_and(|previous: &HostTypeDeclaration| previous.name.as_str() >= name.as_str())
        {
            return Err("binary host manifest types must be lexicographically sorted".into());
        }
        raw_types.push(HostTypeDeclaration {
            name,
            base_type,
            transport,
            value_layout,
            enum_definition,
        });
    }

    let mut contract = HostContract::with_versions(host_abi_version, contract_version)?;
    let mut module_names: Vec<String> = Vec::with_capacity(module_count);
    for _ in 0..module_count {
        let name_index = payload.read_u32()? as usize;
        let version = payload.read_u32()?;
        let name = indexed_string(&strings, name_index, "module name")?;
        used_strings[name_index] = true;
        if module_names
            .last()
            .is_some_and(|previous| previous.as_str() >= name)
        {
            return Err("binary host manifest modules must be lexicographically sorted".into());
        }
        contract.register_module(name, version)?;
        module_names.push(name.to_owned());
    }

    let type_names = raw_types
        .iter()
        .map(|declaration| declaration.name.clone())
        .collect::<Vec<_>>();
    let type_name_refs = type_names.iter().map(String::as_str).collect::<Vec<_>>();
    let mut raw_functions = Vec::with_capacity(function_count);
    let mut next_parameter = 0usize;
    for _ in 0..function_count {
        let function_id = payload.read_u64()?;
        let name_index = payload.read_u32()? as usize;
        let module_index = payload.read_u32()? as usize;
        let capability_index = payload.read_u32()? as usize;
        let parameter_start = payload.read_u32()? as usize;
        let function_parameter_count = payload.read_u32()? as usize;
        let return_type_ref = payload.read_u32()?;
        let call_kind = match payload.read_u8()? {
            0 => HostCallKind::Direct,
            value => return Err(format!("unsupported binary host call kind {value}")),
        };
        let thread_affinity = match payload.read_u8()? {
            0 => HostThreadAffinity::MainThread,
            value => return Err(format!("unsupported binary host thread affinity {value}")),
        };
        let receiver = HostReceiver::from_tag(payload.read_u8()?)?;
        if payload.read_u8()? != 0 {
            return Err("binary host function reserved byte must be zero".into());
        }
        let name = indexed_string(&strings, name_index, "function name")?;
        let module = module_names.get(module_index).ok_or_else(|| {
            format!("binary host function module index {module_index} is invalid")
        })?;
        let capability = indexed_string(&strings, capability_index, "function capability")?;
        used_strings[name_index] = true;
        used_strings[capability_index] = true;
        if raw_functions.last().is_some_and(|previous: &RawFunction| {
            previous.name.as_str() > name
                || (format_version < HOST_MANIFEST_V4_FORMAT_VERSION
                    && previous.name.as_str() == name)
        }) {
            return Err("binary host manifest functions must be lexicographically sorted".into());
        }
        if split_function_name(name)?.0 != module {
            return Err(format!(
                "binary host function `{name}` does not belong to module `{module}`"
            ));
        }
        if parameter_start != next_parameter {
            return Err("binary host manifest parameter ranges must be contiguous".into());
        }
        next_parameter = next_parameter
            .checked_add(function_parameter_count)
            .ok_or_else(|| "binary host manifest parameter range overflow".to_string())?;
        if next_parameter > parameter_count {
            return Err("binary host manifest parameter range exceeds its table".into());
        }
        raw_functions.push(RawFunction {
            function_id,
            name: name.to_owned(),
            capability: capability.to_owned(),
            parameter_start,
            parameter_count: function_parameter_count,
            return_type: decode_type_ref(return_type_ref, &type_name_refs, true, format_version)?,
            call_kind,
            thread_affinity,
            receiver,
        });
    }
    if next_parameter != parameter_count {
        return Err("binary host manifest parameter count does not match function ranges".into());
    }
    let enum_extension_bytes = usize::from(format_version >= HOST_MANIFEST_FORMAT_VERSION) * 4;
    if payload.remaining()
        < parameter_count
            .saturating_mul(4)
            .saturating_add(enum_extension_bytes)
    {
        return Err(format!(
            "binary host manifest parameter table has {} bytes, expected {}",
            payload.remaining(),
            parameter_count
                .saturating_mul(4)
                .saturating_add(enum_extension_bytes)
        ));
    }
    let parameter_refs = (0..parameter_count)
        .map(|_| payload.read_u32())
        .collect::<Result<Vec<_>, _>>()?;
    if format_version >= HOST_MANIFEST_FORMAT_VERSION {
        let enum_variant_count = payload.read_u32()? as usize;
        if enum_variant_count > HOST_MANIFEST_MAX_ENUM_VARIANTS {
            return Err(format!(
                "host manifest exceeds the {HOST_MANIFEST_MAX_ENUM_VARIANTS} enum variant limit"
            ));
        }
        let expected_bytes = enum_variant_count
            .checked_mul(ENUM_VARIANT_ENTRY_SIZE)
            .ok_or_else(|| "binary host enum variant table size overflow".to_string())?;
        if payload.remaining() != expected_bytes {
            return Err(format!(
                "binary host enum variant table has {} bytes, expected {expected_bytes}",
                payload.remaining()
            ));
        }
        let mut previous: Option<(usize, String)> = None;
        for _ in 0..enum_variant_count {
            let type_index = payload.read_u32()? as usize;
            let name_index = payload.read_u32()? as usize;
            let low = payload.read_u64()?;
            let high = payload.read_u64()?;
            let name = indexed_string(&strings, name_index, "enum variant name")?.to_owned();
            used_strings[name_index] = true;
            let declaration = raw_types.get_mut(type_index).ok_or_else(|| {
                format!("binary host enum variant type index {type_index} is invalid")
            })?;
            let definition = declaration.enum_definition.as_mut().ok_or_else(|| {
                format!(
                    "binary host enum variant references non-enum type `{}`",
                    declaration.name
                )
            })?;
            if previous
                .as_ref()
                .is_some_and(|(previous_type, previous_name)| {
                    *previous_type > type_index
                        || (*previous_type == type_index && previous_name.as_str() >= name.as_str())
                })
            {
                return Err("binary host enum variants must be canonically sorted".into());
            }
            previous = Some((type_index, name.clone()));
            let raw = u128::from(low) | (u128::from(high) << 64);
            validate_enum_raw_value(definition.underlying_type, raw)?;
            if definition.variants.insert(name, raw).is_some() {
                return Err("binary host enum variant is duplicated".into());
            }
        }
    } else if payload.remaining() != 0 {
        return Err("binary host manifest contains trailing payload bytes".into());
    }

    for declaration in &raw_types {
        if let Some(definition) = declaration.enum_definition.as_ref() {
            contract.register_enum_type(
                &declaration.name,
                definition.underlying_type,
                definition.flags,
                definition
                    .variants
                    .iter()
                    .map(|(name, raw)| (name.clone(), *raw)),
            )?;
        } else if let Some(layout) = declaration.value_layout {
            contract.register_value_type(&declaration.name, layout)?;
        } else {
            contract.register_type(
                &declaration.name,
                declaration.base_type.as_deref(),
                declaration.transport,
            )?;
        }
    }
    validate_type_graph(&contract.types)?;
    let mut previous_overload_key: Option<String> = None;
    for function in raw_functions {
        let end = function.parameter_start + function.parameter_count;
        let parameters = parameter_refs[function.parameter_start..end]
            .iter()
            .map(|reference| decode_type_ref(*reference, &type_name_refs, false, format_version))
            .collect::<Result<Vec<_>, _>>()?;
        let signature = FunctionSignature::fixed(parameters, function.return_type);
        let overload_key = function_overload_key(&function.name, &signature);
        if previous_overload_key
            .as_ref()
            .is_some_and(|previous| previous >= &overload_key)
        {
            return Err("binary host manifest overloads must be canonically sorted".into());
        }
        previous_overload_key = Some(overload_key);
        contract.register_function_with_options_and_receiver(
            function.function_id,
            function.name,
            signature,
            function.capability,
            function.call_kind,
            function.thread_affinity,
            function.receiver,
        )?;
    }
    if used_strings.iter().any(|used| !used) {
        return Err("binary host manifest contains unused strings".into());
    }
    Ok(contract)
}

fn validate_header_counts(
    module_count: usize,
    function_count: usize,
    parameter_count: usize,
) -> Result<(), String> {
    if module_count > HOST_MANIFEST_MAX_MODULES {
        return Err(format!(
            "host manifest exceeds the {HOST_MANIFEST_MAX_MODULES} module limit"
        ));
    }
    if function_count > HOST_MANIFEST_MAX_FUNCTIONS {
        return Err(format!(
            "host manifest exceeds the {HOST_MANIFEST_MAX_FUNCTIONS} function limit"
        ));
    }
    if parameter_count > HOST_MANIFEST_MAX_PARAMETERS {
        return Err(format!(
            "host manifest exceeds the {HOST_MANIFEST_MAX_PARAMETERS} parameter limit"
        ));
    }
    Ok(())
}

fn read_strings(reader: &mut BinaryReader<'_>, count: usize) -> Result<Vec<String>, String> {
    let mut strings = Vec::with_capacity(count);
    for _ in 0..count {
        let length = reader.read_u32()? as usize;
        if length == 0
            || length > HOST_MANIFEST_MAX_NAME_BYTES.max(HOST_MANIFEST_MAX_CAPABILITY_BYTES)
        {
            return Err("binary host manifest contains an invalid string length".into());
        }
        let value = std::str::from_utf8(reader.read_exact(length)?)
            .map_err(|error| format!("binary host manifest contains invalid UTF-8: {error}"))?
            .to_owned();
        if strings.last().is_some_and(|previous| previous >= &value) {
            return Err(
                "binary host manifest strings must be unique and lexicographically sorted".into(),
            );
        }
        strings.push(value);
    }
    Ok(strings)
}

fn encode_type_ref(
    ty: &Type,
    type_indices: &HashMap<&str, u32>,
    allow_unit: bool,
    format_version: u32,
) -> Result<u32, String> {
    let primitive = match ty {
        Type::Unit if allow_unit => Some(0),
        Type::Unit => return Err("unit is not valid as a host function parameter type".into()),
        Type::Bool => Some(1),
        Type::Integer(IntegerType::I32) => Some(2),
        Type::Integer(IntegerType::U32) => Some(3),
        Type::Integer(IntegerType::I64) => Some(4),
        Type::Integer(IntegerType::U64) => Some(5),
        Type::Float(FloatType::F32) => Some(6),
        Type::Float(FloatType::F64) => Some(7),
        Type::String => Some(8),
        Type::Named { name, arguments } if name == "HostHandle" && arguments.is_empty() => Some(9),
        Type::Integer(IntegerType::I8) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(10)
        }
        Type::Integer(IntegerType::I16) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(11)
        }
        Type::Integer(IntegerType::I128) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(12)
        }
        Type::Integer(IntegerType::Isize) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(13)
        }
        Type::Integer(IntegerType::U8) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(14)
        }
        Type::Integer(IntegerType::U16) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(15)
        }
        Type::Integer(IntegerType::U128) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(16)
        }
        Type::Integer(IntegerType::Usize) if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Some(17)
        }
        Type::Char if format_version >= HOST_MANIFEST_FORMAT_VERSION => Some(18),
        Type::Named { name, arguments } if arguments.is_empty() => {
            let index = *type_indices
                .get(name.as_str())
                .ok_or_else(|| format!("host type `{name}` is not declared"))?;
            return Ok(NAMED_TYPE_BIT | index);
        }
        _ => None,
    };
    primitive.ok_or_else(|| format!("host type `{ty}` cannot be encoded in binary manifest v2"))
}

fn decode_type_ref(
    reference: u32,
    type_names: &[&str],
    allow_unit: bool,
    format_version: u32,
) -> Result<Type, String> {
    if reference & NAMED_TYPE_BIT != 0 {
        let index = (reference & !NAMED_TYPE_BIT) as usize;
        let name = type_names
            .get(index)
            .ok_or_else(|| format!("binary host type reference index {index} is invalid"))?;
        return Ok(Type::named(*name));
    }
    match reference {
        0 if allow_unit => Ok(Type::Unit),
        0 => Err("unit is not valid as a host function parameter type".into()),
        1 => Ok(Type::Bool),
        2 => Ok(Type::Integer(IntegerType::I32)),
        3 => Ok(Type::Integer(IntegerType::U32)),
        4 => Ok(Type::Integer(IntegerType::I64)),
        5 => Ok(Type::Integer(IntegerType::U64)),
        6 => Ok(Type::Float(FloatType::F32)),
        7 => Ok(Type::Float(FloatType::F64)),
        8 => Ok(Type::String),
        9 => Ok(Type::named("HostHandle")),
        10 if format_version >= HOST_MANIFEST_FORMAT_VERSION => Ok(Type::Integer(IntegerType::I8)),
        11 if format_version >= HOST_MANIFEST_FORMAT_VERSION => Ok(Type::Integer(IntegerType::I16)),
        12 if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Ok(Type::Integer(IntegerType::I128))
        }
        13 if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Ok(Type::Integer(IntegerType::Isize))
        }
        14 if format_version >= HOST_MANIFEST_FORMAT_VERSION => Ok(Type::Integer(IntegerType::U8)),
        15 if format_version >= HOST_MANIFEST_FORMAT_VERSION => Ok(Type::Integer(IntegerType::U16)),
        16 if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Ok(Type::Integer(IntegerType::U128))
        }
        17 if format_version >= HOST_MANIFEST_FORMAT_VERSION => {
            Ok(Type::Integer(IntegerType::Usize))
        }
        18 if format_version >= HOST_MANIFEST_FORMAT_VERSION => Ok(Type::Char),
        value => Err(format!("unsupported binary host type reference {value}")),
    }
}

fn encode_integer_type(value: IntegerType) -> u8 {
    match value {
        IntegerType::I8 => 1,
        IntegerType::I16 => 2,
        IntegerType::I32 => 3,
        IntegerType::I64 => 4,
        IntegerType::I128 => 5,
        IntegerType::Isize => 6,
        IntegerType::U8 => 7,
        IntegerType::U16 => 8,
        IntegerType::U32 => 9,
        IntegerType::U64 => 10,
        IntegerType::U128 => 11,
        IntegerType::Usize => 12,
    }
}

fn decode_integer_type(value: u8) -> Result<IntegerType, String> {
    match value {
        1 => Ok(IntegerType::I8),
        2 => Ok(IntegerType::I16),
        3 => Ok(IntegerType::I32),
        4 => Ok(IntegerType::I64),
        5 => Ok(IntegerType::I128),
        6 => Ok(IntegerType::Isize),
        7 => Ok(IntegerType::U8),
        8 => Ok(IntegerType::U16),
        9 => Ok(IntegerType::U32),
        10 => Ok(IntegerType::U64),
        11 => Ok(IntegerType::U128),
        12 => Ok(IntegerType::Usize),
        other => Err(format!(
            "unsupported host enum integer transport tag {other}"
        )),
    }
}

struct RawFunction {
    function_id: u64,
    name: String,
    capability: String,
    parameter_start: usize,
    parameter_count: usize,
    return_type: Type,
    call_kind: HostCallKind,
    thread_affinity: HostThreadAffinity,
    receiver: Option<HostReceiver>,
}
