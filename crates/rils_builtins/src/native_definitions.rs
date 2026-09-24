//! Metadata generated from the same Rust definition used by native handlers.

pub mod clone {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::clone_definition!(decl_rils_trait_metadata);
}

pub mod copy {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::copy_definition!(decl_rils_trait_metadata);
}

pub mod default {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::default_definition!(decl_rils_trait_metadata);
}

pub mod eq {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::eq_definition!(decl_rils_trait_metadata);
}

pub mod hash {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::hash_definition!(decl_rils_trait_metadata);
}

pub mod bit_flags {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::bitflags_definition!(decl_rils_trait_metadata);
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

pub mod range {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::range_definition!(decl_rils_metadata);
    rils_stdlib::range_definition!(decl_rils_trait_impls);
}

pub mod vec_deque {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::vecdeque_definition!(decl_rils_metadata);
    rils_stdlib::vecdeque_definition!(decl_rils_trait_impls);
}

pub mod binary_heap {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::binaryheap_definition!(decl_rils_metadata);
    rils_stdlib::binaryheap_definition!(decl_rils_trait_impls);
}

pub use option::DECLARATION;

pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[
    clone::DECLARATION,
    copy::DECLARATION,
    default::DECLARATION,
    eq::DECLARATION,
    hash::DECLARATION,
    bit_flags::DECLARATION,
    option::DECLARATION,
    result::DECLARATION,
    range::DECLARATION,
    vec_deque::DECLARATION,
    binary_heap::DECLARATION,
];
