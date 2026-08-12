use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use rils_frontend::{FloatType, FunctionSignature, IntegerType, Type};
use serde_json::{Map, Value, json};

pub const HOST_MANIFEST_FORMAT_VERSION: u32 = 1;
pub const HOST_MANIFEST_JSON_FORMAT_VERSION: u32 = 1;
pub const HOST_CONTRACT_ABI_VERSION: u32 = 1;
pub const HOST_CONTRACT_HASH_ALGORITHM: &str = "fnv1a128";
pub const HOST_MANIFEST_MAGIC: [u8; 8] = *b"RILHOST\0";
pub const HOST_MANIFEST_HEADER_SIZE: usize = 64;
pub const HOST_MANIFEST_MAX_BYTES: usize = 256 * 1024 * 1024;
pub const HOST_MANIFEST_JSON_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const HOST_MANIFEST_MAX_MODULES: usize = 4_096;
pub const HOST_MANIFEST_MAX_FUNCTIONS: usize = 65_536;
pub const HOST_MANIFEST_MAX_PARAMETERS: usize = 1_048_576;
const HOST_MANIFEST_HASH_ALGORITHM_ID: u32 = 1;
const HOST_MANIFEST_MODULE_ENTRY_SIZE: usize = 8;
const HOST_MANIFEST_FUNCTION_ENTRY_SIZE: usize = 32;
const HOST_MANIFEST_MAX_NAME_BYTES: usize = 1_024;
const HOST_MANIFEST_MAX_CAPABILITY_BYTES: usize = 1_024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostCallKind {
    Direct,
}

impl HostCallKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Direct => "direct",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HostThreadAffinity {
    MainThread,
}

impl HostThreadAffinity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::MainThread => "main_thread",
        }
    }
}

