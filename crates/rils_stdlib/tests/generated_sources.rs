use rils_builtins_macros::{decl_rils_source, decl_rils_trait_source};
use rils_syntax::rils_stdlib_sources;

#[test]
fn exported_definitions_have_parseable_language_sources() {
    for source in [
        rils_stdlib::option_definition!(decl_rils_source),
        rils_stdlib::result_definition!(decl_rils_source),
        rils_stdlib::integer_definition!(decl_rils_source),
        rils_stdlib::float_definition!(decl_rils_source),
        rils_stdlib::string_definition!(decl_rils_source),
        rils_stdlib::binaryheap_definition!(decl_rils_source),
        rils_stdlib::vecdeque_definition!(decl_rils_source),
    ] {
        let tokens = rils_syntax::lex(source).expect("generated source lexes");
        rils_syntax::parser::parse_builtin_declarations(tokens).expect("generated source parses");
    }
}

#[test]
fn grouped_collections_keep_module_source_paths() {
    let sources = rils_stdlib_sources!("src/stdlib");
    for (path, expected) in [
        (
            "core/collections/binary_heap.rils",
            rils_stdlib::binaryheap_definition!(decl_rils_source),
        ),
        (
            "core/collections/vec_deque.rils",
            rils_stdlib::vecdeque_definition!(decl_rils_source),
        ),
    ] {
        let generated = sources
            .iter()
            .find(|(source_path, _)| *source_path == path)
            .expect("grouped type has a module source path")
            .1;
        assert_eq!(generated, expected);
    }
    assert!(
        !sources
            .iter()
            .any(|(path, _)| *path == "core/collections.rils")
    );
}
