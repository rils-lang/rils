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

/// Returns the runtime ABI view of a standard function signature, replacing
/// declaration generics with type-erased slots while preserving its shape.
pub fn erased_standard_function_signature(name: &str) -> Option<FunctionSignature> {
    let signature = standard_function_signature(name)?;
    Some(FunctionSignature {
        parameters: signature
            .parameters
            .map(|parameters| parameters.into_iter().map(erase_type_variables).collect()),
        return_type: erase_type_variables(signature.return_type),
    })
}

fn erase_type_variables(ty: Type) -> Type {
    match ty {
        Type::Variable(_) => Type::Unknown,
        Type::Tuple(elements) => {
            Type::Tuple(elements.into_iter().map(erase_type_variables).collect())
        }
        Type::Array { element, length } => Type::Array {
            element: Box::new(erase_type_variables(*element)),
            length,
        },
        Type::Reference { mutable, inner } => Type::Reference {
            mutable,
            inner: Box::new(erase_type_variables(*inner)),
        },
        Type::Function {
            parameters,
            return_type,
        } => Type::Function {
            parameters: parameters
                .map(|parameters| parameters.into_iter().map(erase_type_variables).collect()),
            return_type: Box::new(erase_type_variables(*return_type)),
        },
        Type::Option(inner) => Type::Option(Box::new(erase_type_variables(*inner))),
        Type::Result(ok, error) => Type::Result(
            Box::new(erase_type_variables(*ok)),
            Box::new(erase_type_variables(*error)),
        ),
        Type::Named { name, arguments } => Type::Named {
            name,
            arguments: arguments.into_iter().map(erase_type_variables).collect(),
        },
        Type::Associated { .. } => Type::Unknown,
        concrete => concrete,
    }
}

