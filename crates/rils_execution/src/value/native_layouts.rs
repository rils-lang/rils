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
    pub use rils_stdlib::stdlib::collections::vector::vec_layout::{layout, matches};
}

pub mod vec_deque {
    pub use rils_stdlib::stdlib::collections::vec_deque_layout::{layout, matches};
}

pub mod binary_heap {
    pub use rils_stdlib::stdlib::collections::binary_heap_layout::{layout, matches};
}

pub mod hash_set {
    pub use rils_stdlib::stdlib::collections::hash::hash_set_layout::{layout, matches};
}

pub mod btree_set {
    pub use rils_stdlib::stdlib::collections::btree_set_layout::{layout, matches};
}

pub mod hash_map {
    pub use rils_stdlib::stdlib::collections::hash::hash_map_layout::{layout, matches};
}

pub mod btree_map {
    pub use rils_stdlib::stdlib::collections::btree_map_layout::{layout, matches};
}