/// Compile-time description of host functions available to a Rils module.
///
/// The contract contains declarations only. Runtime implementations are linked
/// separately through `BytecodeHost`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostContract {
    host_abi_version: u32,
    contract_version: u32,
    modules: BTreeMap<String, HostModuleDeclaration>,
    functions: BTreeMap<String, HostFunctionDeclaration>,
    function_ids: HashSet<u64>,
    parameter_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostModuleDeclaration {
    pub name: String,
    pub version: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HostFunctionDeclaration {
    pub function_id: u64,
    pub name: String,
    pub signature: FunctionSignature,
    pub capability: String,
    pub call_kind: HostCallKind,
    pub thread_affinity: HostThreadAffinity,
}

impl Default for HostContract {
    fn default() -> Self {
        Self::new()
    }
}

impl HostContract {
    pub fn new() -> Self {
        Self::with_versions(HOST_CONTRACT_ABI_VERSION, 1)
            .expect("default host contract versions are valid")
    }

    pub fn with_versions(host_abi_version: u32, contract_version: u32) -> Result<Self, String> {
        if host_abi_version == 0 || contract_version == 0 {
            return Err("host ABI and contract versions must be non-zero".into());
        }
        Ok(Self {
            host_abi_version,
            contract_version,
            modules: BTreeMap::new(),
            functions: BTreeMap::new(),
            function_ids: HashSet::new(),
            parameter_count: 0,
        })
    }

    pub const fn host_abi_version(&self) -> u32 {
        self.host_abi_version
    }

    pub const fn contract_version(&self) -> u32 {
        self.contract_version
    }

    pub fn register_module(&mut self, name: impl Into<String>, version: u32) -> Result<(), String> {
        let name = name.into();
        validate_module_name(&name)?;
        if self.modules.len() >= HOST_MANIFEST_MAX_MODULES && !self.modules.contains_key(&name) {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_MODULES} module limit"
            ));
        }
        if version == 0 {
            return Err(format!("host module `{name}` version must be non-zero"));
        }
        match self.modules.get(&name) {
            Some(module) if module.version == version => Ok(()),
            Some(module) => Err(format!(
                "host module `{name}` is already declared with version {}, not {version}",
                module.version
            )),
            None => {
                self.modules
                    .insert(name.clone(), HostModuleDeclaration { name, version });
                Ok(())
            }
        }
    }

    pub fn register_function(
        &mut self,
        function_id: u64,
        name: impl Into<String>,
        signature: FunctionSignature,
        capability: impl Into<String>,
    ) -> Result<(), String> {
        self.register_function_with_options(
            function_id,
            name,
            signature,
            capability,
            HostCallKind::Direct,
            HostThreadAffinity::MainThread,
        )
    }

    pub fn register_function_with_options(
        &mut self,
        function_id: u64,
        name: impl Into<String>,
        signature: FunctionSignature,
        capability: impl Into<String>,
        call_kind: HostCallKind,
        thread_affinity: HostThreadAffinity,
    ) -> Result<(), String> {
        let name = name.into();
        let capability = capability.into();
        let (module, function_name) = split_function_name(&name)?;
        if !self.modules.contains_key(module) {
            self.register_module(module, 1)?;
        }
        validate_identifier(function_name, "host function name")?;
        validate_signature(&signature)?;
        let parameter_count = signature
            .parameters
            .as_ref()
            .expect("validated host signatures have fixed parameters")
            .len();
        if self.parameter_count.saturating_add(parameter_count) > HOST_MANIFEST_MAX_PARAMETERS {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_PARAMETERS} parameter limit"
            ));
        }
        if function_id == 0 {
            return Err("host function id must be non-zero".into());
        }
        if capability.is_empty() {
            return Err("host function capability cannot be empty".into());
        }
        if capability.len() > HOST_MANIFEST_MAX_CAPABILITY_BYTES {
            return Err(format!(
                "host function capability exceeds {HOST_MANIFEST_MAX_CAPABILITY_BYTES} bytes"
            ));
        }
        if self.functions.len() >= HOST_MANIFEST_MAX_FUNCTIONS {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_FUNCTIONS} function limit"
            ));
        }
        if self.functions.contains_key(&name) {
            return Err(format!("host function `{name}` is already declared"));
        }
        if !self.function_ids.insert(function_id) {
            return Err(format!(
                "host function id {function_id} is already declared"
            ));
        }
        self.functions.insert(
            name.clone(),
            HostFunctionDeclaration {
                function_id,
                name,
                signature,
                capability,
                call_kind,
                thread_affinity,
            },
        );
        self.parameter_count += parameter_count;
        Ok(())
    }

    pub fn modules(&self) -> impl ExactSizeIterator<Item = &HostModuleDeclaration> {
        self.modules.values()
    }

    pub fn functions(&self) -> impl ExactSizeIterator<Item = &HostFunctionDeclaration> {
        self.functions.values()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    pub fn function(&self, name: &str) -> Option<&HostFunctionDeclaration> {
        self.functions.get(name)
    }

    /// Merges another verified manifest fragment into this logical contract.
    /// Identical declarations are idempotent; conflicting names, ids, versions,
    /// or ABI metadata are rejected independently of fragment load order.
    pub fn merge(&mut self, fragment: &Self) -> Result<(), String> {
        if self.host_abi_version != fragment.host_abi_version
            || self.contract_version != fragment.contract_version
        {
            return Err(format!(
                "host manifest versions differ: expected ABI/contract {}/{}, found {}/{}",
                self.host_abi_version,
                self.contract_version,
                fragment.host_abi_version,
                fragment.contract_version
            ));
        }
        for module in fragment.modules() {
            self.register_module(&module.name, module.version)?;
        }
        for function in fragment.functions() {
            if let Some(existing) = self.function(&function.name) {
                if existing == function {
                    continue;
                }
                return Err(format!(
                    "host function `{}` has conflicting declarations",
                    function.name
                ));
            }
            self.register_function_with_options(
                function.function_id,
                &function.name,
                function.signature.clone(),
                &function.capability,
                function.call_kind,
                function.thread_affinity,
            )?;
        }
        Ok(())
    }

    /// Returns the deterministic, non-cryptographic contract fingerprint.
    pub fn contract_hash(&self) -> String {
        let bytes = self
            .to_manifest_bytes()
            .expect("validated host contracts fit the binary manifest limits");
        let hash = u128::from_le_bytes(
            bytes[48..HOST_MANIFEST_HEADER_SIZE]
                .try_into()
                .expect("manifest hash has a fixed width"),
        );
        format!("{hash:032x}")
    }

    /// Serializes the canonical runtime host manifest.
    pub fn to_manifest_bytes(&self) -> Result<Vec<u8>, String> {
        encode_binary_manifest(self)
    }

    /// Parses and verifies a canonical runtime host manifest.
    pub fn from_manifest_bytes(bytes: &[u8]) -> Result<Self, String> {
        decode_binary_manifest(bytes)
    }

    /// Serializes a human-readable JSON manifest for explicit tooling use.
    pub fn to_manifest_json(&self) -> Result<String, String> {
        serde_json::to_string_pretty(&self.canonical_value(true))
            .map_err(|error| format!("failed to serialize host manifest: {error}"))
    }

    /// Parses a JSON manifest supplied by an editor or conversion tool.
    pub fn from_manifest_json(source: &str) -> Result<Self, String> {
        if source.len() > HOST_MANIFEST_JSON_MAX_BYTES {
            return Err(format!(
                "JSON host manifest exceeds the {HOST_MANIFEST_JSON_MAX_BYTES} byte limit"
            ));
        }
        let value: Value = serde_json::from_str(source)
            .map_err(|error| format!("invalid host manifest JSON: {error}"))?;
        let root = expect_object(&value, "host manifest")?;
        ensure_keys(
            root,
            &[
                "format_version",
                "host_abi_version",
                "contract_version",
                "hash_algorithm",
                "contract_hash",
                "modules",
            ],
            "host manifest",
        )?;
        let format_version = required_u32(root, "format_version", "host manifest")?;
        if format_version != HOST_MANIFEST_JSON_FORMAT_VERSION {
            return Err(format!(
                "unsupported host manifest format version {format_version}"
            ));
        }
        let host_abi_version = required_u32(root, "host_abi_version", "host manifest")?;
        let contract_version = required_u32(root, "contract_version", "host manifest")?;
        if host_abi_version == 0 || contract_version == 0 {
            return Err("host ABI and contract versions must be non-zero".into());
        }
        let mut contract = Self::with_versions(host_abi_version, contract_version)?;
        let modules = required_array(root, "modules", "host manifest")?;
        if modules.len() > HOST_MANIFEST_MAX_MODULES {
            return Err(format!(
                "host manifest exceeds the {HOST_MANIFEST_MAX_MODULES} module limit"
            ));
        }
        for module in modules {
            parse_module(&mut contract, module)?;
        }
        match (root.get("hash_algorithm"), root.get("contract_hash")) {
            (None, None) => {}
            (Some(algorithm), Some(expected)) => {
                let algorithm = algorithm
                    .as_str()
                    .ok_or_else(|| "host manifest `hash_algorithm` must be a string".to_string())?;
                if algorithm != HOST_CONTRACT_HASH_ALGORITHM {
                    return Err(format!(
                        "unsupported host contract hash algorithm `{algorithm}`"
                    ));
                }
                let expected = expected
                    .as_str()
                    .ok_or_else(|| "host manifest `contract_hash` must be a string".to_string())?;
                let actual = contract.contract_hash();
                if expected != actual {
                    return Err(format!(
                        "host contract hash mismatch: manifest has `{expected}`, computed `{actual}`"
                    ));
                }
            }
            _ => {
                return Err(
                    "host manifest `hash_algorithm` and `contract_hash` must appear together"
                        .into(),
                );
            }
        }
        Ok(contract)
    }

    pub(crate) fn signatures(&self) -> HashMap<String, FunctionSignature> {
        self.functions
            .iter()
            .map(|(name, function)| (name.clone(), function.signature.clone()))
            .collect()
    }

    fn canonical_value(&self, include_hash: bool) -> Value {
        let modules = self
            .modules
            .values()
            .map(|module| {
                let functions = self
                    .functions
                    .values()
                    .filter(|function| {
                        split_function_name(&function.name)
                            .is_ok_and(|(name, _)| name == module.name)
                    })
                    .map(|function| {
                        let (_, name) = split_function_name(&function.name)
                            .expect("registered function names are valid");
                        json!({
                            "id": format!("0x{:016x}", function.function_id),
                            "name": name,
                            "parameters": function.signature.parameters.as_ref().expect("host signatures are fixed").iter().map(type_name).collect::<Vec<_>>(),
                            "return": type_name(&function.signature.return_type),
                            "capability": function.capability,
                            "call_kind": function.call_kind.as_str(),
                            "thread_affinity": function.thread_affinity.as_str(),
                        })
                    })
                    .collect::<Vec<_>>();
                json!({
                    "name": module.name,
                    "version": module.version,
                    "functions": functions,
                })
            })
            .collect::<Vec<_>>();
        let mut value = json!({
            "format_version": HOST_MANIFEST_JSON_FORMAT_VERSION,
            "host_abi_version": self.host_abi_version,
            "contract_version": self.contract_version,
            "hash_algorithm": HOST_CONTRACT_HASH_ALGORITHM,
            "modules": modules,
        });
        if include_hash {
            value
                .as_object_mut()
                .expect("manifest root is an object")
                .insert("contract_hash".into(), Value::String(self.contract_hash()));
        }
        value
    }
}

