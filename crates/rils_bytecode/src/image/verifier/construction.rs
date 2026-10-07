use super::*;
use crate::ast::EnumVariant;

impl BytecodeModule {
    fn valid_constructor_type(&self, type_id: usize, expected: &Type) -> bool {
        let (name, arity) = match self.types.get(type_id) {
            Some(RuntimeType::Struct(definition)) => {
                (&definition.name, definition.generic_parameters.len())
            }
            Some(RuntimeType::Enum(definition)) => {
                (&definition.name, definition.generic_parameters.len())
            }
            None => return false,
        };
        self.valid_type(expected)
            && expected.is_type_witness()
            && matches!(expected, Type::Named { name: owner, arguments } if owner == name && arguments.len() == arity)
    }

    pub(super) fn valid_record_constructor(
        &self,
        type_id: usize,
        expected: &Type,
        variant: Option<&str>,
        fields: &[(String, usize)],
    ) -> bool {
        if !self.valid_constructor_type(type_id, expected) {
            return false;
        }
        let declared = match (&self.types[type_id], variant) {
            (RuntimeType::Struct(definition), None) if !definition.opaque_native => {
                &definition.fields
            }
            (RuntimeType::Enum(definition), Some(name)) => {
                let Some(EnumVariant::Record { fields, .. }) = definition
                    .variants
                    .iter()
                    .find(|entry| crate::value::enum_variant_name(entry) == name)
                else {
                    return false;
                };
                fields
            }
            _ => return false,
        };
        let names = fields
            .iter()
            .map(|(name, _)| name.as_str())
            .collect::<std::collections::HashSet<_>>();
        names.len() == fields.len()
            && names.len() == declared.len()
            && declared
                .iter()
                .all(|field| names.contains(field.name.as_str()))
    }

    pub(super) fn valid_tuple_constructor(
        &self,
        type_id: usize,
        expected: &Type,
        variant: &str,
        arity: usize,
    ) -> bool {
        if !self.valid_constructor_type(type_id, expected) {
            return false;
        }
        matches!(&self.types[type_id], RuntimeType::Enum(definition) if definition.variants.iter().any(|entry| matches!(entry, EnumVariant::Tuple { name, fields, .. } if name == variant && fields.len() == arity)))
    }

    pub(super) fn valid_unit_constructor(
        &self,
        type_id: usize,
        expected: &Type,
        variant: &str,
    ) -> bool {
        if !self.valid_constructor_type(type_id, expected) {
            return false;
        }
        matches!(&self.types[type_id], RuntimeType::Enum(definition) if definition.variants.iter().any(|entry| matches!(entry, EnumVariant::Unit { name, .. } if name == variant)))
    }
}
