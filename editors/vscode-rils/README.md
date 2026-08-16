# Rils for Visual Studio Code

This extension provides syntax highlighting and language intelligence through
`rils-analyzer`. Published platform-specific VSIX packages include the matching
Analyzer executable, so users do not need to install it separately.

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
It discovers `rils.toml`, indexes only its configured `script_paths`, and maps files to stable module paths for
cross-file navigation and completion. Without a project file it falls back to workspace-wide `.rils` indexing.
Project-aware completion and navigation understand wildcard imports and recursively grouped imports, including
incomplete `use crate::module::{...` trees while they are being typed.
Language intelligence is provided by `rils-analyzer`. Static diagnostics cover names, basic type
compatibility, match/control flow, ownership, moves, mutability, and local-reference escape rules.
Definite semantic failures are errors; unreachable statements and match arms are warnings.

## Host Manifest and completion

Set `rils.hostManifest.path` to a verified binary `.rilhm` file, using either an absolute path or a path relative
to the workspace folder. When the setting is empty, the Analyzer reads `[host].manifest` from `rils.toml`, then
checks conventional names at the project and configured script roots. Reload the VS Code window after changing
the path or replacing the manifest.

The project convention is `.rils/manifest/**/*.rilhm`. Every fragment is verified and merged into one logical
contract; an explicitly configured `rils.hostManifest.path` remains a single-file override.

The Analyzer uses the contract for diagnostics and type checking. Typing a qualified module path followed by
`::`, for example `unity_engine::math::`, lists its accessible host functions and child modules. Completion items
show the function signature and required capability. Module aliases such as
`use unity_engine::math as math; math::` are supported.

Project modules use the same completion flow. After `crate::`, `self::`, `super::`, a module path, or a
`use` alias, the Analyzer lists child modules and public declarations from the target `.rils` file.

Generate the runtime manifest explicitly from a JSON tool input when needed:

```console
rils host-manifest compile host-contract.json -o rils-host.rilhm
```

## Development

To validate and package the extension into `dist/` from the repository root:

```console
python tools/release-vscode.py
```

The script detects the current platform by default. Pass a supported VS Code
target such as `--target win32-x64`, `--skip-install` to reuse the currently
installed npm dependencies, or `--publish` to publish the generated VSIX after
packaging it. Cross-platform targets require the corresponding Rust target and
linker to be installed.
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
launch configuration starts an Extension Development Host. The extension uses
`rils.server.path` when configured, then its bundled Analyzer. During repository
development it selects the newest Analyzer build from the workspace's
`target/release` and `target/debug` directories, and finally falls back to
`rils-analyzer` on `PATH`.

For another analyzer location, set `rils.server.path`.

## Architecture

The extension owns only VS Code integration and TextMate grammar files.
Parsing and semantic analysis stay in the Rils Rust crate, while the
editor-neutral protocol implementation lives in `tools/rils-analyzer`. This
keeps the analyzer reusable by a future Rider plugin or another editor.