fn encode_binary_manifest(contract: &HostContract) -> Result<Vec<u8>, String> {
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
        payload.push(0);
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
    push_u32(&mut manifest, HOST_MANIFEST_FORMAT_VERSION);
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

fn decode_binary_manifest(bytes: &[u8]) -> Result<HostContract, String> {
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
    if format_version != HOST_MANIFEST_FORMAT_VERSION {
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
        if payload.read_u8()? != 0 {
            return Err("binary host manifest function reserved byte must be zero".into());
        }
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
        contract.register_function_with_options(
            function.function_id,
            function.name,
            FunctionSignature::fixed(parameters, function.return_type),
            function.capability,
            function.call_kind,
            function.thread_affinity,
        )?;
    }
    if used_strings.iter().any(|used| !used) {
        return Err("binary host manifest contains unused strings".into());
    }
    Ok(contract)
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
}

struct BinaryReader<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> BinaryReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, position: 0 }
    }

    fn remaining(&self) -> usize {
        self.bytes.len() - self.position
    }

    fn read_exact(&mut self, length: usize) -> Result<&'a [u8], String> {
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

    fn read_u8(&mut self) -> Result<u8, String> {
        Ok(self.read_exact(1)?[0])
    }

    fn read_u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(
            self.read_exact(4)?
                .try_into()
                .expect("u32 has a fixed width"),
        ))
    }

    fn read_u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(
            self.read_exact(8)?
                .try_into()
                .expect("u64 has a fixed width"),
        ))
    }
}

