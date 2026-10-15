//! Declaration context shared by owned native calls and storage boundaries.

use std::rc::Rc;

use crate::{
    Type,
    environment::Environment,
    value::{
        EnumType, HostType, StructType, Value, record_layout::RecordLayoutResolver,
        runtime_layouts::HostLayoutProvider, storage::TypedStorageContext,
    },
};
use rils_value::DynamicLayout;

mod copy;

#[derive(Clone, Default)]
pub struct NativeOwnedContext {
    pub structs: Vec<Rc<StructType>>,
    pub enums: Vec<Rc<EnumType>>,
    pub hosts: Vec<Rc<HostType>>,
}

impl NativeOwnedContext {
    pub fn from_environment(environment: &Environment) -> Self {
        let (structs, enums) = environment.visible_type_definitions();
        Self {
            structs,
            enums,
            hosts: environment.visible_host_definitions(),
        }
    }

    pub fn layout(&self, ty: &Type) -> Result<Rc<DynamicLayout>, String> {
        self.storage().layout(ty)
    }

    pub fn storage(&self) -> TypedStorageContext<'_> {
        TypedStorageContext::with_hosts(&self.structs, &self.enums, &self.hosts)
    }

    pub fn empty_collection(&self, ty: &Type) -> Result<Value, String> {
        if !ty.is_concrete_type() {
            return Err(format!("cannot infer the complete collection type: {ty}"));
        }
        let Type::Named { name, .. } = ty else {
            return Err(format!("{ty} has no declared empty constructor"));
        };
        let symbol = rils_frontend::standard_library::empty_native_constructor_symbol(name)
            .ok_or_else(|| format!("{ty} has no declared empty constructor"))?;
        super::call_native_constructor_symbol(symbol, Vec::new(), self, Some(ty))
            .ok_or_else(|| format!("{ty} has no native empty constructor"))?
    }

    pub fn none(&self, item: &Type) -> Result<Value, String> {
        self.storage()
            .construct_option(&Type::Option(Box::new(item.clone())), None)
    }
}

pub(crate) fn resolve_layout(
    ty: &Type,
    structs: &[Rc<StructType>],
    enums: &[Rc<EnumType>],
    hosts: &[Rc<HostType>],
) -> Result<Rc<DynamicLayout>, String> {
    let provider = HostLayoutProvider::new(hosts);
    RecordLayoutResolver::with_provider(structs, enums, Some(&provider)).resolve(ty)
}
