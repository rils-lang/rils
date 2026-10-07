//! Pattern bindings use concrete fields and preserve lexical references.

use super::*;

impl Inferencer<'_> {
    pub(super) fn pattern(&mut self, pattern: &Pattern, expected: &Type, borrowed: bool) {
        match pattern {
            Pattern::Binding { name, span } => {
                self.define_binding(
                    name,
                    *span,
                    Binding {
                        ty: if borrowed {
                            Type::Reference {
                                mutable: false,
                                inner: Box::new(expected.clone()),
                            }
                        } else {
                            expected.clone()
                        },
                    },
                );
                self.type_hint(*span, expected.clone(), ": ");
            }
            Pattern::Some { inner, .. } => {
                self.pattern(inner, &option_inner(Some(expected.clone())), borrowed);
            }
            Pattern::Ok { inner, .. } => {
                let ty = match expected {
                    Type::Result(ok, _) => (**ok).clone(),
                    _ => Type::Unknown,
                };
                self.pattern(inner, &ty, borrowed);
            }
            Pattern::Err { inner, .. } => {
                let ty = match expected {
                    Type::Result(_, error) => (**error).clone(),
                    _ => Type::Unknown,
                };
                self.pattern(inner, &ty, borrowed);
            }
            Pattern::TupleVariant { path, fields, .. } => {
                let payload = self
                    .concrete_pattern_variant(path, expected)
                    .and_then(|variant| match variant {
                        VariantDefinition::Tuple(fields) => Some(fields),
                        _ => None,
                    })
                    .unwrap_or_default();
                for (field, ty) in fields.iter().zip(payload.iter()) {
                    self.pattern(field, ty, borrowed);
                }
            }
            Pattern::Record { path, fields, .. } => {
                let record_fields = match self.concrete_pattern_variant(path, expected) {
                    Some(VariantDefinition::Record(fields)) => Some(fields),
                    _ => {
                        if let Type::Named { name, arguments } = expected {
                            self.types.get(name).map(|definition| {
                                let substitutions = definition
                                    .generic_parameters
                                    .iter()
                                    .cloned()
                                    .zip(arguments.iter().cloned())
                                    .collect();
                                definition
                                    .fields
                                    .iter()
                                    .map(|(name, ty)| (name.clone(), ty.substitute(&substitutions)))
                                    .collect()
                            })
                        } else {
                            None
                        }
                    }
                };
                for (name, pattern) in fields {
                    let field_type = record_fields
                        .as_ref()
                        .and_then(|fields| fields.get(name))
                        .cloned()
                        .unwrap_or(Type::Unknown);
                    self.pattern(pattern, &field_type, borrowed);
                }
            }
            Pattern::Wildcard { .. }
            | Pattern::Literal { .. }
            | Pattern::None { .. }
            | Pattern::Path { .. } => {}
        }
    }

    fn concrete_pattern_variant(
        &self,
        path: &[String],
        expected: &Type,
    ) -> Option<VariantDefinition> {
        let variant = path.last()?;
        let Type::Named { name, arguments } = expected else {
            return None;
        };
        let definition = self.types.get(name)?;
        let substitutions = definition
            .generic_parameters
            .iter()
            .cloned()
            .zip(arguments.iter().cloned())
            .collect();
        match definition.variants.get(variant)? {
            VariantDefinition::Unit => Some(VariantDefinition::Unit),
            VariantDefinition::Tuple(fields) => Some(VariantDefinition::Tuple(
                fields
                    .iter()
                    .map(|ty| ty.substitute(&substitutions))
                    .collect(),
            )),
            VariantDefinition::Record(fields) => Some(VariantDefinition::Record(
                fields
                    .iter()
                    .map(|(name, ty)| (name.clone(), ty.substitute(&substitutions)))
                    .collect(),
            )),
        }
    }
}