fn indexed_string<'a>(strings: &'a [String], index: usize, label: &str) -> Result<&'a str, String> {
    strings
        .get(index)
        .map(String::as_str)
        .ok_or_else(|| format!("binary host manifest {label} string index {index} is invalid"))
}

fn push_u32(bytes: &mut Vec<u8>, value: u32) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_le_bytes());
}

fn type_tag(ty: &Type) -> Result<u8, String> {
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
        _ => Err(format!(
            "host type `{ty}` cannot be encoded in binary manifest v1"
        )),
    }
}

fn decode_type_tag(tag: u8, allow_unit: bool) -> Result<Type, String> {
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
        value => Err(format!("unsupported binary host type tag {value}")),
    }
}

fn parse_module(contract: &mut HostContract, value: &Value) -> Result<(), String> {
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

fn parse_function(
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
                .and_then(parse_type)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let return_type = parse_type(required_string(function, "return", "host function")?)?;
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
    contract.register_function_with_options(
        function_id,
        format!("{module_name}::{name}"),
        FunctionSignature::fixed(parameters, return_type),
        capability,
        call_kind,
        thread_affinity,
    )
}

fn expect_object<'a>(value: &'a Value, label: &str) -> Result<&'a Map<String, Value>, String> {
    value
        .as_object()
        .ok_or_else(|| format!("{label} must be a JSON object"))
}

