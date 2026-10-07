//! Record constructors resolve the declaration path before inferring fields.

use super::*;

impl Inferencer<'_> {
    pub(super) fn nominal_variant_type(&self, expression: &Expr) -> Option<Type> {
        let (segments, arguments) = match expression {
            Expr::Path { segments, .. } => (segments, &[][..]),
            Expr::GenericPath {
                segments,
                arguments,
                ..
            } => (segments, arguments.as_slice()),
            _ => return None,
        };
        let (variant, owner_path) = segments.split_last()?;
        let owner = if owner_path == ["Self"] {
            self.lookup("Self")?.ty.clone()
        } else {
            self.declaration_types.resolve(
                &Type::Named {
                    name: owner_path.join("::"),
                    arguments: arguments.to_vec(),
                },
                &self.module_path,
            )
        };
        let Type::Named {
            name,
            mut arguments,
        } = owner
        else {
            return None;
        };
        let definition = self.types.get(&name)?;
        definition.variants.get(variant)?;
        if arguments.is_empty() {
            arguments.resize(definition.generic_parameters.len(), Type::Unknown);
        }
        Some(Type::Named { name, arguments })
    }

    pub(super) fn tuple_variant_type(
        &mut self,
        callee: &Expr,
        arguments: &[Expr],
        actual: &[Type],
    ) -> Option<Type> {
        let (segments, explicit) = match callee {
            Expr::Path { segments, .. } => (segments, &[][..]),
            Expr::GenericPath {
                segments,
                arguments,
                ..
            } => (segments, arguments.as_slice()),
            _ => return None,
        };
        let (variant, owner_path) = segments.split_last()?;
        let owner = if owner_path == ["Self"] {
            self.lookup("Self")?.ty.clone()
        } else {
            self.declaration_types.resolve(
                &Type::Named {
                    name: owner_path.join("::"),
                    arguments: explicit.to_vec(),
                },
                &self.module_path,
            )
        };
        let Type::Named {
            name,
            arguments: type_arguments,
        } = owner
        else {
            return None;
        };
        let definition = self.types.get(&name)?.clone();
        let VariantDefinition::Tuple(fields) = definition.variants.get(variant)? else {
            return None;
        };
        let mut substitutions = definition
            .generic_parameters
            .iter()
            .cloned()
            .zip(type_arguments)
            .collect::<HashMap<_, _>>();
        for (field, actual) in fields.iter().zip(actual) {
            infer_type_variables(field, actual, &mut substitutions);
        }
        for (field, argument) in fields.iter().zip(arguments) {
            self.apply_expected_type(argument, &field.substitute(&substitutions));
        }
        Some(Type::Named {
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
        })
    }

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
