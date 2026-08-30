//! Built-in import signatures understood by bytecode lowering.

use crate::types::FunctionSignature;

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
