//! Retain declaration identities when signatures cross into executable storage.

use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn signature_type(&self, ty: &Type) -> Type {
        let module = self
            .namespace
            .split("::")
            .filter(|segment| !segment.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        let ty = self.declaration_types.resolve(ty, &module);
        self.canonical_signature_type(&ty)
    }

    fn canonical_signature_type(&self, ty: &Type) -> Type {
        let child = |ty: &Type| Box::new(self.canonical_signature_type(ty));
        let children = |types: &[Type]| {
            types
                .iter()
                .map(|ty| self.canonical_signature_type(ty))
                .collect()
        };
        match ty {
            Type::Named { name, arguments } => {
                if name == "Self"
                    && arguments.is_empty()
                    && let Some(ty) = &self.self_type
                {
                    return self.signature_type(ty);
                }
                let name = self
                    .symbol_id(self.types, name)
                    .map(|id| self.canonical_type_path(id).join("::"))
                    .unwrap_or_else(|| name.clone());
                Type::Named {
                    name,
                    arguments: children(arguments),
                }
            }
            Type::Option(item) => Type::Option(child(item)),
            Type::Result(ok, error) => Type::Result(child(ok), child(error)),
            Type::Tuple(items) => Type::Tuple(children(items)),
            Type::Slice(item) => Type::Slice(child(item)),
            Type::Array { element, length } => Type::Array {
                element: child(element),
                length: *length,
            },
            Type::ArrayParameter { element, length } => Type::ArrayParameter {
                element: child(element),
                length: length.clone(),
            },
            Type::Reference { mutable, inner } => Type::Reference {
                mutable: *mutable,
                inner: child(inner),
            },
            Type::Function {
                parameters,
                return_type,
            } => Type::Function {
                parameters: parameters.as_ref().map(|types| children(types)),
                return_type: child(return_type),
            },
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => Type::Associated {
                base: child(base),
                trait_name: trait_name.clone(),
                name: name.clone(),
                arguments: children(arguments),
            },
            Type::BoundVariable { name, bounds } => Type::BoundVariable {
                name: name.clone(),
                bounds: children(bounds),
            },
            _ => ty.clone(),
        }
    }
}

pub(super) fn resolve_field_types(
    definitions: &mut [HirTypeDefinition],
    resolver: &rils_frontend::semantic::DeclarationTypeResolver,
) {
    for definition in definitions {
        let name = match definition {
            HirTypeDefinition::Struct { name, .. } | HirTypeDefinition::Enum { name, .. } => name,
        };
        let module = name.rsplit_once("::").map_or(Vec::new(), |(module, _)| {
            module.split("::").map(str::to_owned).collect()
        });
        match definition {
            HirTypeDefinition::Struct { fields, .. } => {
                for field in fields {
                    field.type_annotation = resolver.resolve(&field.type_annotation, &module);
                }
            }
            HirTypeDefinition::Enum { variants, .. } => {
                for variant in variants {
                    match variant {
                        EnumVariant::Tuple { fields, .. } => {
                            for field in fields {
                                *field = resolver.resolve(field, &module);
                            }
                        }
                        EnumVariant::Record { fields, .. } => {
                            for field in fields {
                                field.type_annotation =
                                    resolver.resolve(&field.type_annotation, &module);
                            }
                        }
                        EnumVariant::Unit { .. } => {}
                    }
                }
            }
        }
    }
}
