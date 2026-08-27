use std::collections::HashMap;

use crate::types::{FunctionSignature, Type};

pub fn standard_function_signature(name: &str) -> Option<FunctionSignature> {
    if let Some(declaration) = rils_builtins::builtin_function(name) {
        let signature = declaration
            .signature
            .expect("function declaration has a signature");
        let return_type = resolve_type_pattern(signature.result);
        return Some(if signature.variadic {
            FunctionSignature::variadic(return_type)
        } else {
            FunctionSignature::fixed(
                signature
                    .parameters
                    .iter()
                    .copied()
                    .map(resolve_type_pattern)
                    .collect(),
                return_type,
            )
        });
    }
    None
}

pub fn builtin_member_type(object: &Type, name: &str) -> Option<Type> {
    let (owner, self_type, mut generics) = builtin_owner(object)?;
    let member = rils_builtins::builtin_member(owner, name)?;
    let signature = member.signature?;
    for parameter in member.type_parameters {
        generics.insert(parameter, Type::Variable((*parameter).into()));
    }
    Some(Type::function(
        signature
            .parameters
            .iter()
            .copied()
            .map(|pattern| resolve_member_pattern(pattern, &self_type, &generics))
            .collect(),
        resolve_member_pattern(signature.result, &self_type, &generics),
    ))
}

pub fn builtin_trait_member_type(trait_name: &str, object: &Type, name: &str) -> Option<Type> {
    let member = rils_builtins::builtin_member(trait_name, name)?;
    let signature = member.signature?;
    let mut generics = HashMap::new();
    let item = match object {
        Type::Named { arguments, .. } => arguments.first().cloned().unwrap_or(Type::Unknown),
        _ => Type::Unknown,
    };
    generics.insert("T", item);
    for parameter in member.type_parameters {
        generics.insert(parameter, Type::Variable((*parameter).into()));
    }
    Some(Type::function(
        signature
            .parameters
            .iter()
            .copied()
            .map(|pattern| resolve_member_pattern(pattern, object, &generics))
            .collect(),
        resolve_member_pattern(signature.result, object, &generics),
    ))
}

pub fn builtin_receiver_mode(object: &Type, name: &str) -> Option<rils_builtins::ReceiverMode> {
    let (owner, _, _) = builtin_owner(object)?;
    rils_builtins::builtin_member(owner, name)?.receiver
}

/// Builds the type-erased ABI signature used when a built-in member is
/// lowered without retaining its concrete receiver type.
pub fn erased_builtin_member_signature(
    member: &rils_builtins::BuiltinMember,
) -> Option<FunctionSignature> {
    let signature = member.signature?;
    let receiver = match member.receiver? {
        rils_builtins::ReceiverMode::Owned => Type::Unknown,
        rils_builtins::ReceiverMode::Shared => Type::Reference {
            mutable: false,
            inner: Box::new(Type::Unknown),
        },
        rils_builtins::ReceiverMode::Mutable => Type::Reference {
            mutable: true,
            inner: Box::new(Type::Unknown),
        },
    };
    let generics = HashMap::new();
    let mut parameters = Vec::with_capacity(signature.parameters.len() + 1);
    parameters.push(receiver);
    parameters.extend(
        signature
            .parameters
            .iter()
            .copied()
            .map(|pattern| resolve_member_pattern(pattern, &Type::Unknown, &generics)),
    );
    Some(FunctionSignature::fixed(
        parameters,
        resolve_member_pattern(signature.result, &Type::Unknown, &generics),
    ))
}

pub fn erased_builtin_import_signature(import: &str) -> Option<FunctionSignature> {
    let mut signatures = rils_builtins::BUILTINS
        .iter()
        .flat_map(|declaration| declaration.members)
        .filter(|member| {
            member
                .builtin_id
                .and_then(rils_builtins::BuiltinId::bytecode_import)
                == Some(import)
        })
        .filter_map(erased_builtin_member_signature);
    let mut merged = signatures.next()?;
    for signature in signatures {
        merged.return_type = erase_type_conflict(&merged.return_type, &signature.return_type);
        let (Some(left), Some(right)) = (&mut merged.parameters, signature.parameters) else {
            merged.parameters = None;
            continue;
        };
        if left.len() != right.len() {
            merged.parameters = None;
            continue;
        }
        for (left, right) in left.iter_mut().zip(right) {
            *left = erase_type_conflict(left, &right);
        }
    }
    Some(merged)
}

