//! Built-in import signatures understood by bytecode lowering.

use crate::types::{FunctionSignature, Type};

use super::ReceiverMode;
pub(super) fn core_import_signature(name: &str) -> Option<FunctionSignature> {
    let declaration = rils_builtins::builtin_function(name)?;
    (declaration.backend == rils_builtins::BuiltinBackend::Runtime)
        .then(|| rils_frontend::standard_library::standard_function_signature(name))
        .flatten()
}

pub(super) fn native_macro_import(
    name: &str,
) -> Option<(&'static str, FunctionSignature, &'static str)> {
    let (path, capability) = match name {
        "#rils_native_print" => ("std::io::print", "std::io"),
        "#rils_native_println" => ("std::io::println", "std::io"),
        "#rils_native_assert" => {
            return Some((
                "core::assert",
                FunctionSignature::variadic(Type::Unit),
                "core",
            ));
        }
        _ => return None,
    };
    Some((
        path,
        rils_frontend::standard_library::standard_function_signature(path)
            .expect("native macro target is declared in rils_builtins"),
        capability,
    ))
}

pub(super) fn collection_import_signature(name: &str) -> Option<(&'static str, FunctionSignature)> {
    let mut segments = name.rsplit("::");
    let member_name = segments.next()?;
    let owner = segments.next()?;
    let member = rils_builtins::builtin_member(owner, member_name)?;
    let runtime_import = member.runtime_import?;
    let signature =
        rils_frontend::standard_library::builtin_associated_function_signature(owner, member_name)?;
    Some((runtime_import, signature))
}

pub(super) fn builtin_method_runtime(
    owner: Option<&str>,
    name: &str,
) -> Option<(rils_builtins::BuiltinId, ReceiverMode)> {
    let mut candidates = rils_builtins::BUILTINS
        .iter()
        .filter(|declaration| owner.is_none_or(|owner| declaration.path == owner))
        .flat_map(|declaration| declaration.members)
        .filter(|member| member.name == name && member.builtin_id.is_some());
    let member = candidates.next().or_else(|| {
        (name == "clone")
            .then(|| rils_builtins::builtin_member("Clone", "clone"))
            .flatten()
    })?;
    let runtime = member.builtin_id?;
    if !runtime.has_direct_runtime_call() {
        return None;
    }
    let receiver_mode = member.receiver?;
    if owner.is_none()
        && candidates.any(|candidate| {
            candidate.receiver != Some(receiver_mode)
                || candidate.builtin_id.is_none_or(|candidate| {
                    !runtime.shares_direct_runtime_implementation(candidate)
                })
        })
    {
        return None;
    }
    let receiver = match receiver_mode {
        rils_builtins::ReceiverMode::Owned => ReceiverMode::Owned,
        rils_builtins::ReceiverMode::Shared => ReceiverMode::Reference { mutable: false },
        rils_builtins::ReceiverMode::Mutable => ReceiverMode::Reference { mutable: true },
    };
    Some((runtime, receiver))
}
