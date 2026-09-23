use rils_builtins_macros::{decl_rils_source, decl_rils_trait_source};
use rils_syntax::rils_stdlib_sources;

fn main() {
    for (path, source) in rils_stdlib_sources!("src/stdlib") {
        print!("{path}\0{source}\0");
    }
}