pub fn builtin_member_type(object: &Type, name: &str) -> Option<Type> {
    let (_, self_type, mut generics) = builtin_owner(object)?;
    let member = builtin_member_for_type(object, name)?;
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

pub fn builtin_iterator_item_type(object: &Type) -> Option<Type> {
    let Type::Function { return_type, .. } = builtin_member_type(object, "next")? else {
        return None;
    };
    let Type::Option(item) = *return_type else {
        return None;
    };
    Some(*item)
}

pub fn builtin_into_iterator_item_type(object: &Type) -> Option<Type> {
    let Type::Function { return_type, .. } = builtin_member_type(object, "into_iter")? else {
        return None;
    };
    builtin_iterator_item_type(&return_type)
}

pub fn builtin_associated_function_signature(owner: &str, name: &str) -> Option<FunctionSignature> {
    let declaration = rils_builtins::builtin(owner)?;
    let member = declaration.member(name)?;
    if member.kind != rils_builtins::BuiltinMemberKind::AssociatedFunction {
        return None;
    }
    let signature = member.signature?;
    let self_type = Type::Named {
        name: owner.into(),
        arguments: declaration
            .type_parameters
            .iter()
            .map(|name| Type::Variable((*name).into()))
            .collect(),
    };
    let generics = declaration
        .type_parameters
        .iter()
        .map(|parameter| (*parameter, Type::Variable((*parameter).into())))
        .collect::<HashMap<_, _>>();
    Some(FunctionSignature::fixed(
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
    builtin_trait_member_type_with_iterator_item(trait_name, object, name, None)
}

pub(crate) fn builtin_trait_member_type_with_iterator_item(
    trait_name: &str,
    object: &Type,
    name: &str,
    iterator_item: Option<Type>,
) -> Option<Type> {
    let member = rils_builtins::builtin_member(trait_name, name)?;
    let signature = member.signature?;
    let mut generics = HashMap::new();
    if let Some(item) = iterator_item {
        generics.insert("Iterator::Item", item);
    }
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
    builtin_member_for_type(object, name)?.receiver
}

pub fn builtin_member_for_type(
    object: &Type,
    name: &str,
) -> Option<&'static rils_builtins::BuiltinMember> {
    let (owner, _, _) = builtin_owner(object)?;
    let member = rils_builtins::builtin_member(owner, name)?;
    if is_indexed_view(object) && !member.indexed_view {
        return None;
    }
    Some(member)
}

pub fn unsupported_indexed_view_member(object: &Type, name: &str) -> bool {
    is_indexed_view(object)
        && builtin_owner(object)
            .and_then(|(owner, _, _)| rils_builtins::builtin_member(owner, name))
            .is_some_and(|member| !member.indexed_view)
}

fn is_indexed_view(ty: &Type) -> bool {
    match ty {
        Type::Reference { inner, .. } => is_indexed_view(inner),
        Type::Array { .. } | Type::Slice(_) => true,
        _ => false,
    }
}

/// Builds the type-erased ABI signature used when a built-in member is
/// lowered without retaining its concrete receiver type.
pub fn erased_builtin_member_signature(
    member: &rils_builtins::BuiltinMember,
) -> Option<FunctionSignature> {
    let signature = member.signature?;
    let receiver = match member.receiver {
        Some(rils_builtins::ReceiverMode::Owned) => Some(Type::Unknown),
        Some(rils_builtins::ReceiverMode::Shared) => Some(Type::Reference {
            mutable: false,
            inner: Box::new(Type::Unknown),
        }),
        Some(rils_builtins::ReceiverMode::Mutable) => Some(Type::Reference {
            mutable: true,
            inner: Box::new(Type::Unknown),
        }),
        None => None,
    };
    let generics = HashMap::new();
    let mut parameters = Vec::with_capacity(signature.parameters.len() + 1);
    parameters.extend(receiver);
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

pub fn erased_runtime_signature(id: rils_builtins::BuiltinId) -> Option<FunctionSignature> {
    let (_, member) = rils_builtins::runtime_member(id)?;
    erased_builtin_member_signature(member)
}

/// Signature stored in a native import for a numeric operation.
pub fn erased_intrinsic_symbol_signature(symbol: &str) -> Option<FunctionSignature> {
    let (declaration, target) = rils_builtins::intrinsic_by_symbol(symbol)?;
    let self_type = target.map(Type::Integer).unwrap_or(Type::Unknown);
    let Type::Function {
        parameters,
        return_type,
    } = intrinsic_type(declaration, self_type.clone())
    else {
        unreachable!("numeric declaration is a function");
    };
    let mut parameters = parameters.expect("numeric declaration has fixed parameters");
    if declaration.kind == rils_builtins::IntrinsicKind::Method {
        parameters.insert(0, self_type);
    }
    Some(FunctionSignature::fixed(parameters, *return_type))
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

pub fn builtin_type_name(name: &str) -> Option<&'static str> {
    let declaration = builtin_named_declaration(name)?;
    matches!(
        declaration.kind,
        rils_builtins::BuiltinKind::Struct | rils_builtins::BuiltinKind::Enum
    )
    .then_some(declaration.path)
}

fn builtin_named_declaration(name: &str) -> Option<&'static rils_builtins::BuiltinDeclaration> {
    rils_builtins::builtin(name).or_else(|| {
        let (module, exported_name) = name.rsplit_once("::")?;
        if rils_builtins::builtin_module_members(module).contains(&exported_name) {
            rils_builtins::builtin(exported_name)
        } else {
            None
        }
    })
}

fn builtin_owner(object: &Type) -> Option<(&'static str, Type, HashMap<&'static str, Type>)> {
    let mut generics = HashMap::new();
    match object {
        Type::Reference { inner, .. } => builtin_owner(inner),
        Type::String => Some(("string", object.clone(), generics)),
        Type::Array { element, .. } => {
            generics.insert("T", (**element).clone());
            Some(("Vec", object.clone(), generics))
        }
        Type::Slice(element) => {
            generics.insert("T", (**element).clone());
            Some(("Vec", object.clone(), generics))
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
        Type::Named { name, arguments } if name == "OwnedIterator" => {
            generics.insert("T", arguments.first().cloned().unwrap_or(Type::Unknown));
            Some(("Iterator", object.clone(), generics))
        }
        Type::Named { name, arguments } => {
            let declaration = builtin_named_declaration(name)?;
            if !matches!(
                declaration.kind,
                rils_builtins::BuiltinKind::Struct | rils_builtins::BuiltinKind::Enum
            ) {
                return None;
            }
            for (parameter, argument) in declaration.type_parameters.iter().zip(arguments) {
                generics.insert(*parameter, argument.clone());
            }
            Some((declaration.path, object.clone(), generics))
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
        TypePattern::Generic(name) => generics
            .get(name)
            .cloned()
            .unwrap_or_else(|| Type::Variable(name.into())),
        TypePattern::Array { element, length } => Type::Array {
            element: Box::new(resolve_member_pattern(*element, self_type, generics)),
            length,
        },
        TypePattern::ArrayParameter { element, length } => Type::ArrayParameter {
            element: Box::new(resolve_member_pattern(*element, self_type, generics)),
            length: length.into(),
        },
        TypePattern::Slice(element) => Type::Slice(Box::new(resolve_member_pattern(
            *element, self_type, generics,
        ))),
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
        TypePattern::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => {
            let base = resolve_member_pattern(*base, self_type, generics);
            if trait_name == Some("Iterator")
                && name == "Item"
                && arguments.is_empty()
                && base == *self_type
                && let Some(item) = generics.get("Iterator::Item")
            {
                return item.clone();
            }
            if trait_name == Some("Iterator")
                && name == "Item"
                && arguments.is_empty()
                && let Type::Named {
                    name: owner,
                    arguments: items,
                } = &base
                && matches!(owner.as_str(), "OwnedIterator" | "Iter" | "Range")
            {
                return items.first().cloned().unwrap_or(Type::Unknown);
            }
            Type::Associated {
                base: Box::new(base),
                trait_name: trait_name.map(str::to_owned),
                name: name.into(),
                arguments: arguments
                    .iter()
                    .copied()
                    .map(|argument| resolve_member_pattern(argument, self_type, generics))
                    .collect(),
            }
        }
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

pub fn resolve_type_pattern(pattern: rils_builtins::TypePattern) -> Type {
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
        TypePattern::BoundGeneric { name, bounds } => Type::BoundVariable {
            name: name.into(),
            bounds: bounds.iter().copied().map(resolve_type_pattern).collect(),
        },
        TypePattern::Array { element, length } => Type::Array {
            element: Box::new(resolve_type_pattern(*element)),
            length,
        },
        TypePattern::ArrayParameter { element, length } => Type::ArrayParameter {
            element: Box::new(resolve_type_pattern(*element)),
            length: length.into(),
        },
        TypePattern::Slice(element) => Type::Slice(Box::new(resolve_type_pattern(*element))),
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
        TypePattern::Associated {
            base,
            trait_name,
            name,
            arguments,
        } => Type::Associated {
            base: Box::new(resolve_type_pattern(*base)),
            trait_name: trait_name.map(str::to_owned),
            name: name.into(),
            arguments: arguments
                .iter()
                .copied()
                .map(resolve_type_pattern)
                .collect(),
        },
    }
}

#[cfg(test)]
#[path = "../tests/unit/standard_library.rs"]
mod tests;