fn erase_type_conflict(left: &Type, right: &Type) -> Type {
    match (left, right) {
        (
            Type::Reference {
                mutable: left_mutable,
                inner: left_inner,
            },
            Type::Reference {
                mutable: right_mutable,
                inner: right_inner,
            },
        ) if left_mutable == right_mutable => Type::Reference {
            mutable: *left_mutable,
            inner: Box::new(erase_type_conflict(left_inner, right_inner)),
        },
        (left, right) if left == right => left.clone(),
        _ => Type::Unknown,
    }
}

pub fn integer_intrinsic_type(
    intrinsic: &rils_builtins::IntrinsicDeclaration,
    integer: crate::types::IntegerType,
) -> Type {
    intrinsic_type(intrinsic, Type::Integer(integer))
}

pub fn float_intrinsic_type(
    intrinsic: &rils_builtins::IntrinsicDeclaration,
    float: crate::types::FloatType,
) -> Type {
    intrinsic_type(intrinsic, Type::Float(float))
}

fn intrinsic_type(intrinsic: &rils_builtins::IntrinsicDeclaration, self_type: Type) -> Type {
    let generics = HashMap::new();
    Type::function(
        intrinsic
            .signature
            .parameters
            .iter()
            .copied()
            .map(|pattern| resolve_member_pattern(pattern, &self_type, &generics))
            .collect(),
        resolve_member_pattern(intrinsic.signature.result, &self_type, &generics),
    )
}

pub fn builtin_owner_name(object: &Type) -> Option<&'static str> {
    builtin_owner(object).map(|(owner, _, _)| owner)
}

fn builtin_owner(object: &Type) -> Option<(&'static str, Type, HashMap<&'static str, Type>)> {
    let mut generics = HashMap::new();
    match object {
        Type::Reference { inner, .. } => builtin_owner(inner),
        Type::String => Some(("string", object.clone(), generics)),
        Type::Array { element, .. } => {
            generics.insert("T", (**element).clone());
            Some(("Array", object.clone(), generics))
        }
        Type::Option(inner) => {
            generics.insert("T", (**inner).clone());
            Some(("Option", object.clone(), generics))
        }
        Type::Result(ok, error) => {
            generics.insert("T", (**ok).clone());
            generics.insert("E", (**error).clone());
            Some(("Result", object.clone(), generics))
        }
        Type::Named { name, arguments }
            if matches!(
                name.as_str(),
                "Vec" | "HashMap" | "HashSet" | "Range" | "SequenceIterator"
            ) =>
        {
            match name.as_str() {
                "HashMap" => {
                    if let Some(key) = arguments.first() {
                        generics.insert("K", key.clone());
                    }
                    if let Some(value) = arguments.get(1) {
                        generics.insert("V", value.clone());
                    }
                }
                _ => {
                    if let Some(item) = arguments.first() {
                        generics.insert("T", item.clone());
                    }
                }
            }
            Some((
                match name.as_str() {
                    "Vec" => "Vec",
                    "HashMap" => "HashMap",
                    "HashSet" => "HashSet",
                    "Range" => "Range",
                    "SequenceIterator" => "Iterator",
                    _ => unreachable!(),
                },
                object.clone(),
                generics,
            ))
        }
        _ => None,
    }
}

fn resolve_member_pattern(
    pattern: rils_builtins::TypePattern,
    self_type: &Type,
    generics: &HashMap<&'static str, Type>,
) -> Type {
    use rils_builtins::TypePattern;
    match pattern {
        TypePattern::SelfType => self_type.clone(),
        TypePattern::Generic(name) => generics.get(name).cloned().unwrap_or(Type::Unknown),
        TypePattern::Option(inner) => Type::Option(Box::new(resolve_member_pattern(
            *inner, self_type, generics,
        ))),
        TypePattern::Result { ok, error } => Type::Result(
            Box::new(resolve_member_pattern(*ok, self_type, generics)),
            Box::new(resolve_member_pattern(*error, self_type, generics)),
        ),
        TypePattern::Tuple(elements) => Type::Tuple(
            elements
                .iter()
                .copied()
                .map(|element| resolve_member_pattern(element, self_type, generics))
                .collect(),
        ),
        TypePattern::Function { parameters, result } => Type::function(
            parameters
                .iter()
                .copied()
                .map(|parameter| resolve_member_pattern(parameter, self_type, generics))
                .collect(),
            resolve_member_pattern(*result, self_type, generics),
        ),
        TypePattern::Reference { mutable, inner } => Type::Reference {
            mutable,
            inner: Box::new(resolve_member_pattern(*inner, self_type, generics)),
        },
        TypePattern::Named { path, arguments } => Type::Named {
            name: path.into(),
            arguments: arguments
                .iter()
                .copied()
                .map(|argument| resolve_member_pattern(argument, self_type, generics))
                .collect(),
        },
        other => resolve_type_pattern(other),
    }
}

