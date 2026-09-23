//! Native methods generated from the shared Rust standard-library definitions.

mod option {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::option_definition!(decl_rils_native);
}

mod result {
    use rils_builtins_macros::decl_rils_native;

    rils_stdlib::result_definition!(decl_rils_native);
}

pub fn call(
    id: rils_builtins::BuiltinId,
    arguments: &[crate::Value],
) -> Option<Result<crate::Value, String>> {
    option::call(id, arguments).or_else(|| result::call(id, arguments))
}
