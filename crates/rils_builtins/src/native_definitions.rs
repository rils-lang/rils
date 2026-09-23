//! Metadata generated from the same Rust definition used by native handlers.

pub mod clone {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::clone_definition!(decl_rils_trait_metadata);
}

pub mod copy {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::copy_definition!(decl_rils_trait_metadata);
}

pub mod option {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::option_definition!(decl_rils_metadata);
    rils_stdlib::option_definition!(decl_rils_trait_impls);
}

pub mod result {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::result_definition!(decl_rils_metadata);
    rils_stdlib::result_definition!(decl_rils_trait_impls);
}

pub mod integer {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::integer_definition!(decl_rils_metadata);
    rils_stdlib::integer_definition!(decl_rils_trait_impls);
}

pub mod float {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::float_definition!(decl_rils_metadata);
    rils_stdlib::float_definition!(decl_rils_trait_impls);
}

pub mod string {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::string_definition!(decl_rils_metadata);
    rils_stdlib::string_definition!(decl_rils_trait_impls);
}

pub use option::DECLARATION;

pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[
    clone::DECLARATION,
    copy::DECLARATION,
    option::DECLARATION,
    result::DECLARATION,
];
