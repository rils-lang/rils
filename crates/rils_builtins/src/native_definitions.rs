//! Metadata generated from the same Rust definition used by native handlers.

pub mod option {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::option_definition!(decl_rils_metadata);
}

pub mod result {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::result_definition!(decl_rils_metadata);
}

pub mod integer {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::integer_definition!(decl_rils_metadata);
}

pub use option::DECLARATION;

pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[option::DECLARATION, result::DECLARATION];
