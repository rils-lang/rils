//! Runtime declarations derived from the shared host contract.

use std::{cell::RefCell, rc::Rc};

use rils_host::{HostContract, HostEnumDefinition};

use super::{EnumType, HostType};
use crate::ast::EnumVariant;
use crate::runtime_builtins::NativeOwnedContext;

pub use rils_host::HOST_FLAGS_RAW_VARIANT;

pub fn enum_definition(name: String, definition: &HostEnumDefinition) -> Rc<EnumType> {
    Rc::new(EnumType {
        name,
        host_definition: Some(definition.clone()),
        generic_parameters: Vec::new(),
        variants: definition.rils_variants(),
        methods: RefCell::default(),
        trait_methods: RefCell::default(),
        implemented_traits: RefCell::new(["Clone".into(), "Copy".into()].into_iter().collect()),
        associated_types: RefCell::default(),
    })
}

/// Link bytecode's portable enum schema to the installed host contract.
/// Methods and trait tables remain local to the compiled declaration.
pub fn link_enum_declaration(declaration: &EnumType, host: &EnumType) -> Result<EnumType, String> {
    let Some(contract) = &host.host_definition else {
        return Err("enum is not a host declaration".into());
    };
    let expected = contract.rils_variants();
    let matches = declaration.name == host.name
        && declaration.generic_parameters.is_empty()
        && declaration.variants.len() == expected.len()
        && declaration
            .variants
            .iter()
            .zip(&expected)
            .all(|(actual, expected)| match (actual, expected) {
                (
                    EnumVariant::Unit { name: actual, .. },
                    EnumVariant::Unit { name: expected, .. },
                ) => actual == expected,
                (
                    EnumVariant::Tuple {
                        name: actual,
                        fields: actual_fields,
                        ..
                    },
                    EnumVariant::Tuple {
                        name: expected,
                        fields: expected_fields,
                        ..
                    },
                ) => actual == expected && actual_fields == expected_fields,
                _ => false,
            });
    if !matches {
        return Err(format!(
            "host enum declaration `{}` does not match its contract",
            host.name
        ));
    }
    let mut linked = declaration.clone();
    linked.host_definition = Some(contract.clone());
    linked
        .implemented_traits
        .get_mut()
        .extend(host.implemented_traits.borrow().iter().cloned());
    Ok(linked)
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
