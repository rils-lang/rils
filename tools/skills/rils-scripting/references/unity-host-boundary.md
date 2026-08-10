# Unity host boundary

Use this reference only for Rils scripts embedded in Unity. The Unity project's binding manifest and examples are authoritative.

## Separate responsibilities

Keep Unity lifecycle, serialization, asset loading, engine callbacks, and native interop in the C# facade. Put gameplay rules, state transitions, UI flow, quests, skills, and other frequently changed logic in Rils. Keep rendering, physics, large numeric loops, and other measured hotspots in C# or native code unless an AOT Rils backend is explicitly available.

For the current Windows integration, keep `rils_capi.dll` as the architecture-specific bridge and reuse the
`Rils.CSharp` source or managed assembly as the platform-neutral facade. In Unity's native plugin importer, enable
Editor and Standalone only for the intended Windows x86_64 target. A different Player architecture needs a matching
native build even though the C# facade itself is AnyCPU IL.

## Discover before calling

Before writing a host call, locate all of the following when the project provides them:

- exported module and function names;
- exact parameter and return types;
- object/asset handle types and invalidation behavior;
- granted capabilities;
- lifecycle entry points;
- reload state schema and migration hook;
- thread restrictions.

Do not infer a binding from a similarly named Unity API. Ask for or propose a C# facade addition when a required operation is missing.

## Design script APIs

Prefer coarse operations that cross the host boundary once:

```rust
// Prefer a project-defined batch operation.
game::movement::apply(commands)
```

Avoid repeatedly fetching and setting individual Unity properties inside large Rils loops. Keep per-frame allocations and owned clones visible and intentional.

Use host handles only according to their declared type. Never serialize VM references, raw pointers, active calls, or captured references as durable state.

## Prepare for hot reload

Model persistent data as owned, versionable state. If the project defines reload hooks, keep migration transactional:

1. Export supported owned state from the old module.
2. Load and validate the new module before switching.
3. Import or migrate state into the new module.
4. Activate the new module only after migration succeeds.
5. Preserve the old module when compilation or migration fails.

Keep transient caches reconstructible. Treat registered callbacks and event subscriptions as resources that must be detached when the owning script instance or module is unloaded.

For release assets, compile the entry `.rils` and all `mod` dependencies into one `.rilbc` before building the
Player. Store the bytes in the project's chosen AssetBundle or Addressables representation and pass them to
`RilsRuntime.LoadBytecode(byte[])`. Do not require the original module directory inside the Player. Keep source in
Editor-only assets only when development-time compilation or source diagnostics require it.

The current `RilsInstance` is a lifecycle boundary for module execution and public function calls, not yet durable
per-instance script state. Do not design reload migration around persistent VM globals until the runtime exposes
that contract.

## Keep lifecycle calls explicit

Follow the project's declared lifecycle interface. Do not assume that every script implements every callback. Make optional callbacks explicit in host metadata or in the project's component contract rather than emulating reflection inside Rils.

Unity APIs normally require the main thread. Do not initiate background host access unless the binding contract explicitly permits it. Rils compilation may run away from the main thread only when the runtime API and project integration declare that safe.

## Handle errors

- Return expected gameplay and host failures as structured `Result` values.
- Include `.rils` source identity and Span in compile/runtime diagnostics.
- Add gameplay context at the C# boundary without replacing the original Rils diagnostic.
- Keep the currently active module running after a failed hot reload.
- Enforce instruction, call-depth, memory, container, string, and host-call budgets when the runtime exposes them.
- Treat `RilsStatus.BytecodeError` as a load/verification failure and retain the previously active module.
