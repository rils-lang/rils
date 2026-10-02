//! Retain declaration identities when signatures cross into executable storage.

use super::*;

impl FunctionLowerer<'_> {
    pub(super) fn signature_type(&self, ty: &Type) -> Type {
        let child = |ty: &Type| Box::new(self.signature_type(ty));
        let children = |types: &[Type]| types.iter().map(|ty| self.signature_type(ty)).collect();
        match ty {
            Type::Named { name, arguments } => {
                if name == "Self"
                    && arguments.is_empty()
                    && let Some(ty) = &self.self_type
                {
                    return self.signature_type(&Type::named(ty));
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
