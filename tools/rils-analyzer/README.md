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
- `rils.toml` project discovery and `src` workspace indexing with cross-file locations
- module declarations, imports, visibility symbols, and namespace semantic tokens
- verified `.rilhm` Host Manifest loading through the LSP `initializationOptions.hostManifestPaths`
- recursive `.rils/manifest` discovery and deterministic multi-fragment contract merging
- host module/member completion after `::`, including `use ... as ...` module aliases, signatures, and capabilities
- host enum and variant hover, including underlying integer types, raw values, and `BitFlags` metadata
- host enum variant and script-defined inherent method completion
- project module and public-item completion after `crate::`, `self::`, `super::`, or a `use` alias

Host manifests are binary runtime contracts. Each configured file is verified before its symbols are added to
diagnostics, type inference, hover, semantic tokens, and completion. Multiple manifests must use the same host ABI
and contract version and cannot contain conflicting declarations.

- inferred return, local binding, and pattern binding type hints
- complete higher-order function signatures such as `fn() -> fn() -> i32`

Richer module-aware type inference and rename support remain future work.
