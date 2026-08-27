# rils_builtins_macros

Compile-time implementation details for `rils_builtins`. This crate parses the
stable built-in ID configuration owned by `rils_builtins` and expands its typed IDs
and lookup macro. It does not define built-in members or own their stable IDs.

`builtin_file!` parses a `.rils` standard-library declaration with the shared
`rils_syntax` lexer and parser, then emits the same metadata while checking that
every configured ID has exactly one method declaration. Standard-library files
may mark metadata-only associated functions with `#[metadata]` and reuse a
cross-module ID with an attribute such as `#[runtime(core::sequence::len)]`.

`builtin_numeric_file!` verifies that `.rils` declarations cover every concrete
integer primitive (`i8` through `usize`) or both floating-point primitives, and
generates their shared intrinsic and constant tables while preserving the
separate intrinsic execution backend.

`type_pattern!` converts the same Rust-style type syntax into a static
`TypePattern`, including generics, references, tuples, callbacks, `Option`,
`Result`, iterators, and fully qualified named types.
