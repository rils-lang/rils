//! Runtime declarations derived from the shared host contract.

use std::{cell::RefCell, rc::Rc};

use rils_host::{HostContract, HostEnumDefinition};

use super::{EnumType, HostType};
use crate::runtime_builtins::NativeOwnedContext;

pub use rils_host::HOST_FLAGS_RAW_VARIANT;

pub fn enum_definition(name: String, definition: &HostEnumDefinition) -> Rc<EnumType> {
    Rc::new(EnumType {
        name,
        generic_parameters: Vec::new(),
        variants: definition.rils_variants(),
        methods: RefCell::default(),
        trait_methods: RefCell::default(),
        implemented_traits: RefCell::new(["Clone".into(), "Copy".into()].into_iter().collect()),
        associated_types: RefCell::default(),
    })
}

impl NativeOwnedContext {
    /// Portable handle and inline transports have Copy semantics. Enum
    /// transports keep their enum declaration, including raw flags payloads.
    pub fn from_host_contract(contract: &HostContract) -> Result<Self, String> {
        let mut context = Self::default();
        for declaration in contract.types() {
            if let Some(definition) = &declaration.enum_definition {
                context
                    .enums
                    .push(enum_definition(declaration.name.clone(), definition));
            } else {
                context.hosts.push(Rc::new(HostType {
                    name: declaration.name.clone(),
                    base_types: contract.type_lineage(&declaration.name)?,
                    copy: true,
                    methods: RefCell::default(),
                }));
            }
        }
        Ok(context)
    }
}
