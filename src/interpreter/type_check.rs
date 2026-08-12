use super::*;

pub(super) fn validate_named_fields(
    definitions: &[NamedField],
    mut values: HashMap<String, Value>,
    span: Span,
    type_name: &str,
    substitutions: &HashMap<String, Type>,
) -> Result<HashMap<String, Value>, RuntimeError> {
    let mut validated = HashMap::new();
    for field in definitions {
        let value = values.remove(&field.name).ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "missing field `{}` when constructing `{type_name}`",
                    field.name
                ),
                span,
            )
        })?;
        let expected = field.type_annotation.substitute(substitutions);
        let value = apply_type(
            Some(&expected),
            &value,
            span,
            &format!("{type_name}.{}", field.name),
        )?;
        validated.insert(field.name.clone(), value);
    }
    if let Some(unexpected) = values.keys().next() {
        return Err(RuntimeError::new(
            format!("unknown field `{unexpected}` for `{type_name}`"),
            span,
        ));
    }
    Ok(validated)
}

pub(super) fn generic_substitutions(parameters: &[GenericParameter]) -> HashMap<String, Type> {
    parameters
        .iter()
        .map(|parameter| (parameter.name.clone(), Type::Unknown))
        .collect()
}

pub(super) fn generic_arguments(
    parameters: &[GenericParameter],
    substitutions: &HashMap<String, Type>,
) -> Vec<Type> {
    parameters
        .iter()
        .map(|parameter| {
            substitutions
                .get(&parameter.name)
                .cloned()
                .unwrap_or(Type::Unknown)
        })
        .collect()
}

pub(super) fn infer_named_fields(
    definitions: &[NamedField],
    values: &HashMap<String, Value>,
    substitutions: &mut HashMap<String, Type>,
    span: Span,
    type_name: &str,
) -> Result<(), RuntimeError> {
    for field in definitions {
        let value = values.get(&field.name).ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "missing field `{}` when constructing `{type_name}`",
                    field.name
                ),
                span,
            )
        })?;
        infer_type_from_value(&field.type_annotation, value, substitutions)
            .map_err(|message| RuntimeError::new(message, span))?;
    }
    Ok(())
}

pub(super) fn infer_type_from_value(
    expected: &Type,
    value: &Value,
    substitutions: &mut HashMap<String, Type>,
) -> Result<(), String> {
    match (expected, value) {
        (Type::Variable(name), value) => {
            let actual = Type::of_value(value).unwrap_or(Type::Unknown);
            bind_type_variable(name, actual, substitutions)
        }
        (
            Type::Reference {
                mutable: expected_mutable,
                inner,
            },
            Value::Reference(reference),
        ) if !*expected_mutable || reference.mutable => {
            let value = reference.read()?;
            infer_type_from_value(inner, &value, substitutions)
        }
        (
            Type::Option(inner),
            Value::Option {
                value: Some(value), ..
            },
        ) => infer_type_from_value(inner, value, substitutions),
        (
            Type::Option(inner),
            Value::Option {
                value: None,
                element_type: Some(actual),
            },
        ) => infer_type_from_type(inner, actual, substitutions),
        (
            Type::Result(ok, error),
            Value::Result {
                value,
                ok_type,
                error_type,
            },
        ) => {
            match value {
                Ok(value) => infer_type_from_value(ok, value, substitutions)?,
                Err(value) => infer_type_from_value(error, value, substitutions)?,
            }
            if let Some(actual) = ok_type {
                infer_type_from_type(ok, actual, substitutions)?;
            }
            if let Some(actual) = error_type {
                infer_type_from_type(error, actual, substitutions)?;
            }
            Ok(())
        }
        (Type::Named { name, arguments }, Value::Struct(instance))
            if instance.type_definition.name == *name =>
        {
            infer_type_arguments(arguments, &instance.type_arguments, substitutions)
        }
        (Type::Named { name, arguments }, Value::Enum(instance))
            if instance.type_definition.name == *name =>
        {
            infer_type_arguments(arguments, &instance.type_arguments, substitutions)
        }
        (
            expected @ Type::Function { .. },
            value @ (Value::Function(_)
            | Value::NativeFunction(_)
            | Value::VariantConstructor(_)
            | Value::BoundMethod(_)),
        ) => {
            let actual = Type::of_value(value).unwrap_or(Type::opaque_function());
            infer_type_from_type(expected, &actual, substitutions)
        }
        (Type::Unknown, _) => Ok(()),
        (expected, value) if expected.accepts(value) => Ok(()),
        (expected, value) => Err(format!(
            "type mismatch: expected {expected}, found {}",
            value.type_name()
        )),
    }
}

