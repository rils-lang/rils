use super::*;
use crate::ast::EnumVariant;
use rils_frontend::copy_types::CopyTypes;

/// Rebuild capabilities from the verified declaration table, never mutable runtime flags.
pub(super) fn verify(module: &BytecodeModule) -> Result<(), BytecodeError> {
    let mut declarations = CopyTypes::default();
    for ty in &module.types {
        let (name, parameters, fields) = match ty {
            RuntimeType::Struct(definition) => (
                &definition.name,
                &definition.generic_parameters,
                definition
                    .fields
                    .iter()
                    .map(|field| field.type_annotation.clone())
                    .collect::<Vec<_>>(),
            ),
            RuntimeType::Enum(definition) => (
                &definition.name,
                &definition.generic_parameters,
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
            ),
        };
        declarations.define(
            name.clone(),
            parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .collect(),
            fields,
        );
    }
    let implementations = module
        .trait_implementations
        .iter()
        .filter(|implementation| implementation.trait_name == "Copy")
        .collect::<Vec<_>>();
    for implementation in &implementations {
        if !declarations.implement(&implementation.target) {
            return Err(BytecodeError::new(
                "Copy implementation references an unknown type",
                Span::default(),
            ));
        }
    }
    // Concrete external nominal declarations are linked against the actual host
    // context before starting the VM, including when no import is called yet.
    let mut hosts = HashSet::new();
    for import in &module.imports {
        for ty in import
            .signature
            .parameters
            .iter()
            .flatten()
            .chain([&import.signature.return_type])
        {
            imported_types(ty, &mut hosts);
        }
    }
    for ty in &module.types {
        match ty {
            RuntimeType::Struct(definition) => {
                for field in &definition.fields {
                    imported_types(&field.type_annotation, &mut hosts);
                }
            }
            RuntimeType::Enum(definition) => {
                for variant in &definition.variants {
                    match variant {
                        EnumVariant::Unit { .. } => {}
                        EnumVariant::Tuple { fields, .. } => {
                            for field in fields {
                                imported_types(field, &mut hosts);
                            }
                        }
                        EnumVariant::Record { fields, .. } => {
                            for field in fields {
                                imported_types(&field.type_annotation, &mut hosts);
                            }
                        }
                    }
                }
            }
        }
    }
    hosts.retain(|name| {
        !module.types.iter().any(|ty| match ty {
            RuntimeType::Struct(definition) => &definition.name == name,
            RuntimeType::Enum(definition) => &definition.name == name,
        }) && rils_builtins::builtin(name).is_none()
    });
    for implementation in implementations {
        let parameters = module
            .types
            .iter()
            .find_map(|ty| match ty {
                RuntimeType::Struct(definition) if definition.name == implementation.target => {
                    Some(&definition.generic_parameters)
                }
                RuntimeType::Enum(definition) if definition.name == implementation.target => {
                    Some(&definition.generic_parameters)
                }
                _ => None,
            })
            .expect("Copy target was checked");
        let ty = Type::Named {
            name: implementation.target.clone(),
            arguments: parameters
                .iter()
                .map(|parameter| Type::Variable(parameter.name.clone()))
                .collect(),
        };
        // An imported source/host enum still has to validate its declared fields.
        hosts.remove(&implementation.target);
        if !declarations.fields_are_copy(&ty, &hosts) {
            return Err(BytecodeError::new(
                format!("`{ty}` cannot implement Copy because it contains non-Copy fields"),
                Span::default(),
            ));
        }
    }
    Ok(())
}

fn imported_types(ty: &Type, names: &mut HashSet<String>) {
    match ty {
        Type::Named { name, arguments } if arguments.is_empty() => {
            names.insert(name.clone());
        }
        Type::Named { arguments, .. } | Type::Tuple(arguments) => {
            for argument in arguments {
                imported_types(argument, names);
            }
        }
        Type::Option(item)
        | Type::Reference { inner: item, .. }
        | Type::Array { element: item, .. } => imported_types(item, names),
        Type::Result(ok, error) => {
            imported_types(ok, names);
            imported_types(error, names);
        }
        _ => {}
    }
}
