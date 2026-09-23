use rils_builtins_macros::decl_rils_source;

#[test]
fn language_sources_match_the_rust_definitions() {
    for (generated, checked_in) in [
        (
            rils_stdlib::option_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/option.rils"),
        ),
        (
            rils_stdlib::result_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/result.rils"),
        ),
        (
            rils_stdlib::integer_definition!(decl_rils_source),
            include_str!("../../rils_builtins/stdlib/core/integer.rils"),
        ),
    ] {
        assert_eq!(generated, checked_in.replace("\r\n", "\n"));
    }
}
