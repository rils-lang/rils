//! Resolve standard-library paths through the shared declaration catalog.

use crate::Type;

pub(super) fn storage_type(ty: &Type) -> Type {
    let child = |ty: &Type| Box::new(storage_type(ty));
    let children = |types: &[Type]| types.iter().map(storage_type).collect();
    match ty {
        Type::Named { name, arguments } => Type::Named {
            name: rils_frontend::standard_library::builtin_type_name(name)
                .unwrap_or(name)
                .to_owned(),
            arguments: children(arguments),
        },
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
