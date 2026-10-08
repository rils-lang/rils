# rils_native

`rils_native` defines the runtime-independent registration interface for native Rils values. A registration contains function pointers for layout construction or borrowed element reads; it does not retain runtime values or execution state.

`rils_stdlib` owns the registrations for built-in types. `rils_execution` queries the registry while resolving layouts and accessing native collections. The interpreter and bytecode VM share those results.

`EqualityRegistration` compares borrowed Rust leaves or composed views without constructing execution values. `of<T>()` uses Rust `PartialEq`; `projected<Wrapper, T>()` accepts the wrapper and its leaf through `AsRef<T>`, after checking both Rust and Rils identities. A composed type supplies its own visitor with `view()`, so collection ordering stays with the collection implementation. Missing leaf registrations return an error.


`KeyRegistration` reads a checked `&NativeLeafRef` and declares whether its identity preserves total ordering with `ordered`. `key_leaf()` handles standalone wrappers and raw composed leaves; `key()` recursively visits records, variants and options. `ordered_key()` requires a registered ordered leaf. The registry has no built-in scalar dispatch: standard-library declarations and macros register primitive keys. Collection declarations may impose stricter element bounds; BinaryHeap derives its check from the same `HeapElement` implementation list. Execution checks the expected collection type and nominal `Eq + Hash` bounds. Identity extraction never requires `Clone` on the source payload, though `NativeKey` may allocate owned strings and nested identity data.

Rust API migration: change key callbacks from `DynamicValueRef` to `&NativeLeafRef`, use `with_rust` for checked reads, and set `ordered` explicitly. Custom registries must register primitive keys they support as well.
