//! Record constructors resolve the declaration path before inferring fields.

use super::*;

impl Inferencer<'_> {
    pub(super) fn record_literal(
        &mut self,
        path: &[String],
        fields: &[crate::ast::RecordField],
        returns: &mut Vec<Type>,
    ) -> Type {
        let actual_fields = fields
            .iter()
            .map(|field| (&field.name, self.expression(&field.value, returns)))
            .collect::<Vec<_>>();
        let Some(first) = path.first() else {
            return Type::Unknown;
        };
        let resolve = |segments: &[String]| {
            self.declaration_types
                .resolve(&Type::named(segments.join("::")), &self.module_path)
        };
        let (owner, variant) = if first == "Self" {
            let Some(owner) = self.lookup("Self").map(|binding| binding.ty.clone()) else {
                return Type::Unknown;
            };
            (owner, path.get(1))
        } else {
            let full = resolve(path);
            if matches!(&full, Type::Named { name, .. } if self.types.contains_key(name)) {
                (full, None)
            } else if path.len() > 1 {
                (resolve(&path[..path.len() - 1]), path.last())
            } else {
                (full, None)
            }
        };
        let Type::Named { name, arguments } = owner else {
            return Type::Unknown;
        };
        let Some(definition) = self.types.get(&name).cloned() else {
            return Type::Named { name, arguments };
        };
        let declared_fields = variant
            .and_then(|name| definition.variants.get(name))
            .and_then(|variant| match variant {
                VariantDefinition::Record(fields) => Some(fields),
                _ => None,
            })
            .unwrap_or(&definition.fields);
        let mut substitutions = definition
            .generic_parameters
            .iter()
            .cloned()
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        for (field, actual) in actual_fields {
            if let Some(expected) = declared_fields.get(field) {
                infer_type_variables(expected, &actual, &mut substitutions);
            }
        }
        for field in fields {
            if let Some(expected) = declared_fields.get(&field.name) {
                self.apply_expected_type(&field.value, &expected.substitute(&substitutions));
            }
        }
        Type::Named {
            name,
            arguments: definition
                .generic_parameters
                .iter()
                .map(|parameter| {
                    substitutions
                        .get(parameter)
                        .cloned()
                        .unwrap_or(Type::Unknown)
                })
                .collect(),
        }
    }
}
