use std::collections::HashSet;

use super::*;
use rils_frontend::{ast::EnumVariant, copy_types::CopyTypes};

impl NativeOwnedContext {
    fn copy_types(&self) -> (CopyTypes, HashSet<String>) {
        let mut declarations = CopyTypes::default();
        for definition in self
            .structs
            .iter()
            .filter(|definition| !definition.opaque_native)
        {
            declarations.define(
                definition.name.clone(),
                definition
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect(),
                definition
                    .fields
                    .iter()
                    .map(|field| field.type_annotation.clone())
                    .collect(),
            );
            if definition.implemented_traits.borrow().contains("Copy") {
                declarations.implement(&definition.name);
            }
        }
        for definition in &self.enums {
            declarations.define(
                definition.name.clone(),
                definition
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .collect(),
                definition
                    .variants
                    .iter()
                    .flat_map(|variant| match variant {
                        EnumVariant::Unit { .. } => vec![],
                        EnumVariant::Tuple { fields, .. } => fields.clone(),
                        EnumVariant::Record { fields, .. } => fields
                            .iter()
                            .map(|field| field.type_annotation.clone())
                            .collect(),
                    })
                    .collect(),
            );
            if definition.implemented_traits.borrow().contains("Copy") {
                declarations.implement(&definition.name);
            }
        }
        let hosts = self
            .hosts
            .iter()
            .filter(|host| host.copy)
            .map(|host| host.name.clone())
            .collect();
        (declarations, hosts)
    }

    pub fn type_is_copy(&self, ty: &Type) -> bool {
        let (declarations, hosts) = self.copy_types();
        declarations.is_copy(ty, &hosts)
    }

    /// Validate all variants against the actual registered host Copy policies.
    pub fn copy_fields_eligible(&self, ty: &Type) -> bool {
        let (declarations, hosts) = self.copy_types();
        declarations.fields_are_copy(ty, &hosts)
    }
}
