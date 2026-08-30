use super::*;

pub(super) fn encode_binary_manifest(contract: &HostContract) -> Result<Vec<u8>, String> {
    let mut string_set = BTreeSet::new();
    for module in contract.modules.values() {
        string_set.insert(module.name.as_str());
    }
    for function in contract.functions.values() {
        string_set.insert(function.name.as_str());
        string_set.insert(function.capability.as_str());
    }
    let strings = string_set.into_iter().collect::<Vec<_>>();
    let string_indices = strings
        .iter()
        .enumerate()
        .map(|(index, value)| (*value, index as u32))
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
    for module in contract.modules.values() {
        push_u32(&mut payload, string_indices[module.name.as_str()]);
        push_u32(&mut payload, module.version);
    }

    let mut parameter_types = Vec::with_capacity(contract.parameter_count);
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
            u32::try_from(parameter_types.len())
                .map_err(|_| "host manifest parameter table is too large")?,
        );
        push_u32(
            &mut payload,
            u32::try_from(parameters.len()).map_err(|_| "host function has too many parameters")?,
        );
        payload.push(type_tag(&function.signature.return_type)?);
        payload.push(match function.call_kind {
            HostCallKind::Direct => 0,
        });
        payload.push(match function.thread_affinity {
            HostThreadAffinity::MainThread => 0,
        });
        payload.push(function.receiver.map_or(0, HostReceiver::as_tag));
        for parameter in parameters {
            parameter_types.push(type_tag(parameter)?);
        }
    }
    payload.extend_from_slice(&parameter_types);

    if payload.len().saturating_add(HOST_MANIFEST_HEADER_SIZE) > HOST_MANIFEST_MAX_BYTES {
        return Err(format!(
            "binary host manifest exceeds the {HOST_MANIFEST_MAX_BYTES} byte limit"
        ));
    }
    let payload_len = u32::try_from(payload.len())
        .map_err(|_| "binary host manifest payload exceeds the u32 format limit")?;
    let mut manifest = Vec::with_capacity(HOST_MANIFEST_HEADER_SIZE + payload.len());
    manifest.extend_from_slice(&HOST_MANIFEST_MAGIC);
    push_u32(&mut manifest, HOST_MANIFEST_LEGACY_FORMAT_VERSION);
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

