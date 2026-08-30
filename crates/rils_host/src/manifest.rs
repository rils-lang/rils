use super::*;

impl HostContract {
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
        validate_type_graph(&self.types)?;
        binary_v2::encode(self)
    }

    /// Parses and verifies a canonical runtime host manifest.
    pub fn from_manifest_bytes(bytes: &[u8]) -> Result<Self, String> {
        let version = bytes
            .get(8..12)
            .and_then(|value| value.try_into().ok())
            .map(u32::from_le_bytes)
            .ok_or_else(|| "binary host manifest is shorter than its fixed header".to_string())?;
        match version {
            HOST_MANIFEST_LEGACY_FORMAT_VERSION => decode_binary_manifest(bytes),
            HOST_MANIFEST_V2_FORMAT_VERSION
            | HOST_MANIFEST_V3_FORMAT_VERSION
            | HOST_MANIFEST_V4_FORMAT_VERSION
            | HOST_MANIFEST_FORMAT_VERSION => binary_v2::decode(bytes),
            _ => Err(format!(
                "unsupported binary host manifest format version {version}"
            )),
        }
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
                "types",
                "modules",
            ],
            "host manifest",
        )?;
        let format_version = required_u32(root, "format_version", "host manifest")?;
        if format_version != HOST_MANIFEST_JSON_FORMAT_VERSION
            && format_version != HOST_MANIFEST_V4_JSON_FORMAT_VERSION
            && format_version != HOST_MANIFEST_V3_JSON_FORMAT_VERSION
            && format_version != HOST_MANIFEST_V2_JSON_FORMAT_VERSION
            && format_version != HOST_MANIFEST_LEGACY_JSON_FORMAT_VERSION
        {
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
        if format_version >= HOST_MANIFEST_V2_JSON_FORMAT_VERSION {
            let types = required_array(root, "types", "host manifest")?;
            if types.len() > HOST_MANIFEST_MAX_TYPES {
                return Err(format!(
                    "host manifest exceeds the {HOST_MANIFEST_MAX_TYPES} type limit"
                ));
            }
            for declaration in types {
                parse_type_declaration(&mut contract, declaration, format_version)?;
            }
            validate_type_graph(&contract.types)?;
        } else if root.contains_key("types") {
            return Err("host manifest v1 cannot declare named host types".into());
        }
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
                let actual = match format_version {
                    HOST_MANIFEST_LEGACY_JSON_FORMAT_VERSION => legacy_contract_hash(&contract)?,
                    HOST_MANIFEST_V2_JSON_FORMAT_VERSION => {
                        manifest_hash(&binary_v2::encode_legacy_v2(&contract)?)?
                    }
                    HOST_MANIFEST_V3_JSON_FORMAT_VERSION => {
                        manifest_hash(&binary_v2::encode_legacy_v3(&contract)?)?
                    }
                    HOST_MANIFEST_V4_JSON_FORMAT_VERSION => {
                        manifest_hash(&binary_v2::encode_legacy_v4(&contract)?)?
                    }
                    _ => contract.contract_hash(),
                };
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

    pub fn signatures(&self) -> HashMap<String, FunctionSignature> {
        let mut grouped = HashMap::<String, Vec<FunctionSignature>>::new();
        for function in self.functions.values() {
            grouped
                .entry(function.name.clone())
                .or_default()
                .push(function.signature.clone());
        }
        for function in self.functions.values() {
            if function.receiver.is_some()
                && let Some((_, method)) = function.name.rsplit_once("::")
                && let Some(receiver_type) = function
                    .signature
                    .parameters
                    .as_ref()
                    .and_then(|parameters| parameters.first())
                    .and_then(named_type_name)
            {
                grouped
                    .entry(format!("{receiver_type}::{method}"))
                    .or_default()
                    .push(function.signature.clone());
                for declaration in self.types.values() {
                    if declaration.name != receiver_type
                        && is_assignable(&self.types, receiver_type, &declaration.name)
                    {
                        grouped
                            .entry(format!("{}::{method}", declaration.name))
                            .or_default()
                            .push(function.signature.clone());
                    }
                }
            }
        }
        grouped
            .into_iter()
            .map(|(name, overloads)| {
                // Receiver methods may already use the canonical
                // `Type::member` path. In that case the direct declaration and
                // the receiver index describe the same callable; do not turn
                // that duplicate into a fake overload with an unknown
                // signature.
                let mut unique = Vec::new();
                for overload in overloads {
                    if !unique.contains(&overload) {
                        unique.push(overload);
                    }
                }
                let signature = if unique.len() == 1 {
                    unique.into_iter().next().expect("one signature")
                } else {
                    let common_return = unique
                        .first()
                        .map(|signature| signature.return_type.clone())
                        .filter(|return_type| {
                            unique
                                .iter()
                                .all(|signature| signature.return_type == *return_type)
                        })
                        .unwrap_or(Type::Unknown);
                    // The frontend signature table cannot represent overload sets yet.
                    // Keep the call variadic so HIR remains responsible for selecting
                    // the exact parameter list, but preserve a return type shared by
                    // every overload so chained calls and inferred locals stay typed.
                    FunctionSignature::variadic(common_return)
                };
                (name, signature)
            })
            .collect()
    }

    #[doc(hidden)]
    pub fn method_function_overloads(&self) -> HashMap<String, Vec<HostFunctionDeclaration>> {
        let mut methods = HashMap::<String, Vec<HostFunctionDeclaration>>::new();
        for function in self.functions.values() {
            if function.receiver.is_some()
                && let Some((_, method)) = function.name.rsplit_once("::")
                && let Some(receiver_type) = function
                    .signature
                    .parameters
                    .as_ref()
                    .and_then(|parameters| parameters.first())
                    .and_then(named_type_name)
            {
                methods
                    .entry(format!("{receiver_type}::{method}"))
                    .or_default()
                    .push(function.clone());
                for declaration in self.types.values() {
                    if declaration.name != receiver_type
                        && is_assignable(&self.types, receiver_type, &declaration.name)
                    {
                        methods
                            .entry(format!("{}::{method}", declaration.name))
                            .or_default()
                            .push(function.clone());
                    }
                }
            }
        }
        methods
    }

    fn canonical_value(&self, include_hash: bool) -> Value {
        let types = self
            .types
            .values()
            .map(|declaration| {
                if let Some(enum_definition) = declaration.enum_definition.as_ref() {
                    return json!({
                        "name": declaration.name,
                        "kind": "enum",
                        "transport": type_name(&Type::Integer(enum_definition.underlying_type)),
                        "flags": enum_definition.flags,
                        "variants": enum_definition.variants.iter().map(|(name, raw)| json!({
                            "name": name,
                            "raw": format!("0x{raw:x}"),
                        })).collect::<Vec<_>>(),
                    });
                }
                let mut value = json!({
                    "name": declaration.name,
                    "kind": if declaration.value_layout.is_some() { "value" } else { "opaque" },
                    "transport": declaration.transport.as_str(),
                });
                if let Some(base_type) = declaration.base_type.as_deref() {
                    value
                        .as_object_mut()
                        .expect("host type JSON is an object")
                        .insert("base".into(), Value::String(base_type.into()));
                }
                if let Some(layout) = declaration.value_layout {
                    value
                        .as_object_mut()
                        .expect("host type JSON is an object")
                        .insert("layout".into(), Value::String(layout.canonical_name()));
                }
                value
            })
            .collect::<Vec<_>>();
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
                        let mut value = json!({
                            "id": format!("0x{:016x}", function.function_id),
                            "name": name,
                            "parameters": function.signature.parameters.as_ref().expect("host signatures are fixed").iter().map(type_name).collect::<Vec<_>>(),
                            "return": type_name(&function.signature.return_type),
                            "capability": function.capability,
                            "call_kind": function.call_kind.as_str(),
                            "thread_affinity": function.thread_affinity.as_str(),
                        });
                        if let Some(receiver) = function.receiver {
                            value.as_object_mut().expect("function JSON is an object").insert(
                                "receiver".into(),
                                Value::String(receiver.as_str().into()),
                            );
                        }
                        value
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
            "types": types,
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
