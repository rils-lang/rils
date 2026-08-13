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
    let (owner, self_type, generics) = builtin_owner(object)?;
    let member = rils_builtins::builtin_member(owner, name)?;
    let signature = member.signature?;
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

pub fn builtin_receiver_mode(object: &Type, name: &str) -> Option<rils_builtins::ReceiverMode> {
    let (owner, _, _) = builtin_owner(object)?;
    rils_builtins::builtin_member(owner, name)?.receiver
}

fn builtin_owner(object: &Type) -> Option<(&'static str, Type, HashMap<&'static str, Type>)> {
    let mut generics = HashMap::new();
    match object {
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
        Type::Named { name, arguments } if matches!(name.as_str(), "Vec" | "Range") => {
            if let Some(item) = arguments.first() {
                generics.insert("T", item.clone());
            }
            Some((
                if name == "Vec" { "Vec" } else { "Range" },
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
    match pattern {
        rils_builtins::TypePattern::SelfType => self_type.clone(),
        rils_builtins::TypePattern::Generic(name) => {
            generics.get(name).cloned().unwrap_or(Type::Unknown)
        }
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
        TypePattern::String => Type::String,
        TypePattern::F32 => Type::Float(crate::types::FloatType::F32),
        TypePattern::F64 => Type::Float(crate::types::FloatType::F64),
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
