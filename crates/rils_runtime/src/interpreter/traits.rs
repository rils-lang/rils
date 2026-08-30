use super::*;

pub(super) fn implemented_traits(
    value: &Value,
) -> Option<&std::cell::RefCell<std::collections::HashSet<String>>> {
    match value {
        Value::StructType(definition) => Some(&definition.implemented_traits),
        Value::EnumType(definition) => Some(&definition.implemented_traits),
        _ => None,
    }
}

pub(super) fn validate_trait_implementation(
    definition: &TraitType,
    associated_types: &HashMap<String, TypeAliasType>,
    methods: &[crate::ast::ImplMethod],
    target: &Type,
    span: Span,
) -> Result<(), RuntimeError> {
    for required in &definition.methods {
        let implementation = methods
            .iter()
            .find(|method| method.name == required.name)
            .ok_or_else(|| {
                RuntimeError::new(
                    format!(
                        "impl of trait `{}` is missing method `{}`",
                        definition.name, required.name
                    ),
                    span,
                )
            })?;
        validate_trait_method_signature(required, implementation, target, associated_types)?;
    }
    if let Some(extra) = methods.iter().find(|method| {
        !definition
            .methods
            .iter()
            .any(|item| item.name == method.name)
    }) {
        return Err(RuntimeError::new(
            format!(
                "method `{}` is not a member of trait `{}`",
                extra.name, definition.name
            ),
            extra.span,
        ));
    }
    Ok(())
}

pub(super) fn validate_trait_method_signature(
    required: &TraitMethod,
    implementation: &crate::ast::ImplMethod,
    target: &Type,
    associated_types: &HashMap<String, TypeAliasType>,
) -> Result<(), RuntimeError> {
    if required.generic_parameters.len() != implementation.generic_parameters.len()
        || required.parameters.len() != implementation.parameters.len()
        || required
            .generic_parameters
            .iter()
            .zip(&implementation.generic_parameters)
            .any(|(required, actual)| {
                required.name != actual.name || required.bounds != actual.bounds
            })
    {
        return Err(RuntimeError::new(
            format!(
                "method `{}` does not match its trait signature",
                required.name
            ),
            implementation.span,
        ));
    }
    for (required_parameter, actual_parameter) in
        required.parameters.iter().zip(&implementation.parameters)
    {
        if required_parameter.name == "self" && actual_parameter.name != "self" {
            return Err(RuntimeError::new(
                format!("method `{}` must take `self`", required.name),
                implementation.span,
            ));
        }
        let expected = required_parameter
            .type_annotation
            .as_ref()
            .map(|value| substitute_associated(value, target, associated_types));
        let actual = actual_parameter
            .type_annotation
            .as_ref()
            .map(|value| substitute_associated(value, target, associated_types))
            .or_else(|| (actual_parameter.name == "self").then(|| target.clone()));
        let expected =
            expected.or_else(|| (required_parameter.name == "self").then(|| target.clone()));
        if !optional_types_compatible(expected.as_ref(), actual.as_ref()) {
            return Err(RuntimeError::new(
                format!(
                    "parameter `{}` of method `{}` does not match the trait",
                    actual_parameter.name, required.name
                ),
                actual_parameter.span,
            ));
        }
    }
    let expected_return = required
        .return_type
        .as_ref()
        .map(|value| substitute_associated(value, target, associated_types))
        .unwrap_or(Type::Unit);
    let actual_return = implementation
        .return_type
        .as_ref()
        .map(|value| substitute_associated(value, target, associated_types))
        .unwrap_or(Type::Unit);
    if !trait_types_compatible(&expected_return, &actual_return) {
        return Err(RuntimeError::new(
            format!(
                "return type of method `{}` does not match the trait",
                required.name
            ),
            implementation.span,
        ));
    }
    Ok(())
}

fn optional_types_compatible(expected: Option<&Type>, actual: Option<&Type>) -> bool {
    match (expected, actual) {
        (None, None) => true,
        (Some(expected), Some(actual)) => trait_types_compatible(expected, actual),
        _ => false,
    }
}

