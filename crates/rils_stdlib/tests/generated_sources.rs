use rils_builtins_macros::{decl_rils_source, decl_rils_trait_source};
use rils_syntax::rils_stdlib_sources;

#[test]
fn language_sources_match_the_rust_definitions() {
    for (generated, checked_in) in [
        (
            rils_stdlib::option_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/option/option.rils"),
        ),
        (
            rils_stdlib::result_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/result/result.rils"),
        ),
        (
            rils_stdlib::integer_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/integer.rils"),
        ),
        (
            rils_stdlib::float_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/float.rils"),
        ),
        (
            rils_stdlib::string_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/string/string.rils"),
        ),
        (
            rils_stdlib::binaryheap_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/collections/binary_heap.rils"),
        ),
        (
            rils_stdlib::vecdeque_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/collections/vec_deque.rils"),
        ),
    ] {
        assert_eq!(generated, checked_in.replace("\r\n", "\n"));
    }
}

#[test]
fn grouped_rust_collections_use_their_module_source_paths() {
    let sources = rils_stdlib_sources!("src/stdlib");
    for (path, checked_in) in [
        (
            "core/collections/binary_heap.rils",
            include_str!("../../rils_builtins/stdlib/core/collections/binary_heap.rils"),
        ),
        (
            "core/collections/vec_deque.rils",
            include_str!("../../rils_builtins/stdlib/core/collections/vec_deque.rils"),
        ),
    ] {
        let generated = sources
            .iter()
            .find(|(source_path, _)| *source_path == path)
            .expect("grouped type has a module source path")
            .1;
        assert_eq!(generated, checked_in.replace("\r\n", "\n"));
    }
    assert!(
        !sources
            .iter()
            .any(|(path, _)| *path == "core/collections.rils")
    );
}
