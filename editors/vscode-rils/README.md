# Rils for Visual Studio Code

This extension provides syntax highlighting and connects VS Code to
`rils-analyzer` for diagnostics and language intelligence.

Version 0.1 follows the Rils 0.1 language version. Its TextMate grammar covers
the implemented declarations, generics and trait bounds, trait implementations,
nominal types and variants, pattern matching, function types, and function-like
macro syntax. It also highlights ownership operations and local `&T`/`&mut T`
references, built-in traits, and iterator-based `for value in iterable` loops.
It also covers generic type aliases, associated types, and `start..end` ranges.
Trait highlighting includes UFCS and fully qualified `<Type as Trait>::Item` paths.
The grammar also recognizes tuple fields, fixed-array/`Vec<T>` types, collection literals, and
field/index place syntax such as `value.0`, `value.field`, and `value[index]`.
The language server resolves explicit UFCS calls to their trait method declarations and traverses
both the owner and index expressions for diagnostics, references, hover, and semantic tokens.
It also indexes `.rils` files under every VS Code workspace folder for cross-file module navigation.
Language intelligence is provided by `rils-analyzer`. Static diagnostics cover names, basic type
compatibility, match/control flow, ownership, moves, mutability, and local-reference escape rules.
Definite semantic failures are errors; unreachable statements and match arms are warnings.

## Development

To validate and package the extension into `dist/` from the repository root:

```powershell
.\tools\package-vscode-rils.ps1
```

Pass `-SkipInstall` to reuse the currently installed npm dependencies.
The build step bundles the extension and language client into `out/extension.js`, so packaged
VSIX files do not include the development `node_modules` tree.

From the repository root:

```sh
cargo build -p rils_analyzer
cd editors/vscode-rils
npm install
npm run check
```

Open `editors/vscode-rils` as a VS Code workspace and press `F5`. The included
launch configuration starts an Extension Development Host. The extension first searches
the workspace's `target/release` and `target/debug` directories, then falls
back to `rils-analyzer` on `PATH`.

For another analyzer location, set `rils.server.path`.

## Architecture

The extension owns only VS Code integration and TextMate grammar files.
Parsing and semantic analysis stay in the Rils Rust crate, while the
editor-neutral protocol implementation lives in `tools/rils-analyzer`. This
keeps the analyzer reusable by a future Rider plugin or another editor.
