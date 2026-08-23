use std::{
    collections::{HashMap, HashSet},
    rc::Rc,
};

use crate::{
    types::{FunctionSignature, Type},
    value::Value,
};

use super::{call_core_import, core_imports};

pub const BYTECODE_HOST_ABI_VERSION: u32 = rils_compiler::HOST_CONTRACT_ABI_VERSION;

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
}

impl BytecodeHost {
    pub fn new(abi_version: u32) -> Self {
        Self {
            abi_version,
            capabilities: HashSet::new(),
            functions: HashMap::new(),
        }
    }

    pub fn standard() -> Self {
        let mut host = Self::new(BYTECODE_HOST_ABI_VERSION);
        host.allow_capability("core");
        for (name, signature) in core_imports() {
            let import_name = name.to_string();
            let handler_name = import_name.clone();
            host.register_function(import_name, signature, "core", move |arguments| {
                call_core_import(&handler_name, arguments)
            })
            .expect("standard core imports are unique");
        }
        host
    }

    pub fn allow_capability(&mut self, capability: impl Into<String>) {
        self.capabilities.insert(capability.into());
    }

    pub fn enable_standard_io(&mut self) -> Result<(), String> {
        self.enable_standard_capability("std::io")?;
        if !self.functions.contains_key("std::io::print") {
            self.register_function(
                "std::io::print",
                FunctionSignature::variadic(Type::Unit),
                "std::io",
                |arguments| {
                    for value in arguments {
                        print!("{value}");
                    }
                    Ok(Value::Unit)
                },
            )?;
        }
        if !self.functions.contains_key("std::io::println") {
            self.register_function(
                "std::io::println",
                FunctionSignature::variadic(Type::Unit),
                "std::io",
                |arguments| {
                    for (index, value) in arguments.iter().enumerate() {
                        if index > 0 {
                            print!(" ");
                        }
                        print!("{value}");
                    }
                    println!();
                    Ok(Value::Unit)
                },
            )?;
        }
        Ok(())
    }

    pub fn enable_standard_fs(&mut self) -> Result<(), String> {
        self.enable_standard_capability("std::fs")
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
