//! Built-in import signatures understood by bytecode lowering.

use crate::types::{FunctionSignature, Type};

use super::ReceiverMode;
pub(super) fn core_import_signature(name: &str) -> Option<FunctionSignature> {
    Some(match name {
        "type_of" => FunctionSignature::fixed(vec![Type::Unknown], Type::String),
        "clone" => FunctionSignature::fixed(
            vec![Type::Reference {
                mutable: false,
                inner: Box::new(Type::Unknown),
            }],
            Type::Unknown,
        ),
        "is_ok" | "is_err" | "is_some" | "is_none" => {
            FunctionSignature::fixed(vec![Type::Unknown], Type::Bool)
        }
        "unwrap" => FunctionSignature::fixed(vec![Type::Unknown], Type::Unknown),
        "unwrap_or" => FunctionSignature::fixed(vec![Type::Unknown, Type::Unknown], Type::Unknown),
        _ => return None,
    })
}

pub(super) fn native_macro_import(
    name: &str,
) -> Option<(&'static str, FunctionSignature, &'static str)> {
    Some(match name {
        "#rils_native_print" => (
            "std::io::print",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
        ),
        "#rils_native_println" => (
            "std::io::println",
            FunctionSignature::variadic(Type::Unit),
            "std::io",
        ),
        "#rils_native_assert" => (
            "core::assert",
            FunctionSignature::variadic(Type::Unit),
            "core",
        ),
        _ => return None,
    })
}

pub(super) fn collection_import_signature(name: &str) -> Option<(&'static str, FunctionSignature)> {
    Some(match name {
        "Vec::new" | "std::collections::Vec::new" => (
            "core::vec::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        "Vec::from" | "std::collections::Vec::from" => (
            "core::vec::from",
            FunctionSignature::fixed(
                vec![Type::Unknown],
                Type::Named {
                    name: "Vec".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        "HashMap::new" | "std::collections::HashMap::new" => (
            "core::hash_map::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "HashMap".into(),
                    arguments: vec![Type::Unknown, Type::Unknown],
                },
            ),
        ),
        "HashSet::new" | "std::collections::HashSet::new" => (
            "core::hash_set::new",
            FunctionSignature::fixed(
                Vec::new(),
                Type::Named {
                    name: "HashSet".into(),
                    arguments: vec![Type::Unknown],
                },
            ),
        ),
        _ => return None,
    })
}

pub(super) fn builtin_method_import(
    owner: Option<&str>,
    name: &str,
) -> Option<(&'static str, FunctionSignature, ReceiverMode)> {
    let mut candidates = rils_builtins::BUILTINS
        .iter()
        .filter(|declaration| owner.is_none_or(|owner| declaration.path == owner))
        .flat_map(|declaration| declaration.members)
        .filter(|member| member.name == name && member.runtime.is_some());
    let member = candidates.next().or_else(|| {
        (name == "clone")
            .then(|| rils_builtins::builtin_member("Clone", "clone"))
            .flatten()
    })?;
    let runtime = member.runtime?;
    let import = runtime.bytecode_import()?;
    let receiver_mode = member.receiver?;
    if owner.is_none()
        && candidates.any(|candidate| {
            candidate.receiver != Some(receiver_mode)
                || candidate
                    .runtime
                    .and_then(rils_builtins::RuntimeMemberId::bytecode_import)
                    != Some(import)
        })
    {
        return None;
    }
    let receiver = match receiver_mode {
        rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
        rils_builtins::ReceiverMode::Shared => ReceiverMode::Reference { mutable: false },
        rils_builtins::ReceiverMode::Mutable => ReceiverMode::Reference { mutable: true },
    };
    Some((
        import,
        rils_frontend::standard_library::erased_builtin_import_signature(import)?,
        receiver,
    ))
}
