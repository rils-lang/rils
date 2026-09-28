//! Layout factories emitted from the same Rust standard-library declarations.

pub mod integer {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::integer_definition!(decl_rils_layout);
}

pub mod float {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::float_definition!(decl_rils_layout);
}

pub mod string {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::string_definition!(decl_rils_layout);
}

pub mod option {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::option_definition!(decl_rils_layout);
}

pub mod vec {
    pub use rils_stdlib::stdlib::collections::vector::is_native_vec as matches;
}

pub mod vec_deque {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::vecdeque_definition!(decl_rils_layout);
}

pub mod binary_heap {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::binaryheap_definition!(decl_rils_layout);
}

pub mod hash_set {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::hashset_definition!(decl_rils_layout);
}

pub mod btree_set {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::btreeset_definition!(decl_rils_layout);
}

pub mod hash_map {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::hashmap_definition!(decl_rils_layout);
}

pub mod btree_map {
    use rils_stdlib_macros::decl_rils_layout;

    rils_stdlib::btreemap_definition!(decl_rils_layout);
}
