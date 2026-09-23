use rils_builtins_macros::decl_rils_source;

fn main() {
    print!(
        "{}\0{}\0{}",
        rils_stdlib::option_definition!(decl_rils_source),
        rils_stdlib::result_definition!(decl_rils_source),
        rils_stdlib::integer_definition!(decl_rils_source)
    );
}
