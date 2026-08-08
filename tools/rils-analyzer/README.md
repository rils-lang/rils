# rils-analyzer

`rils-analyzer` is Rils's editor-independent Language Server Protocol (LSP)
implementation. The VS Code extension in `editors/vscode-rils` is its first
client; other editors can reuse the same executable.

## Build

```sh
cargo build -p rils_analyzer
```

The executable is written to `target/debug/rils-analyzer` (or
`target/debug/rils-analyzer.exe` on Windows).

## Current features

- syntax and semantic diagnostics
- go to definition and find references within the current document
- hover information
- document symbols
- semantic tokens
- custom-type references inside annotations
- generic type aliases, trait associated types and fully qualified projections
- UFCS method symbols with go-to-definition for `Trait::method` and `<Type as Trait>::method`
- tuple fields, fixed arrays, `Vec<T>`, nested place borrowing and concrete index-expression analysis
- workspace `.rils` indexing with cross-file definition and reference locations
- module declarations, imports, visibility symbols, and namespace semantic tokens
- inferred return, local binding, and pattern binding type hints
- complete higher-order function signatures such as `fn() -> fn() -> int`

Richer module-aware type inference and rename support remain future work.