fn ensure_keys(object: &Map<String, Value>, allowed: &[&str], label: &str) -> Result<(), String> {
    if let Some(key) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("unknown {label} field `{key}`"));
    }
    Ok(())
}

fn required_value<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a Value, String> {
    object
        .get(key)
        .ok_or_else(|| format!("{label} is missing `{key}`"))
}

fn required_string<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a str, String> {
    required_value(object, key, label)?
        .as_str()
        .ok_or_else(|| format!("{label} `{key}` must be a string"))
}

fn required_u64(object: &Map<String, Value>, key: &str, label: &str) -> Result<u64, String> {
    required_value(object, key, label)?
        .as_u64()
        .ok_or_else(|| format!("{label} `{key}` must be an unsigned integer"))
}

fn required_u32(object: &Map<String, Value>, key: &str, label: &str) -> Result<u32, String> {
    u32::try_from(required_u64(object, key, label)?)
        .map_err(|_| format!("{label} `{key}` exceeds u32"))
}

fn required_function_id(object: &Map<String, Value>) -> Result<u64, String> {
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

fn required_array<'a>(
    object: &'a Map<String, Value>,
    key: &str,
    label: &str,
) -> Result<&'a [Value], String> {
    required_value(object, key, label)?
        .as_array()
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{label} `{key}` must be an array"))
}

fn split_function_name(name: &str) -> Result<(&str, &str), String> {
    let (module, function) = name
        .rsplit_once("::")
        .ok_or_else(|| format!("host function `{name}` must include a module-qualified name"))?;
    validate_module_name(module)?;
    validate_identifier(function, "host function name")?;
    Ok((module, function))
}

fn validate_module_name(name: &str) -> Result<(), String> {
    if name.is_empty()
        || name.len() > HOST_MANIFEST_MAX_NAME_BYTES
        || name.split("::").any(|segment| !is_identifier(segment))
    {
        return Err(format!("`{name}` is not a valid host module path"));
    }
    Ok(())
}

fn validate_identifier(name: &str, label: &str) -> Result<(), String> {
    if name.len() <= HOST_MANIFEST_MAX_NAME_BYTES && is_identifier(name) {
        Ok(())
    } else {
        Err(format!("`{name}` is not a valid {label}"))
    }
}

fn is_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    chars
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_alphanumeric())
}

fn validate_signature(signature: &FunctionSignature) -> Result<(), String> {
    let Some(parameters) = &signature.parameters else {
        return Err("host function signatures must have a fixed parameter list".into());
    };
    for parameter in parameters {
        if !is_portable_host_type(parameter, false) {
            return Err(format!(
                "host function parameter type `{parameter}` is not supported by the portable host contract"
            ));
        }
    }
    if !is_portable_host_type(&signature.return_type, true) {
        return Err(format!(
            "host function return type `{}` is not supported by the portable host contract",
            signature.return_type
        ));
    }
    Ok(())
}

fn is_portable_host_type(ty: &Type, allow_unit: bool) -> bool {
    match ty {
        Type::Unit => allow_unit,
        Type::Bool | Type::String => true,
        Type::Integer(
            IntegerType::I32 | IntegerType::I64 | IntegerType::U32 | IntegerType::U64,
        ) => true,
        Type::Float(_) => true,
        _ => false,
    }
}

fn parse_type(name: &str) -> Result<Type, String> {
    match name {
        "()" => Ok(Type::Unit),
        "bool" => Ok(Type::Bool),
        "i32" => Ok(Type::Integer(IntegerType::I32)),
        "i64" => Ok(Type::Integer(IntegerType::I64)),
        "u32" => Ok(Type::Integer(IntegerType::U32)),
        "u64" => Ok(Type::Integer(IntegerType::U64)),
        "f32" => Ok(Type::Float(FloatType::F32)),
        "f64" => Ok(Type::Float(FloatType::F64)),
        "string" => Ok(Type::String),
        _ => Err(format!("unsupported host manifest type `{name}`")),
    }
}