pub(super) fn decode_binary_manifest(bytes: &[u8]) -> Result<HostContract, String> {
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
    if format_version != HOST_MANIFEST_LEGACY_FORMAT_VERSION {
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
    if string_count > module_count.saturating_add(function_count.saturating_mul(2)) {
        return Err("binary host manifest string count exceeds the canonical maximum".into());
    }
    let minimum_payload_len = module_count
        .checked_mul(HOST_MANIFEST_MODULE_ENTRY_SIZE)
        .and_then(|size| {
            function_count
                .checked_mul(HOST_MANIFEST_FUNCTION_ENTRY_SIZE)
                .and_then(|function_size| size.checked_add(function_size))
        })
        .and_then(|size| size.checked_add(parameter_count))
        .ok_or_else(|| "binary host manifest table size overflow".to_string())?;
    if payload_len < minimum_payload_len {
        return Err("binary host manifest payload is too short for its declared tables".into());
    }
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
    let mut strings = Vec::with_capacity(string_count);
    for _ in 0..string_count {
        let length = payload.read_u32()? as usize;
        if length == 0
            || length > HOST_MANIFEST_MAX_NAME_BYTES.max(HOST_MANIFEST_MAX_CAPABILITY_BYTES)
        {
            return Err("binary host manifest contains an invalid string length".into());
        }
        let value = std::str::from_utf8(payload.read_exact(length)?)
            .map_err(|error| format!("binary host manifest contains invalid UTF-8: {error}"))?
            .to_owned();
        if strings.last().is_some_and(|previous| previous >= &value) {
            return Err(
                "binary host manifest strings must be unique and lexicographically sorted".into(),
            );
        }
        strings.push(value);
    }
    let mut used_strings = vec![false; string_count];
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

    let mut raw_functions = Vec::with_capacity(function_count);
    let mut next_parameter = 0usize;
    for _ in 0..function_count {
        let function_id = payload.read_u64()?;
        let name_index = payload.read_u32()? as usize;
        let module_index = payload.read_u32()? as usize;
        let capability_index = payload.read_u32()? as usize;
        let parameter_start = payload.read_u32()? as usize;
        let function_parameter_count = payload.read_u32()? as usize;
        let return_type = decode_type_tag(payload.read_u8()?, true)?;
        let call_kind = match payload.read_u8()? {
            0 => HostCallKind::Direct,
            value => return Err(format!("unsupported binary host call kind {value}")),
        };
        let thread_affinity = match payload.read_u8()? {
            0 => HostThreadAffinity::MainThread,
            value => {
                return Err(format!("unsupported binary host thread affinity {value}"));
            }
        };
        let receiver = HostReceiver::from_tag(payload.read_u8()?)?;
        let name = indexed_string(&strings, name_index, "function name")?;
        let module = module_names.get(module_index).ok_or_else(|| {
            format!("binary host function module index {module_index} is invalid")
        })?;
        let capability = indexed_string(&strings, capability_index, "function capability")?;
        used_strings[name_index] = true;
        used_strings[capability_index] = true;
        if raw_functions
            .last()
            .is_some_and(|previous: &RawBinaryFunction| previous.name.as_str() >= name)
        {
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
        raw_functions.push(RawBinaryFunction {
            function_id,
            name: name.to_owned(),
            capability: capability.to_owned(),
            parameter_start,
            parameter_count: function_parameter_count,
            return_type,
            call_kind,
            thread_affinity,
            receiver,
        });
    }
    if next_parameter != parameter_count {
        return Err("binary host manifest parameter count does not match function ranges".into());
    }
    if payload.remaining() != parameter_count {
        return Err(format!(
            "binary host manifest parameter table has {} bytes, expected {parameter_count}",
            payload.remaining()
        ));
    }
    let parameter_tags = payload.read_exact(parameter_count)?;
    for function in raw_functions {
        let end = function.parameter_start + function.parameter_count;
        let parameters = parameter_tags[function.parameter_start..end]
            .iter()
            .map(|tag| decode_type_tag(*tag, false))
            .collect::<Result<Vec<_>, _>>()?;
        contract.register_function_with_options_and_receiver(
            function.function_id,
            function.name,
            FunctionSignature::fixed(parameters, function.return_type),
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

pub(super) fn legacy_contract_hash(contract: &HostContract) -> Result<String, String> {
    let bytes = encode_binary_manifest(contract)?;
    manifest_hash(&bytes)
}

pub(super) fn manifest_hash(bytes: &[u8]) -> Result<String, String> {
    if bytes.len() < HOST_MANIFEST_HEADER_SIZE {
        return Err("binary host manifest is shorter than its fixed header".into());
    }
    let hash = u128::from_le_bytes(
        bytes[48..HOST_MANIFEST_HEADER_SIZE]
            .try_into()
            .expect("manifest hash has a fixed width"),
    );
    Ok(format!("{hash:032x}"))
}

struct RawBinaryFunction {
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

pub(super) struct BinaryReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BinaryReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    pub(super) fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    pub(super) fn read_exact(&mut self, length: usize) -> Result<&'a [u8], String> {
        let end = self
            .position
            .checked_add(length)
            .ok_or_else(|| "binary host manifest offset overflow".to_string())?;
        let value = self
            .bytes
            .get(self.position..end)
            .ok_or_else(|| "binary host manifest is truncated".to_string())?;
        self.position = end;
        Ok(value)
    }

    pub(super) fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    pub(super) fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.read_exact(4)?
                .try_into()
                .expect("u32 has a fixed width"),
        ))
    }

    pub(super) fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.read_exact(8)?
                .try_into()
                .expect("u64 has a fixed width"),
        ))
    }
}

pub(super) fn indexed_string<'a>(
    strings: &'a [String],
    index: usize,
    label: &str,
) -> Result<&'a str, String> {
    strings
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("binary host manifest {label} string index {index} is invalid"))
}

pub(super) fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

pub(super) fn type_tag(ty: &Type) -> Result<u8, String> {
    match ty {
        Type::Unit => Ok(0),
        Type::Bool => Ok(1),
        Type::Integer(IntegerType::I32) => Ok(2),
        Type::Integer(IntegerType::U32) => Ok(3),
        Type::Integer(IntegerType::I64) => Ok(4),
        Type::Integer(IntegerType::U64) => Ok(5),
        Type::Float(FloatType::F32) => Ok(6),
        Type::Float(FloatType::F64) => Ok(7),
        Type::String => Ok(8),
        Type::Named { name, arguments } if name == "HostHandle" && arguments.is_empty() => Ok(9),
        _ => Err(format!(
            "host type `{ty}` cannot be encoded in binary manifest v1"
        )),
    }
}

pub(super) fn decode_type_tag(tag: u8, allow_unit: bool) -> Result<Type, String> {
    match tag {
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
        value => Err(format!("unsupported binary host type tag {value}")),
    }
}
