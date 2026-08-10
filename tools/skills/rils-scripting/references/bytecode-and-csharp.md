# Bytecode and C# embedding

Use this reference for `.rilbc`, `rils_capi`, `Rils.CSharp`, or Unity asset packaging. Treat repository-local API
and format documentation as authoritative when newer.

## Choose the input boundary

- Use `rils::compile(source)` only for source already held in memory. It performs no implicit file access.
- Use `rils::compile_file(path)` for an entry file with `mod name;`; it recursively resolves `name.rils` or
  `name/mod.rils` and links the complete source graph into one module.
- Compile `.rils` offline for a release build, then package only the resulting `.rilbc` when source distribution is
  unnecessary.
- Load bundled data from bytes. Do not unpack an AssetBundle or Addressables asset merely to recreate a source tree.

CLI workflow:

```console
rils compile scripts/main.rils -o scripts/main.rilbc
rils verify scripts/main.rilbc
rils run scripts/main.rilbc
```

Rust hosts use `BytecodeModule::to_bytes` / `from_bytes` or `write_file` / `read_file`.

## Respect the experimental format boundary

`.rilbc` v1 uses an explicit little-endian section container, not Rust enum or memory serialization. It records the
format version, language version, host ABI, target pointer width, required sections, and a CRC32 payload checksum.
Loading performs structural limits and the normal bytecode verifier before execution.

Do not describe v1 as cross-version stable. Because it can contain `usize` and `isize`, reject artifacts whose
32/64-bit pointer width differs from the runtime. Recompile artifacts for the target runtime instead of converting
their bytes manually.

Treat bytecode as untrusted input even when it is precompiled. Preserve instruction/call-depth limits and any host
capability checks. Compilation removes parser work from the player but does not make a module inherently trusted.

## Use the C and C# facades

The native library is `rils_capi` (`rils_capi.dll` on Windows). Keep P/Invoke declarations centralized in the
`.NET Standard 2.1` `Rils.CSharp` facade; application and Unity code should not duplicate them.

Use the high-level C# API:

```csharp
using Rils.CSharp;

using var runtime = new RilsRuntime();
using var module = runtime.LoadBytecode(scriptAsset.bytes);
using var instance = module.CreateInstance();
RilsValue result = instance.Execute();
```

Use `LoadBytecodeFile(path)` only when a real file is already the host contract. Development tooling may use
`Compile(source)` or `CompileFile(path)`, but release-time Unity loading should normally use `LoadBytecode(byte[])`.

For Unity Editor compilation, serialize the returned `RilsModule` with `GetBytecode()` or
`WriteBytecodeFile(path)`. The C ABI uses `rils_module_bytecode_size` followed by
`rils_module_write_bytecode`, or the direct `rils_module_write_bytecode_file` path. The memory form writes into a
caller-owned buffer; never add a cross-DLL free requirement for this workflow. Refresh Unity's AssetDatabase after
writing under `Assets`, then mark or move the `.bytes` asset into the project's Addressables group.

The matching C ABI entries are `rils_module_load_bytecode` and `rils_module_load_bytecode_file`. Input slices are
borrowed only for the call. Runtime/module/instance handles are thread-bound, generation-checked, and must be
disposed on their creating thread. A runtime disposes its child handles.

The current cross-boundary value protocol supports unit, bool, concrete integers, `isize`/`usize`, `f32`/`f64`, and
`char`. Do not claim strings, collections, Option/Result, host objects, callbacks, or persistent instance state are
available until the project facade exposes them.

## Build and validate in the Rils repository

Use the Python tooling from the repository root:

```console
python tools/generate-csharp-bindings.py --check
python tools/build-capi.py
python tools/export-unity-package.py
```

The Windows package contains `rils_capi.dll` and `Rils.CSharp.dll`; the C header is generator/documentation input
and is not staged as a runtime artifact. Do not change crate, analyzer, or plugin versions merely to rebuild these
files.

The Unity exporter produces a drop-in `Rils.CSharp` directory with flattened C# source files and an unsafe-enabled
asmdef. Native libraries live under `Internal/<architecture>/`; the current exporter provides
`Internal/x86_64/rils_capi.dll`. Use `--output <UnityProject>/Assets/Rils.CSharp` when exporting directly into a
project. The exporter replaces that complete directory, so keep application scripts and custom bindings elsewhere.
