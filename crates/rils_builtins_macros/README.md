# rils_builtins_macros

Compile-time catalog generation for `rils_builtins`. `builtin_stdlib!` reads the
remaining `.rils` module tree and prelude declarations, then emits the builtin
catalog, module relationship index, and source inventory. `builtin_catalog_file!`
uses the shared `rils_syntax` parser for each file.

Types and methods come from the Rust definitions in `rils_stdlib`; numeric
operations are keyed by their exported symbol paths. This crate does not assign
numeric builtin IDs.

`type_pattern!` converts Rust-style type syntax into a static `TypePattern`,
including generics, references, tuples, callbacks, `Option`, and `Result`.
