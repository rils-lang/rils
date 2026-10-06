//! Resolve runtime declarations and validate explicit source type paths.

use super::*;

pub(in crate::interpreter) fn expand_source_type(
    ty: &Type,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Type, RuntimeError> {
    if let Some(path) = environment.borrow().inaccessible_type(ty) {
        return Err(RuntimeError::new(
            format!("type path `{path}` is private"),
            span,
        ));
    }
    expand_type_aliases(ty, environment, span)
}

pub(in crate::interpreter) fn expand_type_aliases(
    ty: &Type,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Type, RuntimeError> {
    fn resolve_type_path(environment: &EnvironmentRef, name: &str, span: Span) -> Option<Value> {
        if let Some(
            value @ (Value::StructType(_)
            | Value::EnumType(_)
            | Value::TraitType(_)
            | Value::TypeAlias(_)),
        ) = environment.borrow().get(name)
        {
            return Some(value);
        }
        let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
        let (environment, path) =
            super::super::execution::anchored_environment(&path, environment, span).ok()?;
        let (first, segments) = path.split_first()?;
        let mut value = environment.borrow().get(first)?;
        for segment in segments {
            let Value::Module(module) = value else {
                return None;
            };
            if !module.public.borrow().contains(segment) {
                return None;
            }
            value = module.members.borrow().get(segment)?;
        }
        Some(value)
    }

    fn expand(
        ty: &Type,
        environment: &EnvironmentRef,
        span: Span,
        stack: &mut Vec<String>,
    ) -> Result<Type, RuntimeError> {
        match ty {
            Type::Tuple(elements) => Ok(Type::Tuple(
                elements
                    .iter()
                    .map(|element| expand(element, environment, span, stack))
                    .collect::<Result<Vec<_>, _>>()?,
            )),
            Type::Array { element, length } => Ok(Type::Array {
                element: Box::new(expand(element, environment, span, stack)?),
                length: *length,
            }),
            Type::Option(inner) => Ok(Type::Option(Box::new(expand(
                inner,
                environment,
                span,
                stack,
            )?))),
            Type::Result(ok, error) => Ok(Type::Result(
                Box::new(expand(ok, environment, span, stack)?),
                Box::new(expand(error, environment, span, stack)?),
            )),
            Type::Reference { mutable, inner } => Ok(Type::Reference {
                mutable: *mutable,
                inner: Box::new(expand(inner, environment, span, stack)?),
            }),
            Type::Function {
                parameters,
                return_type,
            } => Ok(Type::Function {
                parameters: parameters
                    .as_ref()
                    .map(|parameters| {
                        parameters
                            .iter()
                            .map(|parameter| expand(parameter, environment, span, stack))
                            .collect()
                    })
                    .transpose()?,
                return_type: Box::new(expand(return_type, environment, span, stack)?),
            }),
            Type::Associated {
                base,
                trait_name,
                name,
                arguments,
            } => {
                let base = expand(base, environment, span, stack)?;
                let arguments = arguments
                    .iter()
                    .map(|argument| expand(argument, environment, span, stack))
                    .collect::<Result<Vec<_>, _>>()?;
                if trait_name.as_deref() == Some("IntoIterator")
                    && name == "IntoIter"
                    && arguments.is_empty()
                    && type_implements_trait(&base, "Iterator", environment)
                {
                    return Ok(base);
                }
                let lookup_trait = if trait_name.as_deref() == Some("IntoIterator")
                    && name == "Item"
                    && arguments.is_empty()
                    && type_implements_trait(&base, "Iterator", environment)
                {
                    Some("Iterator")
                } else {
                    trait_name.as_deref()
                };
                let Type::Named {
                    name: target_name, ..
                } = &base
                else {
                    return Ok(Type::Associated {
                        base: Box::new(base),
                        trait_name: trait_name.clone(),
                        name: name.clone(),
                        arguments,
                    });
                };
                if lookup_trait == Some("Iterator")
                    && name == "Item"
                    && let Some(item) =
                        rils_frontend::standard_library::builtin_iterator_item_type(&base)
                {
                    return Ok(item);
                }
                if trait_name.as_deref() == Some("IntoIterator")
                    && name == "Item"
                    && let Some(item) =
                        rils_frontend::standard_library::builtin_into_iterator_item_type(&base)
                {
                    return Ok(item);
                }
                let definitions = match environment.borrow().get(target_name) {
                    Some(Value::StructType(definition)) => definition
                        .associated_types
                        .borrow()
                        .iter()
                        .filter(|(implemented_trait, _)| {
                            lookup_trait
                                .is_none_or(|expected| expected == implemented_trait.as_str())
                        })
                        .filter_map(|(_, items)| items.get(name).cloned())
                        .collect::<Vec<_>>(),
                    Some(Value::EnumType(definition)) => definition
                        .associated_types
                        .borrow()
                        .iter()
                        .filter(|(implemented_trait, _)| {
                            lookup_trait
                                .is_none_or(|expected| expected == implemented_trait.as_str())
                        })
                        .filter_map(|(_, items)| items.get(name).cloned())
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                if definitions.len() > 1 {
                    return Err(RuntimeError::new(
                        format!(
                            "associated type `{target_name}::{name}` is ambiguous; use `<{target_name} as Trait>::{name}`"
                        ),
                        span,
                    ));
                }
                let Some(alias) = definitions.into_iter().next() else {
                    if let Some(trait_name) = trait_name {
                        return Err(RuntimeError::new(
                            format!(
                                "type `{target_name}` has no associated type `{name}` from trait `{trait_name}`"
                            ),
                            span,
                        ));
                    }
                    return Ok(Type::Associated {
                        base: Box::new(base),
                        trait_name: trait_name.clone(),
                        name: name.clone(),
                        arguments,
                    });
                };
                if alias.generic_parameters.len() != arguments.len() {
                    return Err(RuntimeError::new(
                        format!(
                            "associated type `{target_name}::{name}` expects {} type argument(s), received {}",
                            alias.generic_parameters.len(),
                            arguments.len()
                        ),
                        span,
                    ));
                }
                let substitutions = alias
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .zip(arguments)
                    .collect::<HashMap<_, _>>();
                expand(
                    &alias.target.substitute(&substitutions),
                    environment,
                    span,
                    stack,
                )
            }
            Type::Named { name, arguments } => {
                let arguments = arguments
                    .iter()
                    .map(|argument| expand(argument, environment, span, stack))
                    .collect::<Result<Vec<_>, _>>()?;
                let resolved = resolve_type_path(environment, name, span);
                match resolved {
                    Some(Value::StructType(definition)) => {
                        return Ok(Type::Named {
                            name: definition.name.clone(),
                            arguments,
                        });
                    }
                    Some(Value::EnumType(definition)) => {
                        return Ok(Type::Named {
                            name: definition.name.clone(),
                            arguments,
                        });
                    }
                    Some(Value::HostType(definition)) => {
                        return Ok(Type::Named {
                            name: definition.name.clone(),
                            arguments,
                        });
                    }
                    _ => {}
                }
                let Some(Value::TypeAlias(alias)) = resolved else {
                    if environment.borrow().is_declared_type(name) {
                        return Ok(Type::Named {
                            name: name.clone(),
                            arguments,
                        });
                    }
                    let segments = name.split("::").collect::<Vec<_>>();
                    if let [base, associated] = segments.as_slice() {
                        return expand(
                            &Type::Associated {
                                base: Box::new(Type::named(*base)),
                                trait_name: None,
                                name: (*associated).into(),
                                arguments,
                            },
                            environment,
                            span,
                            stack,
                        );
                    }
                    return Ok(Type::Named {
                        name: name.clone(),
                        arguments,
                    });
                };
                if alias.generic_parameters.len() != arguments.len() {
                    return Err(RuntimeError::new(
                        format!(
                            "type alias `{name}` expects {} type argument(s), received {}",
                            alias.generic_parameters.len(),
                            arguments.len()
                        ),
                        span,
                    ));
                }
                if stack.contains(name) {
                    return Err(RuntimeError::new(
                        format!("recursive type alias `{name}`"),
                        span,
                    ));
                }
                let substitutions = alias
                    .generic_parameters
                    .iter()
                    .zip(&arguments)
                    .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                    .collect::<HashMap<_, _>>();
                for (parameter, argument) in alias.generic_parameters.iter().zip(&arguments) {
                    if matches!(
                        argument,
                        Type::Unknown | Type::Variable(_) | Type::BoundVariable { .. }
                    ) {
                        continue;
                    }
                    for bound in &parameter.bounds {
                        if !type_implements_trait_bound(
                            argument,
                            &bound.substitute(&substitutions),
                            None,
                            environment,
                        ) {
                            return Err(RuntimeError::new(
                                format!(
                                    "type `{argument}` does not implement required trait `{bound}` for type alias `{name}`"
                                ),
                                span,
                            ));
                        }
                    }
                }
                let substitutions = alias
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .zip(arguments)
                    .collect::<HashMap<_, _>>();
                stack.push(name.clone());
                let result = expand(
                    &alias.target.substitute(&substitutions),
                    environment,
                    span,
                    stack,
                );
                stack.pop();
                result
            }
            other => Ok(other.clone()),
        }
    }

    let ty = environment.borrow().resolve_declaration_type(ty);
    expand(&ty, environment, span, &mut Vec::new())
}
