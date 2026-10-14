//! Owned declaration handles shared by all lexical and module environments.

use crate::{
    Type,
    value::{Declaration, EnumType, HostType, StructType},
};
use rils_frontend::semantic::DeclarationTypeResolver;
use std::{cell::RefCell, collections::HashMap, rc::Rc};

#[derive(Default)]
pub(super) struct TypeDeclarations {
    values: RefCell<HashMap<String, Declaration>>,
    resolver: RefCell<DeclarationTypeResolver>,
}

impl TypeDeclarations {
    pub(super) fn register(&self, value: &Declaration) {
        let (name, declaration) = match value {
            Declaration::StructType(value) => (&value.name, Declaration::StructType(value.clone())),
            Declaration::EnumType(value) => (&value.name, Declaration::EnumType(value.clone())),
            Declaration::TraitType(value) => (&value.name, Declaration::TraitType(value.clone())),
            Declaration::TypeAlias(value) => (&value.name, Declaration::TypeAlias(value.clone())),
            Declaration::HostType(value) => (&value.name, Declaration::HostType(value.clone())),
            _ => return,
        };
        self.values.borrow_mut().insert(name.clone(), declaration);
    }

    pub(super) fn get(&self, name: &str) -> Option<Declaration> {
        self.values.borrow().get(name).cloned()
    }

    pub(super) fn definitions(&self) -> (Vec<Rc<StructType>>, Vec<Rc<EnumType>>) {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        for declaration in self.values.borrow().values() {
            match declaration {
                Declaration::StructType(value) => structs.push(value.clone()),
                Declaration::EnumType(value) => enums.push(value.clone()),
                _ => {}
            }
        }
        (structs, enums)
    }

    pub(super) fn set_resolver(&self, resolver: DeclarationTypeResolver) {
        *self.resolver.borrow_mut() = resolver;
    }

    pub(super) fn host_definitions(&self) -> Vec<Rc<HostType>> {
        self.values
            .borrow()
            .values()
            .filter_map(|declaration| match declaration {
                Declaration::HostType(value) => Some(value.clone()),
                _ => None,
            })
            .collect()
    }

    pub(super) fn resolve(&self, ty: &Type, module: &[String]) -> Type {
        self.resolver.borrow().resolve(ty, module)
    }

    pub(super) fn is_declared_type(&self, name: &str) -> bool {
        self.resolver.borrow().is_declared_type(name)
    }

    pub(super) fn declared_value_traits(&self, name: &str) -> std::collections::HashSet<String> {
        let resolver = self.resolver.borrow();
        let declarations = resolver.copy_types();
        let mut traits = std::collections::HashSet::new();
        if declarations.is_declared(name) {
            traits.insert("Copy".into());
        }
        if declarations.has_clone(name) {
            traits.insert("Clone".into());
        }
        traits
    }

    pub(super) fn inaccessible_type(&self, ty: &Type, module: &[String]) -> Option<String> {
        self.resolver.borrow().inaccessible_type(ty, module)
    }
}
