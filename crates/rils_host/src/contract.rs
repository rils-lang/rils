use super::*;

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
            types: BTreeMap::new(),
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

    pub fn register_type(
        &mut self,
        name: impl Into<String>,
        base_type: Option<impl Into<String>>,
        transport: HostTypeTransport,
    ) -> Result<(), String> {
        let name = name.into();
        let base_type = base_type.map(Into::into);
        if transport != HostTypeTransport::HostHandle {
            return Err("inline host values must be registered with register_value_type".into());
        }
        validate_type_name(&name)?;
        if let Some(base) = base_type.as_deref() {
            validate_type_name(base)?;
            if base == name {
                return Err(format!("host type `{name}` cannot inherit itself"));
            }
        }
        if self.types.len() >= HOST_MANIFEST_MAX_TYPES && !self.types.contains_key(&name) {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_TYPES} type limit"
            ));
        }
        let declaration = HostTypeDeclaration {
            name: name.clone(),
            base_type,
            transport,
            value_layout: None,
            enum_definition: None,
        };
        match self.types.get(&name) {
            Some(existing) if existing == &declaration => Ok(()),
            Some(_) => Err(format!("host type `{name}` has conflicting declarations")),
            None => {
                self.types.insert(name, declaration);
                Ok(())
            }
        }
    }

    pub fn register_value_type(
        &mut self,
        name: impl Into<String>,
        layout: HostValueLayout,
    ) -> Result<(), String> {
        let name = name.into();
        validate_type_name(&name)?;
        if self.types.len() >= HOST_MANIFEST_MAX_TYPES && !self.types.contains_key(&name) {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_TYPES} type limit"
            ));
        }
        let declaration = HostTypeDeclaration {
            name: name.clone(),
            base_type: None,
            transport: HostTypeTransport::InlineValue,
            value_layout: Some(layout),
            enum_definition: None,
        };
        match self.types.get(&name) {
            Some(existing) if existing == &declaration => Ok(()),
            Some(_) => Err(format!("host type `{name}` has conflicting declarations")),
            None => {
                self.types.insert(name, declaration);
                Ok(())
            }
        }
    }

    pub fn register_enum_type(
        &mut self,
        name: impl Into<String>,
        underlying_type: IntegerType,
        flags: bool,
        variants: impl IntoIterator<Item = (String, u128)>,
    ) -> Result<(), String> {
        let name = name.into();
        validate_type_name(&name)?;
        if self.types.len() >= HOST_MANIFEST_MAX_TYPES && !self.types.contains_key(&name) {
            return Err(format!(
                "host contract exceeds the {HOST_MANIFEST_MAX_TYPES} type limit"
            ));
        }
        let variants = variants.into_iter().collect::<BTreeMap<_, _>>();
        if variants.len() > HOST_MANIFEST_MAX_ENUM_VARIANTS {
            return Err(format!(
                "host enum `{name}` exceeds the {HOST_MANIFEST_MAX_ENUM_VARIANTS} variant limit"
            ));
        }
        for (variant, raw_value) in &variants {
            validate_identifier(variant, "host enum variant")?;
            validate_enum_raw_value(underlying_type, *raw_value)
                .map_err(|message| format!("host enum `{name}` variant `{variant}` {message}"))?;
        }
        let declaration = HostTypeDeclaration {
            name: name.clone(),
            base_type: None,
            transport: HostTypeTransport::Enum,
            value_layout: None,
            enum_definition: Some(HostEnumDefinition {
                underlying_type,
                flags,
                variants,
            }),
        };
        match self.types.get(&name) {
            Some(existing) if existing == &declaration => Ok(()),
            Some(_) => Err(format!("host type `{name}` has conflicting declarations")),
            None => {
                self.types.insert(name, declaration);
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
        self.register_function_with_options_and_receiver(
            function_id,
            name,
            signature,
            capability,
            call_kind,
            thread_affinity,
            None,
        )
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "this stable registration API keeps each host ABI property explicit"
    )]
    pub fn register_function_with_options_and_receiver(
        &mut self,
        function_id: u64,
        name: impl Into<String>,
        signature: FunctionSignature,
        capability: impl Into<String>,
        call_kind: HostCallKind,
        thread_affinity: HostThreadAffinity,
        receiver: Option<HostReceiver>,
    ) -> Result<(), String> {
        let name = name.into();
        let capability = capability.into();
        let (module, function_name) = split_function_name(&name)?;
        if !self.modules.contains_key(module) {
            self.register_module(module, 1)?;
        }
        validate_identifier(function_name, "host function name")?;
        self.validate_signature(&signature)?;
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
        let overload_key = function_overload_key(&name, &signature);
        if self.functions.contains_key(&overload_key) {
            return Err(format!(
                "host function `{name}` is already declared with mapped parameter signature `{}`",
                format_parameter_list(&signature)
            ));
        }
        if !self.function_ids.insert(function_id) {
            return Err(format!(
                "host function id {function_id} is already declared"
            ));
        }
        self.functions.insert(
            overload_key,
            HostFunctionDeclaration {
                function_id,
                name,
                signature,
                capability,
                call_kind,
                thread_affinity,
                receiver,
            },
        );
        self.parameter_count += parameter_count;
        Ok(())
    }

    pub fn modules(&self) -> impl ExactSizeIterator<Item = &HostModuleDeclaration> {
        self.modules.values()
    }

    pub fn types(&self) -> impl ExactSizeIterator<Item = &HostTypeDeclaration> {
        self.types.values()
    }

    pub fn host_type(&self, name: &str) -> Option<&HostTypeDeclaration> {
        self.types.get(name)
    }

    pub fn is_type_assignable(&self, expected: &str, actual: &str) -> bool {
        is_assignable(&self.types, expected, actual)
    }

    pub fn type_assignment_distance(&self, expected: &str, actual: &str) -> Option<usize> {
        if expected == actual {
            return Some(0);
        }
        let mut distance = 0usize;
        let mut current = self.types.get(actual)?;
        while let Some(base) = current.base_type.as_deref() {
            distance += 1;
            if base == expected {
                return Some(distance);
            }
            current = self.types.get(base)?;
        }
        None
    }

    pub fn type_lineage(&self, name: &str) -> Result<HashSet<String>, String> {
        let mut lineage = HashSet::new();
        let mut current = self
            .types
            .get(name)
            .ok_or_else(|| format!("host type `{name}` is not declared"))?;
        while let Some(base_name) = current.base_type.as_deref() {
            if !lineage.insert(base_name.to_owned()) {
                return Err(format!(
                    "host type inheritance contains a cycle at `{base_name}`"
                ));
            }
            current = self.types.get(base_name).ok_or_else(|| {
                format!("host type `{name}` inherits unknown host type `{base_name}`")
            })?;
        }
        Ok(lineage)
    }

    pub fn receiver_methods(&self, receiver_type: &str) -> Vec<&HostFunctionDeclaration> {
        self.functions
            .values()
            .filter(|function| function.receiver.is_some())
            .filter(|function| {
                function
                    .signature
                    .parameters
                    .as_ref()
                    .and_then(|parameters| parameters.first())
                    .and_then(named_type_name)
                    .is_some_and(|expected| {
                        expected == receiver_type
                            || is_assignable(&self.types, expected, receiver_type)
                    })
            })
            .collect()
    }

    pub fn functions(&self) -> impl ExactSizeIterator<Item = &HostFunctionDeclaration> {
        self.functions.values()
    }

    pub fn is_empty(&self) -> bool {
        self.functions.is_empty()
    }

    pub fn function(&self, name: &str) -> Option<&HostFunctionDeclaration> {
        let mut overloads = self.functions_named(name);
        let function = overloads.next()?;
        overloads.next().is_none().then_some(function)
    }

    pub fn functions_named(&self, name: &str) -> impl Iterator<Item = &HostFunctionDeclaration> {
        self.functions
            .values()
            .filter(move |function| function.name == name)
    }

    #[doc(hidden)]
    pub fn function_overloads(&self) -> HashMap<String, Vec<HostFunctionDeclaration>> {
        let mut overloads = HashMap::<String, Vec<HostFunctionDeclaration>>::new();
        for function in self.functions.values() {
            overloads
                .entry(function.name.clone())
                .or_default()
                .push(function.clone());
        }
        overloads
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
        for declaration in fragment.types() {
            if let Some(enum_definition) = declaration.enum_definition.as_ref() {
                self.register_enum_type(
                    &declaration.name,
                    enum_definition.underlying_type,
                    enum_definition.flags,
                    enum_definition
                        .variants
                        .iter()
                        .map(|(name, value)| (name.clone(), *value)),
                )?;
            } else if let Some(layout) = declaration.value_layout {
                self.register_value_type(&declaration.name, layout)?;
            } else {
                self.register_type(
                    &declaration.name,
                    declaration.base_type.as_deref(),
                    declaration.transport,
                )?;
            }
        }
        for function in fragment.functions() {
            if let Some(existing) = self
                .functions_named(&function.name)
                .find(|existing| existing.signature.parameters == function.signature.parameters)
            {
                if existing == function {
                    continue;
                }
                return Err(format!(
                    "host function `{}` has conflicting declarations",
                    function.name
                ));
            }
            self.register_function_with_options_and_receiver(
                function.function_id,
                &function.name,
                function.signature.clone(),
                &function.capability,
                function.call_kind,
                function.thread_affinity,
                function.receiver,
            )?;
        }
        Ok(())
    }

    fn validate_signature(&self, signature: &FunctionSignature) -> Result<(), String> {
        validate_signature(signature, &self.types)
    }
}