fn type_name(ty: &Type) -> &'static str {
    match ty {
        Type::Unit => "()",
        Type::Bool => "bool",
        Type::Integer(IntegerType::I32) => "i32",
        Type::Integer(IntegerType::I64) => "i64",
        Type::Integer(IntegerType::U32) => "u32",
        Type::Integer(IntegerType::U64) => "u64",
        Type::Float(FloatType::F32) => "f32",
        Type::Float(FloatType::F64) => "f64",
        Type::String => "string",
        _ => unreachable!("host contract types were validated before serialization"),
    }
}

fn fnv1a128_parts(parts: &[&[u8]]) -> u128 {
    const OFFSET: u128 = 0x6c62_272e_07bb_0142_62b8_2175_6295_c58d;
    const PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013b;
    parts
        .iter()
        .flat_map(|part| part.iter())
        .fold(OFFSET, |hash, byte| {
            (hash ^ u128::from(*byte)).wrapping_mul(PRIME)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example_contract() -> HostContract {
        let mut contract = HostContract::new();
        contract.register_module("unity_engine::time", 2).unwrap();
        contract
            .register_function(
                7,
                "unity_engine::time::frame_count",
                FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
                "unity.time",
            )
            .unwrap();
        contract
    }

    #[test]
    fn contract_rejects_duplicate_names_ids_and_non_portable_types() {
        let mut contract = example_contract();
        assert!(
            contract
                .register_function(
                    8,
                    "unity_engine::time::frame_count",
                    FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
                    "unity.time",
                )
                .unwrap_err()
                .contains("already declared")
        );
        assert!(
            contract
                .register_function(
                    7,
                    "unity_engine::time::delta_time",
                    FunctionSignature::fixed(Vec::new(), Type::Float(FloatType::F32)),
                    "unity.time",
                )
                .unwrap_err()
                .contains("id 7")
        );
        assert!(
            HostContract::new()
                .register_function(
                    1,
                    "unity_engine::bad",
                    FunctionSignature::fixed(vec![Type::USIZE], Type::Unit),
                    "unity",
                )
                .unwrap_err()
                .contains("not supported")
        );
    }

    #[test]
    fn manifest_round_trips_canonically_and_verifies_hash() {
        let contract = example_contract();
        let json = contract.to_manifest_json().unwrap();
        let parsed = HostContract::from_manifest_json(&json).unwrap();
        assert_eq!(parsed, contract);
        assert_eq!(parsed.to_manifest_json().unwrap(), json);
        assert_eq!(parsed.contract_hash().len(), 32);
        assert!(json.contains("\"id\": \"0x0000000000000007\""));

        let corrupted = json.replace("frame_count", "fixed_count");
        assert!(
            HostContract::from_manifest_json(&corrupted)
                .unwrap_err()
                .contains("hash mismatch")
        );
    }

    #[test]
    fn binary_manifest_round_trips_canonically_and_rejects_corruption() {
        let contract = example_contract();
        let manifest = contract.to_manifest_bytes().unwrap();
        assert_eq!(&manifest[..8], &HOST_MANIFEST_MAGIC);
        assert_eq!(
            HostContract::from_manifest_bytes(&manifest).unwrap(),
            contract
        );
        assert_eq!(
            HostContract::from_manifest_bytes(&manifest)
                .unwrap()
                .to_manifest_bytes()
                .unwrap(),
            manifest
        );

        let mut corrupted = manifest.clone();
        *corrupted.last_mut().unwrap() ^= 1;
        assert!(
            HostContract::from_manifest_bytes(&corrupted)
                .unwrap_err()
                .contains("hash mismatch")
        );
        assert!(
            HostContract::from_manifest_bytes(&manifest[..manifest.len() - 1])
                .unwrap_err()
                .contains("length mismatch")
        );
    }

    #[test]
    fn binary_manifest_verifier_rejects_unknown_type_after_valid_hash() {
        let mut contract = HostContract::new();
        contract
            .register_function(
                2,
                "unity_engine::math::abs",
                FunctionSignature::fixed(
                    vec![Type::Integer(IntegerType::I32)],
                    Type::Integer(IntegerType::I32),
                ),
                "unity.math",
            )
            .unwrap();
        let mut manifest = contract.to_manifest_bytes().unwrap();
        *manifest.last_mut().unwrap() = 0xff;
        let hash = fnv1a128_parts(&[&manifest[..48], &manifest[HOST_MANIFEST_HEADER_SIZE..]]);
        manifest[48..HOST_MANIFEST_HEADER_SIZE].copy_from_slice(&hash.to_le_bytes());
        assert!(
            HostContract::from_manifest_bytes(&manifest)
                .unwrap_err()
                .contains("type tag 255")
        );
    }

    #[test]
    fn manifest_hash_is_independent_of_registration_order() {
        let mut left = HostContract::new();
        left.register_function(
            2,
            "unity_engine::debug::enabled",
            FunctionSignature::fixed(Vec::new(), Type::Bool),
            "unity.debug",
        )
        .unwrap();
        left.register_function(
            1,
            "unity_engine::debug::flush",
            FunctionSignature::fixed(Vec::new(), Type::Unit),
            "unity.debug",
        )
        .unwrap();

        let mut right = HostContract::new();
        right
            .register_function(
                1,
                "unity_engine::debug::flush",
                FunctionSignature::fixed(Vec::new(), Type::Unit),
                "unity.debug",
            )
            .unwrap();
        right
            .register_function(
                2,
                "unity_engine::debug::enabled",
                FunctionSignature::fixed(Vec::new(), Type::Bool),
                "unity.debug",
            )
            .unwrap();

        assert_eq!(left.contract_hash(), right.contract_hash());
        assert_eq!(
            left.to_manifest_bytes().unwrap(),
            right.to_manifest_bytes().unwrap()
        );
        assert_eq!(
            left.to_manifest_json().unwrap(),
            right.to_manifest_json().unwrap()
        );
    }

    #[test]
    fn fragments_merge_canonically_and_reject_conflicts() {
        let mut first = HostContract::new();
        first
            .register_function(
                1,
                "unity_engine::time::frame_count",
                FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::U64)),
                "unity.time",
            )
            .unwrap();
        let mut second = HostContract::new();
        second
            .register_function(
                2,
                "game::score::get",
                FunctionSignature::fixed(Vec::new(), Type::Integer(IntegerType::I32)),
                "game.score",
            )
            .unwrap();

        let mut left = first.clone();
        left.merge(&second).unwrap();
        let mut right = second.clone();
        right.merge(&first).unwrap();
        assert_eq!(
            left.to_manifest_bytes().unwrap(),
            right.to_manifest_bytes().unwrap()
        );
        left.merge(&first).unwrap();

        let mut conflicting = HostContract::new();
        conflicting
            .register_function(
                2,
                "game::score::other",
                FunctionSignature::fixed(Vec::new(), Type::Unit),
                "game.score",
            )
            .unwrap();
        assert!(second.merge(&conflicting).unwrap_err().contains("id 2"));
    }

    #[test]
    fn manifest_rejects_unknown_fields_and_future_call_kinds() {
        let json = example_contract().to_manifest_json().unwrap();
        let unknown = json.replace("\"version\": 2", "\"version\": 2, \"typo\": true");
        assert!(
            HostContract::from_manifest_json(&unknown)
                .unwrap_err()
                .contains("unknown host module field")
        );
        let command = json.replace("\"direct\"", "\"command\"");
        assert!(
            HostContract::from_manifest_json(&command)
                .unwrap_err()
                .contains("unsupported host call kind")
        );
    }

    #[test]
    fn bundled_manifest_example_is_valid() {
        let manifest = include_str!("../../../examples/unity-host-manifest.json");
        let contract = HostContract::from_manifest_json(manifest).unwrap();
        assert_eq!(contract.functions().len(), 1);
        assert_eq!(contract.functions().next().unwrap().function_id, 100);
        assert!(
            contract
                .to_manifest_json()
                .unwrap()
                .contains("contract_hash")
        );
    }
}