fn trait_types_compatible(expected: &Type, actual: &Type) -> bool {
    match (expected, actual) {
        (Type::Unknown | Type::Variable(_), _) => true,
        (Type::Option(expected), Type::Option(actual)) => trait_types_compatible(expected, actual),
        (Type::Result(expected_ok, expected_error), Type::Result(actual_ok, actual_error)) => {
            trait_types_compatible(expected_ok, actual_ok)
                && trait_types_compatible(expected_error, actual_error)
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
        ) => expected_mutable == actual_mutable && trait_types_compatible(expected, actual),
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
            (match (expected_parameters, actual_parameters) {
                (Some(expected), Some(actual)) => {
                    expected.len() == actual.len()
                        && expected
                            .iter()
                            .zip(actual)
                            .all(|(expected, actual)| trait_types_compatible(expected, actual))
                }
                (None, _) => true,
                _ => false,
            }) && trait_types_compatible(expected_return, actual_return)
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
        ) => {
            nominal_names_compatible(expected_name, actual_name)
                && expected_arguments.len() == actual_arguments.len()
                && expected_arguments
                    .iter()
                    .zip(actual_arguments)
                    .all(|(expected, actual)| trait_types_compatible(expected, actual))
        }
        _ => expected == actual,
    }
}

fn nominal_names_compatible(expected: &str, actual: &str) -> bool {
    expected == actual
        || matches!(
            (expected.rsplit("::").next(), actual.rsplit("::").next()),
            (Some("Formatter"), Some("Formatter")) | (Some("FormatError"), Some("FormatError"))
        )
}

pub(super) fn substitute_self(value: &Type, target: &Type) -> Type {
    match value {
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| substitute_self(element, target))
                .collect(),
        ),
        Type::Array { element, length } => Type::Array {
            element: Box::new(substitute_self(element, target)),
            length: *length,
        },
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => target.clone(),
        Type::Option(inner) => Type::Option(Box::new(substitute_self(inner, target))),
        Type::Result(ok, error) => Type::Result(
            Box::new(substitute_self(ok, target)),
            Box::new(substitute_self(error, target)),
        ),
        Type::Reference { mutable, inner } => Type::Reference {
            mutable: *mutable,
            inner: Box::new(substitute_self(inner, target)),
        },
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters.as_ref().map(|parameters| {
                parameters
                    .iter()
                    .map(|parameter| substitute_self(parameter, target))
                    .collect()
            }),
            return_type: Box::new(substitute_self(return_type, target)),
        },
        Type::Named { name, arguments } => Type::Named {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_self(argument, target))
                .collect(),
        },
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(substitute_self(base, target)),
            trait_name: trait_name.clone(),
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_self(argument, target))
                .collect(),
        },
        other => other.clone(),
    }
}

pub(super) fn substitute_associated(
    value: &Type,
    target: &Type,
    associated_types: &HashMap<String, TypeAliasType>,
) -> Type {
    match value {
        Type::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .map(|element| substitute_associated(element, target, associated_types))
                .collect(),
        ),
        Type::Array { element, length } => Type::Array {
            element: Box::new(substitute_associated(element, target, associated_types)),
            length: *length,
        },
        Type::Associated {
            base,
            trait_name: _,
            name,
            arguments,
        } if matches!(base.as_ref(), Type::Named { name, arguments } if name == "Self" && arguments.is_empty()) =>
        {
            let Some(alias) = associated_types.get(name) else {
                return Type::Unknown;
            };
            if alias.generic_parameters.len() != arguments.len() {
                return Type::Unknown;
            }
            let substitutions = alias
                .generic_parameters
                .iter()
                .map(|parameter| parameter.name.clone())
                .zip(
                    arguments
                        .iter()
                        .map(|argument| substitute_associated(argument, target, associated_types)),
                )
                .collect::<HashMap<_, _>>();
            substitute_self(&alias.target.substitute(&substitutions), target)
        }
        Type::Option(inner) => Type::Option(Box::new(substitute_associated(
            inner,
            target,
            associated_types,
        ))),
        Type::Result(ok, error) => Type::Result(
            Box::new(substitute_associated(ok, target, associated_types)),
            Box::new(substitute_associated(error, target, associated_types)),
        ),
        Type::Reference { mutable, inner } => Type::Reference {
            mutable: *mutable,
            inner: Box::new(substitute_associated(inner, target, associated_types)),
        },
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters.as_ref().map(|parameters| {
                parameters
                    .iter()
                    .map(|parameter| substitute_associated(parameter, target, associated_types))
                    .collect()
            }),
            return_type: Box::new(substitute_associated(return_type, target, associated_types)),
        },
        Type::Named { name, arguments } if name == "Self" && arguments.is_empty() => target.clone(),
        Type::Named { name, arguments } => Type::Named {
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_associated(argument, target, associated_types))
                .collect(),
        },
        Type::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(substitute_associated(base, target, associated_types)),
            trait_name: trait_name.clone(),
            name: name.clone(),
            arguments: arguments
                .iter()
                .map(|argument| substitute_associated(argument, target, associated_types))
                .collect(),
        },
        other => other.clone(),
    }
}
