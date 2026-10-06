//! Weak declaration handles shared by all lexical and module environments.

use crate::{
    Type,
    value::{EnumType, StructType, TraitType, TypeAliasType, Value},
};
use rils_frontend::semantic::DeclarationTypeResolver;
use std::{
    cell::RefCell,
    collections::HashMap,
    rc::{Rc, Weak},
};

enum Declaration {
    Struct(Weak<StructType>),
    Enum(Weak<EnumType>),
    Trait(Weak<TraitType>),
    Alias(Weak<TypeAliasType>),
}

impl Declaration {
    fn value(&self) -> Option<Value> {
        match self {
            Self::Struct(value) => value.upgrade().map(Value::StructType),
            Self::Enum(value) => value.upgrade().map(Value::EnumType),
            Self::Trait(value) => value.upgrade().map(Value::TraitType),
            Self::Alias(value) => value.upgrade().map(Value::TypeAlias),
        }
    }
}

#[derive(Default)]
pub(super) struct TypeDeclarations {
    values: RefCell<HashMap<String, Declaration>>,
    resolver: RefCell<DeclarationTypeResolver>,
}

impl TypeDeclarations {
    pub(super) fn register(&self, value: &Value) {
        let (name, declaration) = match value {
            Value::StructType(value) => (&value.name, Declaration::Struct(Rc::downgrade(value))),
            Value::EnumType(value) => (&value.name, Declaration::Enum(Rc::downgrade(value))),
            Value::TraitType(value) => (&value.name, Declaration::Trait(Rc::downgrade(value))),
            Value::TypeAlias(value) => (&value.name, Declaration::Alias(Rc::downgrade(value))),
            _ => return,
        };
        self.values.borrow_mut().insert(name.clone(), declaration);
    }

    pub(super) fn get(&self, name: &str) -> Option<Value> {
        self.values.borrow().get(name)?.value()
    }

    pub(super) fn definitions(&self) -> (Vec<Rc<StructType>>, Vec<Rc<EnumType>>) {
        let mut structs = Vec::new();
        let mut enums = Vec::new();
        self.values.borrow_mut().retain(|_, declaration| {
            match declaration.value() {
                Some(Value::StructType(value)) => structs.push(value),
                Some(Value::EnumType(value)) => enums.push(value),
                Some(_) => {}
                None => return false,
            }
            true
        });
        (structs, enums)
    }

    pub(super) fn set_resolver(&self, resolver: DeclarationTypeResolver) {
        *self.resolver.borrow_mut() = resolver;
    }

    pub(super) fn resolve(&self, ty: &Type, module: &[String]) -> Type {
        self.resolver.borrow().resolve(ty, module)
    }

    pub(super) fn is_declared_type(&self, name: &str) -> bool {
        self.resolver.borrow().is_declared_type(name)
    }

    pub(super) fn inaccessible_type(&self, ty: &Type, module: &[String]) -> Option<String> {
        self.resolver.borrow().inaccessible_type(ty, module)
    }
}
