# rils_native

`rils_native` defines the runtime-independent registration interface for native Rils values. A registration contains function pointers for layout construction or borrowed element reads; it does not retain runtime values or execution state.

`rils_stdlib` owns the registrations for built-in types. `rils_execution` queries the registry while resolving layouts and accessing native collections. The interpreter and bytecode VM share those results.
