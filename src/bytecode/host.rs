use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    types::{FunctionSignature, Type},
    value::Value,
};

use super::{call_core_import, core_imports, resolve_core_import};

pub const BYTECODE_HOST_ABI_VERSION: u32 = rils_host::HOST_CONTRACT_ABI_VERSION;

pub type BytecodeHostHandler = dyn Fn(&[Value]) -> Result<Value, String>;

#[derive(Clone)]
pub(super) struct HostBinding {
    pub signature: FunctionSignature,
    pub capability: String,
    pub function: Rc<BytecodeHostHandler>,
}

#[derive(Clone)]
pub struct BytecodeHost {
    pub(super) abi_version: u32,
    pub(super) capabilities: HashSet<String>,
    pub(super) functions: HashMap<String, Vec<HostBinding>>,
    pub(super) host_value_formatter: Option<Rc<crate::HostValueFormatter>>,
}

impl BytecodeHost {
    pub fn new(abi_version: u32) -> Self {
        Self {
            abi_version,
            capabilities: HashSet::new(),
            functions: HashMap::new(),
            host_value_formatter: None,
        }
    }

    pub fn standard() -> Self {
        let mut host = Self::new(BYTECODE_HOST_ABI_VERSION);
        host.allow_capability("core");
        for (name, signature) in core_imports() {
            let import_name = name.to_string();
            let operation = resolve_core_import(name)
                .unwrap_or_else(|| panic!("standard core import `{name}` has no operation ID"));
            host.register_function(import_name, signature, "core", move |arguments| {
                call_core_import(operation, arguments)
            })
            .expect("standard core imports are unique");
        }
        host
    }

    pub fn allow_capability(&mut self, capability: impl Into<String>) {
        self.capabilities.insert(capability.into());
    }

    pub fn enable_standard_io(&mut self) -> Result<(), String> {
        self.set_shared_output_handler(crate::output::default_output_handler())
    }

    pub fn set_output_handler<F>(&mut self, handler: F) -> Result<(), String>
    where
        F: Fn(&str, bool) -> Result<(), String> + 'static,
    {
        self.set_shared_output_handler(Rc::new(handler))
    }

    pub fn set_host_value_formatter<F>(&mut self, formatter: F)
    where
        F: Fn(&Value, crate::HostFormatSpec) -> Result<Option<String>, String> + 'static,
    {
        self.host_value_formatter = Some(Rc::new(formatter));
    }

    pub fn reset_host_value_formatter(&mut self) {
        self.host_value_formatter = None;
    }

    pub(crate) fn set_shared_output_handler(
        &mut self,
        handler: Rc<crate::OutputHandler>,
    ) -> Result<(), String> {
        self.enable_standard_capability("std::io")?;
        self.functions.remove("std::io::print");
        self.functions.remove("std::io::println");
        let print_handler = handler.clone();
        self.register_function(
            "std::io::print",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
            move |arguments| {
                let Some(Value::String(format)) = arguments.first() else {
                    return Err("print! requires a format string".into());
                };
                let output = crate::formatting::format_arguments(format, &arguments[1..])?;
                print_handler(&output, false)?;
                Ok(Value::Unit)
            },
        )?;
        self.register_function(
            "std::io::println",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
            move |arguments| {
                if arguments.is_empty() {
                    handler("", true)?;
                    return Ok(Value::Unit);
                }
                let Some(Value::String(format)) = arguments.first() else {
                    return Err("println! requires a format string".into());
                };
                let output = crate::formatting::format_arguments(format, &arguments[1..])?;
                handler(&output, true)?;
                Ok(Value::Unit)
            },
        )?;
        Ok(())
    }

    pub fn enable_standard_fs(&mut self) -> Result<(), String> {
        self.enable_standard_capability("std::fs")
    }

    pub fn enable_standard_library(&mut self) -> Result<(), String> {
        for capability in Self::standard_library_capabilities() {
            self.enable_standard_capability(capability)?;
        }
        Ok(())
    }

    pub fn standard_library_capabilities() -> Vec<&'static str> {
        rils_builtins::standard_host_capabilities()
    }

    pub fn enable_standard_library_capability(&mut self, capability: &str) -> Result<(), String> {
        if !Self::standard_library_capabilities().contains(&capability) {
            return Err(format!(
                "unknown standard-library capability `{capability}`"
            ));
        }
        self.enable_standard_capability(capability)
    }

    pub fn register_function<F>(
        &mut self,
        name: impl Into<String>,
        signature: FunctionSignature,
        capability: impl Into<String>,
        function: F,
    ) -> Result<(), String>
    where
        F: Fn(&[Value]) -> Result<Value, String> + 'static,
    {
        let name = name.into();
        if self.functions.get(&name).is_some_and(|bindings| {
            bindings
                .iter()
                .any(|binding| binding.signature == signature)
        }) {
            return Err(format!(
                "bytecode host function `{name}` is already registered with that signature"
            ));
        }
        self.functions.entry(name).or_default().push(HostBinding {
            signature,
            capability: capability.into(),
            function: Rc::new(function),
        });
        Ok(())
    }

    fn enable_standard_capability(&mut self, capability: &str) -> Result<(), String> {
        self.allow_capability(capability);
        for (name, function) in crate::standard_library::bytecode_host_functions() {
            if !name.starts_with(capability) || self.functions.contains_key(&name) {
                continue;
            }
            let signature = function
                .signature
                .clone()
                .ok_or_else(|| format!("standard function `{name}` has no signature"))?;
            self.functions.entry(name).or_default().push(HostBinding {
                signature,
                capability: capability.into(),
                function: function.function.clone(),
            });
        }
        Ok(())
    }
}

impl Default for BytecodeHost {
    fn default() -> Self {
        Self::standard()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BytecodeImport {
    pub name: String,
    pub signature: FunctionSignature,
    pub abi_version: u32,
    pub capability: String,
}