pub(crate) fn resolve_type_pattern(pattern: rils_builtins::TypePattern) -> Type {
    use rils_builtins::TypePattern;
    match pattern {
        TypePattern::SelfType | TypePattern::AnyInteger | TypePattern::Unknown => Type::Unknown,
        TypePattern::Generic(name) => Type::Variable(name.into()),
        TypePattern::Unit => Type::Unit,
        TypePattern::Bool => Type::Bool,
        TypePattern::Char => Type::Char,
        TypePattern::String => Type::String,
        TypePattern::F32 => Type::Float(crate::types::FloatType::F32),
        TypePattern::F64 => Type::Float(crate::types::FloatType::F64),
        TypePattern::U32 => Type::Integer(crate::types::IntegerType::U32),
        TypePattern::U8 => Type::Integer(crate::types::IntegerType::U8),
        TypePattern::Usize => Type::USIZE,
        TypePattern::Named { path, arguments } => Type::Named {
            name: path.into(),
            arguments: arguments
                .iter()
                .copied()
                .map(resolve_type_pattern)
                .collect(),
        },
        TypePattern::Option(inner) => Type::Option(Box::new(resolve_type_pattern(*inner))),
        TypePattern::Result { ok, error } => Type::Result(
            Box::new(resolve_type_pattern(*ok)),
            Box::new(resolve_type_pattern(*error)),
        ),
        TypePattern::Tuple(elements) => {
            Type::Tuple(elements.iter().copied().map(resolve_type_pattern).collect())
        }
        TypePattern::Function { parameters, result } => Type::function(
            parameters
                .iter()
                .copied()
                .map(resolve_type_pattern)
                .collect(),
            resolve_type_pattern(*result),
        ),
        TypePattern::Reference { mutable, inner } => Type::Reference {
            mutable,
            inner: Box::new(resolve_type_pattern(*inner)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integer_intrinsic_types_replace_nested_self_patterns() {
        let intrinsic = rils_builtins::integer_method("checked_add").unwrap();
        assert_eq!(
            integer_intrinsic_type(intrinsic, crate::types::IntegerType::I32),
            Type::function(vec![Type::I32], Type::Option(Box::new(Type::I32)))
        );
    }

    #[test]
    fn float_intrinsic_types_preserve_concrete_float_type() {
        let intrinsic = rils_builtins::float_method("clamp").unwrap();
        let float = Type::Float(crate::types::FloatType::F32);
        assert_eq!(
            float_intrinsic_type(intrinsic, crate::types::FloatType::F32),
            Type::function(vec![float.clone(), float.clone()], float)
        );
    }

    #[test]
    fn overloaded_imports_erase_parameter_and_return_conflicts() {
        let signature = erased_builtin_import_signature("core::value::replace").unwrap();
        assert!(signature.parameters.is_none());
        assert_eq!(signature.return_type, Type::Unknown);
    }

    #[test]
    fn derived_debug_import_has_one_reference_layer_per_argument() {
        assert_eq!(
            erased_builtin_import_signature("core::fmt::write_derived_debug"),
            Some(FunctionSignature::fixed(
                vec![
                    Type::Reference {
                        mutable: true,
                        inner: Box::new(Type::Unknown),
                    },
                    Type::Reference {
                        mutable: false,
                        inner: Box::new(Type::Unknown),
                    },
                ],
                Type::Result(Box::new(Type::Unit), Box::new(Type::named("FormatError")),),
            ))
        );
    }

    #[test]
    fn builtin_method_generics_preserve_callback_result_types() {
        assert_eq!(
            builtin_member_type(&Type::Option(Box::new(Type::I32)), "map"),
            Some(Type::function(
                vec![Type::function(vec![Type::I32], Type::Variable("U".into()))],
                Type::Option(Box::new(Type::Variable("U".into()))),
            ))
        );
        assert_eq!(
            builtin_member_type(
                &Type::Result(Box::new(Type::I32), Box::new(Type::String)),
                "map_err",
            ),
            Some(Type::function(
                vec![Type::function(
                    vec![Type::String],
                    Type::Variable("F".into()),
                )],
                Type::Result(Box::new(Type::I32), Box::new(Type::Variable("F".into()))),
            ))
        );
    }

    #[test]
    fn builtin_members_replace_generics_nested_in_return_types() {
        let tasks = Type::Named {
            name: "Vec".into(),
            arguments: vec![Type::named("Task")],
        };
        assert_eq!(
            builtin_member_type(&tasks, "into_iter"),
            Some(Type::function(
                Vec::new(),
                Type::Named {
                    name: "SequenceIterator".into(),
                    arguments: vec![Type::named("Task")],
                }
            ))
        );
    }
}
