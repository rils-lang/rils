use rils_builtins_macros::{decl_rils_source, decl_rils_trait_source};

fn main() {
    print!(
        "{}\0{}\0{}\0{}\0{}\0{}\0{}",
        rils_stdlib::option_definition!(decl_rils_source),
        rils_stdlib::result_definition!(decl_rils_source),
        rils_stdlib::integer_definition!(decl_rils_source),
        rils_stdlib::float_definition!(decl_rils_source),
        rils_stdlib::string_definition!(decl_rils_source),
        rils_stdlib::clone_definition!(decl_rils_trait_source),
        rils_stdlib::copy_definition!(decl_rils_trait_source)
    );
}
