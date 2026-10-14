//! Name bindings for declarations, separate from runtime storage slots.
use std::rc::Rc;

use crate::value::{
    BuiltinType, EnumType, HostType, ModuleValue, StructType, TraitType, TypeAliasType, Value,
};

#[derive(Clone)]
pub enum Declaration {
    HostType(Rc<HostType>),
    BuiltinType(BuiltinType),
    Module(Rc<ModuleValue>),
    StructType(Rc<StructType>),
    EnumType(Rc<EnumType>),
    TraitType(Rc<TraitType>),
    TypeAlias(Rc<TypeAliasType>),
}

impl Declaration {
    pub fn type_name(&self) -> String {
        match self {
            Self::HostType(value) => format!("type {}", value.name),
            Self::BuiltinType(value) => match value {
                BuiltinType::Vec => "type Vec".into(),
                BuiltinType::HashMap => "type HashMap".into(),
                BuiltinType::HashSet => "type HashSet".into(),
                BuiltinType::Integer(kind) => format!("type {kind}"),
                BuiltinType::Float(kind) => format!("type {kind}"),
            },
            Self::Module(value) => format!("module {}", value.name),
            Self::StructType(value) => format!("type {}", value.name),
            Self::EnumType(value) => format!("type {}", value.name),
            Self::TraitType(value) => format!("trait {}", value.name),
            Self::TypeAlias(value) => format!("type alias {}", value.name),
        }
    }
}

#[derive(Clone)]
pub enum Binding {
    Value(Value),
    Declaration(Declaration),
}

impl From<Value> for Binding {
    fn from(value: Value) -> Self {
        Self::Value(value)
    }
}
impl From<Declaration> for Binding {
    fn from(value: Declaration) -> Self {
        Self::Declaration(value)
    }
}
impl Binding {
    pub fn into_value(self) -> Result<Value, String> {
        match self {
            Self::Value(value) => Ok(value),
            Self::Declaration(value) => Err(format!(
                "{} is a declaration, not a runtime value",
                value.type_name()
            )),
        }
    }
    pub fn into_declaration(self) -> Option<Declaration> {
        match self {
            Self::Declaration(value) => Some(value),
            Self::Value(_) => None,
        }
    }
    pub fn type_name(&self) -> String {
        match self {
            Self::Declaration(value) => value.type_name(),
            Self::Value(value) => value.type_name(),
        }
    }
}
