---
name: rils-scripting
description: Write, review, refactor, diagnose, precompile, and embed Rils (.rils/.rilbc) scripts using the language's current concrete numeric types, explicit move/Clone ownership, lexical references, modules, traits, collections, Result-based errors, bytecode VM, C API, and C# facade. Use for Rils gameplay or application logic, multi-file module compilation, bytecode packaging, and integration with embedded hosts such as Unity without inventing unavailable bindings.
---

# Rils Scripting

## Establish the project contract

1. Read the nearest `AGENTS.md` and project documentation before editing.
2. Search existing `.rils` files to learn naming, module layout, entry points, and conventions.
3. Locate the host API declaration, generated binding file, or examples before calling Unity or other host functions.
4. Treat APIs not present in those sources as unavailable. Propose the missing binding separately instead of fabricating it in Rils.
5. Determine the Rils version used by the project. Use [references/language-0.1.md](references/language-0.1.md) as the bundled 0.1 baseline; prefer project-local documentation when it is newer.
6. Determine whether the host consumes source, an in-memory module, or a `.rilbc` artifact. Do not assume a runtime file layout when the host uses bundles.

Useful discovery commands:

```console
rg --files -g "*.rils" -g "AGENTS.md" -g "*rils*"
rg -n "host|binding|module|capability|pub fn|\.rils" .
```

## Write idiomatic Rils

- Use explicit parameter and return types at public/module boundaries.
- Use concrete scalar names (`i8` through `i128`, `u8` through `u128`, `isize`, `usize`, `f32`, `f64`, and `char`). Do not use the removed `int` or `float` names.
- Keep Unity-facing functions small and move reusable logic into ordinary typed functions and owned data types.
- Use the final expression of a block or function as its value; add a semicolon only when discarding that value.
- Model missing data with `Option<T>` and failures with `Result<T, E>`; never use `nil` or exceptions.
- Use `match` when handling all variants. Use `?` only inside a compatible `Result`-returning function.
- Use precise `fn(A) -> R` function types for callbacks; reserve `function` for compatibility with an opaque native callable.
- Use `pub` only for declarations that must cross a module boundary.
- Prefer stable, coarse-grained host calls over many small calls inside per-frame loops.

## Preserve ownership and references

- Assume assignment, argument passing, return, and owning iteration move every non-Copy value.
- Call `clone(&value)` only when an independent owned copy is required; do not insert cloning merely to silence an ownership error without checking the data flow.
- Use `&T` and `&mut T` only as lexical local references or parameters.
- Never return a reference, capture one in a closure, store one in an owning value, or place one in a global.
- Allow multiple `&mut` references to the same place when useful; Rils intentionally does not enforce Rust's unique mutable borrowing rule.
- Remember that `for item in values` consumes an owned array or `Vec` in Rils 0.1. Clone deliberately if the original collection must remain available.
- Do not move or replace an owner while one of its places has an active reference.

Read [references/language-0.1.md](references/language-0.1.md) before implementing nontrivial ownership, trait, iterator, macro, or module behavior.

## Integrate with Unity safely

Read [references/unity-host-boundary.md](references/unity-host-boundary.md) when a task touches Unity lifecycle functions, engine objects, hot reload, persistence, or performance.
Read [references/bytecode-and-csharp.md](references/bytecode-and-csharp.md) when a task touches `.rilbc`, AssetBundle/Addressables, `rils_capi`, native plugin layout, P/Invoke, or `Rils.CSharp`.

- Keep engine objects behind project-defined opaque handles or host types; do not assume raw C# or native object access.
- Keep references and VM-local closures out of reload-persistent state.
- Represent reload-persistent state using owned structs, enums, arrays, `Vec`, `Option`, and `Result` values supported by the host serializer.
- Do not assume lifecycle names such as `start` or `update`; follow the project's declared entry-point contract exactly.
- Do not expose filesystem or other capabilities unless the host manifest explicitly grants them.
- Prefer `RilsRuntime.LoadBytecode(byte[])` for bundled Unity assets. Keep offline compilation separate from runtime loading.

## Validate changes

1. Run the project's documented Rils analyzer, check, or test command.
2. If working in the Rils source repository, run a script with `cargo run -- path/to/script.rils` and add an example or automated test when changing reusable language behavior.
3. For bytecode packaging changes, exercise `rils compile`, `rils verify`, and `rils run`; test corrupt or incompatible input as well as a valid round trip.
4. For C# facade changes, regenerate low-level bindings with the repository's Python generator, build the facade, and run its managed/native smoke test.
5. If the Unity project owns validation, run its EditMode or PlayMode tests and confirm diagnostics retain the `.rils` file and source span.
6. Exercise both the success path and relevant `None`/`Err`, move, and reload paths.
7. Report validation that could not be run; never claim that unregistered host bindings compile.

## Avoid unsupported Rust syntax

Do not introduce lifetimes, reference fields, `dyn Trait`, `where`, const generics, trait default method bodies, pattern guards, `|` patterns, `@` bindings, grouped/glob imports, or `crate`/`self`/`super` paths for the bundled Rils 0.1 baseline. Check project-local language documentation before using capabilities added after that baseline.
