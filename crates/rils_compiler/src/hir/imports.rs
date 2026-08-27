//! Built-in import signatures understood by bytecode lowering.

use crate::types::{FunctionSignature, Type};

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
