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

pub mod btree_map {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::btreemap_definition!(decl_rils_metadata);
}

pub mod btree_set {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::btreeset_definition!(decl_rils_metadata);
}

pub mod hash_map {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::hashmap_definition!(decl_rils_metadata);
}

pub mod hash_set {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::hashset_definition!(decl_rils_metadata);
}

pub mod vec {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::vec_definition!(decl_rils_metadata);
}

pub mod iter {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::iter_definition!(decl_rils_metadata);
}

pub mod iterator {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::iterator_definition!(decl_rils_trait_metadata);
}

pub mod into_iterator {
    use rils_builtins_macros::decl_rils_trait_metadata;

    rils_stdlib::intoiterator_definition!(decl_rils_trait_metadata);
}

pub mod format_error {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::formaterror_definition!(decl_rils_metadata);
}

pub mod boxed {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::box_definition!(decl_rils_metadata);
}

pub mod io_error {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::error_definition!(decl_rils_metadata);
}

pub mod io_error_kind {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::errorkind_definition!(decl_rils_metadata);
}

pub mod rc {
    use rils_builtins_macros::{decl_rils_metadata, decl_rils_trait_impls};

    rils_stdlib::rc_definition!(decl_rils_metadata);
    rils_stdlib::rc_definition!(decl_rils_trait_impls);
}

pub mod weak {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::weak_definition!(decl_rils_metadata);
}

pub mod cell {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::cell_definition!(decl_rils_metadata);
}

pub mod ref_cell {
    use rils_builtins_macros::decl_rils_metadata;

    rils_stdlib::refcell_definition!(decl_rils_metadata);
}

pub mod fs {
    use rils_builtins_macros::decl_rils_function_metadata;

    pub mod read_to_string {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_read_to_string_definition!(decl_rils_function_metadata);
    }
    pub mod write {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_write_definition!(decl_rils_function_metadata);
    }
    pub mod append {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_append_definition!(decl_rils_function_metadata);
    }
    pub mod try_exists {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_try_exists_definition!(decl_rils_function_metadata);
    }
    pub mod create_dir_all {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_create_dir_all_definition!(decl_rils_function_metadata);
    }
    pub mod remove_file {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_remove_file_definition!(decl_rils_function_metadata);
    }
    pub mod remove_dir {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_remove_dir_definition!(decl_rils_function_metadata);
    }
    pub mod read_dir {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_fs_read_dir_definition!(decl_rils_function_metadata);
    }

    pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[
        read_to_string::DECLARATION,
        write::DECLARATION,
        append::DECLARATION,
        try_exists::DECLARATION,
        create_dir_all::DECLARATION,
        remove_file::DECLARATION,
        remove_dir::DECLARATION,
        read_dir::DECLARATION,
    ];
}

pub mod io_functions {
    use rils_builtins_macros::decl_rils_function_metadata;

    pub mod read_line {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_read_line_definition!(decl_rils_function_metadata);
    }
    pub mod print {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_print_definition!(decl_rils_function_metadata);
    }
    pub mod println {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_println_definition!(decl_rils_function_metadata);
    }
    pub mod write {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_write_definition!(decl_rils_function_metadata);
    }
    pub mod write_line {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_write_line_definition!(decl_rils_function_metadata);
    }
    pub mod flush {
        use super::decl_rils_function_metadata;
        rils_stdlib::std_io_flush_definition!(decl_rils_function_metadata);
    }

    pub const DECLARATIONS: &[crate::BuiltinDeclaration] = &[
        read_line::DECLARATION,
        print::DECLARATION,
        println::DECLARATION,
        write::DECLARATION,
        write_line::DECLARATION,
        flush::DECLARATION,
    ];
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
    btree_map::DECLARATION,
    btree_set::DECLARATION,
    hash_map::DECLARATION,
    hash_set::DECLARATION,
    vec::DECLARATION,
    iter::DECLARATION,
    iterator::DECLARATION,
    into_iterator::DECLARATION,
    format_error::DECLARATION,
    boxed::DECLARATION,
    io_error::DECLARATION,
    io_error_kind::DECLARATION,
    rc::DECLARATION,
    weak::DECLARATION,
    cell::DECLARATION,
    ref_cell::DECLARATION,
];