pub(super) fn infer_type_from_type(
    expected: &Type,
    actual: &Type,
    substitutions: &mut HashMap<String, Type>,
) -> Result<(), String> {
    match (expected, actual) {
        (Type::Variable(name), actual) => bind_type_variable(name, actual.clone(), substitutions),
        (Type::Option(expected), Type::Option(actual)) => {
            infer_type_from_type(expected, actual, substitutions)
        }
        (Type::Result(expected_ok, expected_error), Type::Result(actual_ok, actual_error)) => {
            infer_type_from_type(expected_ok, actual_ok, substitutions)?;
            infer_type_from_type(expected_error, actual_error, substitutions)
        }
        (
            Type::Reference {
                mutable: expected_mutable,
                inner: expected,
            },
            Type::Reference {
                mutable: actual_mutable,
                inner: actual,
            },
        ) if !*expected_mutable || *actual_mutable => {
            infer_type_from_type(expected, actual, substitutions)
        }
        (
            Type::Function {
                parameters: expected_parameters,
                return_type: expected_return,
            },
            Type::Function {
                parameters: actual_parameters,
                return_type: actual_return,
            },
        ) => {
            if let (Some(expected_parameters), Some(actual_parameters)) =
                (expected_parameters, actual_parameters)
            {
                infer_type_arguments(expected_parameters, actual_parameters, substitutions)?;
            }
            infer_type_from_type(expected_return, actual_return, substitutions)
        }
        (
            Type::Named {
                name: expected_name,
                arguments: expected_arguments,
            },
            Type::Named {
                name: actual_name,
                arguments: actual_arguments,
            },
        ) if expected_name == actual_name => {
            infer_type_arguments(expected_arguments, actual_arguments, substitutions)
        }
        (Type::Unknown, _) | (_, Type::Unknown) => Ok(()),
        (expected, actual) if expected == actual => Ok(()),
        (expected, actual) => Err(format!(
            "generic type mismatch: expected {expected}, found {actual}"
        )),
    }
}

pub(super) fn infer_type_arguments(
    expected: &[Type],
    actual: &[Type],
    substitutions: &mut HashMap<String, Type>,
) -> Result<(), String> {
    if expected.len() != actual.len() {
        return Err(format!(
            "generic arity mismatch: expected {} arguments, found {}",
            expected.len(),
            actual.len()
        ));
    }
    for (expected, actual) in expected.iter().zip(actual) {
        infer_type_from_type(expected, actual, substitutions)?;
    }
    Ok(())
}

pub(super) fn bind_type_variable(
    name: &str,
    actual: Type,
    substitutions: &mut HashMap<String, Type>,
) -> Result<(), String> {
    let current = substitutions.get(name).cloned().unwrap_or(Type::Unknown);
    let merged = merge_types(&current, &actual).ok_or_else(|| {
        format!("generic parameter `{name}` inferred as both {current} and {actual}")
    })?;
    substitutions.insert(name.to_owned(), merged);
    Ok(())
}

pub(super) fn validate_generic_bounds(
    parameters: &[GenericParameter],
    substitutions: &HashMap<String, Type>,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<(), RuntimeError> {
    for parameter in parameters {
        let actual = substitutions
            .get(&parameter.name)
            .cloned()
            .unwrap_or(Type::Unknown);
        for bound in &parameter.bounds {
            let trait_value = environment
                .borrow()
                .get(bound)
                .ok_or_else(|| RuntimeError::new(format!("unknown trait bound `{bound}`"), span))?;
            if !matches!(trait_value, Value::TraitType(_)) {
                return Err(RuntimeError::new(format!("`{bound}` is not a trait"), span));
            }
            if actual == Type::Unknown {
                continue;
            }
            if !type_implements_trait(&actual, bound, environment) {
                return Err(RuntimeError::new(
                    format!(
                        "type `{actual}` does not implement required trait `{bound}` for `{}`",
                        parameter.name
                    ),
                    span,
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn type_implements_trait(
    actual: &Type,
    trait_name: &str,
    environment: &EnvironmentRef,
) -> bool {
    match trait_name {
        "IntoIterator" if matches!(actual, Type::Array { .. }) => true,
        "IntoIterator" if matches!(actual, Type::Named { name, arguments } if name == "Vec" && arguments.len() == 1) => {
            true
        }
        "Iterator" if matches!(actual, Type::Named { name, arguments } if name == "SequenceIterator" && arguments.len() == 1) => {
            true
        }
        "Iterator" | "IntoIterator" if matches!(actual, Type::Named { name, arguments } if name == "Range" && arguments.is_empty()) => {
            true
        }
        "Copy" => type_is_copy(actual, environment),
        "Clone" => type_is_clone(actual, environment),
        _ => {
            let Type::Named { name, .. } = actual else {
                return false;
            };
            match environment.borrow().get(name) {
                Some(Value::StructType(definition)) => {
                    definition.implemented_traits.borrow().contains(trait_name)
                }
                Some(Value::EnumType(definition)) => {
                    definition.implemented_traits.borrow().contains(trait_name)
                }
                _ => false,
            }
        }
    }
}

pub(super) fn expand_type_aliases(
    ty: &Type,
    environment: &EnvironmentRef,
    span: Span,
) -> Result<Type, RuntimeError> {
    fn resolve_type_path(environment: &EnvironmentRef, name: &str, span: Span) -> Option<Value> {
        let path = name.split("::").map(str::to_owned).collect::<Vec<_>>();
        let (environment, path) =
            super::execution::anchored_environment(&path, environment, span).ok()?;
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
                let definitions = match environment.borrow().get(target_name) {
                    Some(Value::StructType(definition)) => definition
                        .associated_types
                        .borrow()
                        .iter()
                        .filter(|(implemented_trait, _)| {
                            trait_name
                                .as_ref()
                                .is_none_or(|expected| expected == *implemented_trait)
                        })
                        .filter_map(|(_, items)| items.get(name).cloned())
                        .collect::<Vec<_>>(),
                    Some(Value::EnumType(definition)) => definition
                        .associated_types
                        .borrow()
                        .iter()
                        .filter(|(implemented_trait, _)| {
                            trait_name
                                .as_ref()
                                .is_none_or(|expected| expected == *implemented_trait)
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
                for (parameter, argument) in alias.generic_parameters.iter().zip(&arguments) {
                    if matches!(argument, Type::Unknown | Type::Variable(_)) {
                        continue;
                    }
                    for bound in &parameter.bounds {
                        if !type_implements_trait(argument, bound, environment) {
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

    expand(ty, environment, span, &mut Vec::new())
}

fn type_is_copy(actual: &Type, environment: &EnvironmentRef) -> bool {
    match actual {
        Type::Unit
        | Type::Bool
        | Type::Integer(_)
        | Type::Float(_)
        | Type::IntegerVariable(_)
        | Type::FloatVariable(_)
        | Type::Char
        | Type::Reference { .. } => true,
        Type::Function { .. } => true,
        Type::String | Type::Unknown | Type::Variable(_) | Type::Associated { .. } => false,
        Type::Option(inner) => type_is_copy(inner, environment),
        Type::Result(ok, error) => {
            type_is_copy(ok, environment) && type_is_copy(error, environment)
        }
        Type::Tuple(elements) => elements.iter().all(|ty| type_is_copy(ty, environment)),
        Type::Array { element, .. } => type_is_copy(element, environment),
        Type::Named { name, arguments } => match environment.borrow().get(name) {
            Some(Value::StructType(definition)) => {
                if definition.implemented_traits.borrow().contains("Copy") {
                    return true;
                }
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .zip(arguments.iter().cloned())
                    .collect::<HashMap<_, _>>();
                definition.fields.iter().all(|field| {
                    type_is_copy(
                        &field.type_annotation.substitute(&substitutions),
                        environment,
                    )
                })
            }
            Some(Value::EnumType(definition)) => {
                if definition.implemented_traits.borrow().contains("Copy") {
                    return true;
                }
                let substitutions = definition
                    .generic_parameters
                    .iter()
                    .map(|parameter| parameter.name.clone())
                    .zip(arguments.iter().cloned())
                    .collect::<HashMap<_, _>>();
                definition.variants.iter().all(|variant| match variant {
                    EnumVariant::Unit { .. } => true,
                    EnumVariant::Tuple { fields, .. } => fields
                        .iter()
                        .all(|field| type_is_copy(&field.substitute(&substitutions), environment)),
                    EnumVariant::Record { fields, .. } => fields.iter().all(|field| {
                        type_is_copy(
                            &field.type_annotation.substitute(&substitutions),
                            environment,
                        )
                    }),
                })
            }
            _ => false,
        },
    }
}

fn type_is_clone(actual: &Type, environment: &EnvironmentRef) -> bool {
    match actual {
        Type::Unknown | Type::Variable(_) | Type::Associated { .. } => false,
        Type::Option(inner) => type_is_clone(inner, environment),
        Type::Result(ok, error) => {
            type_is_clone(ok, environment) && type_is_clone(error, environment)
        }
        Type::Tuple(elements) => elements.iter().all(|ty| type_is_clone(ty, environment)),
        Type::Array { element, .. } => type_is_clone(element, environment),
        Type::Named { name, .. } if name == "Vec" => true,
        Type::Named { name, .. } => matches!(
            environment.borrow().get(name),
            Some(Value::StructType(_) | Value::EnumType(_))
        ),
        _ => true,
    }
}

pub(super) fn check_arity(
    name: &str,
    minimum: usize,
    maximum: usize,
    actual: usize,
    span: Span,
) -> Result<(), RuntimeError> {
    if (minimum..=maximum).contains(&actual) {
        Ok(())
    } else if minimum == maximum {
        Err(RuntimeError::new(
            format!("`{name}` expects {minimum} arguments, received {actual}"),
            span,
        ))
    } else if maximum == usize::MAX {
        Err(RuntimeError::new(
            format!("`{name}` expects at least {minimum} argument(s), received {actual}"),
            span,
        ))
    } else if minimum == 0 {
        Err(RuntimeError::new(
            format!("`{name}` expects at most {maximum} argument(s), received {actual}"),
            span,
        ))
    } else {
        Err(RuntimeError::new(
            format!("`{name}` expects {minimum} to {maximum} arguments, received {actual}"),
            span,
        ))
    }
}

pub(super) fn apply_type(
    expected: Option<&crate::types::Type>,
    value: &Value,
    span: Span,
    subject: &str,
) -> Result<Value, RuntimeError> {
    if let Some(expected) = expected {
        return expected.constrain(value).ok_or_else(|| {
            RuntimeError::new(
                format!(
                    "type mismatch for `{subject}`: expected {expected}, found {}",
                    value.type_name()
                ),
                span,
            )
        });
    }
    Ok(value.clone())
}
