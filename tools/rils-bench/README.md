# Rils Benchmarks

`rils-bench` is an opt-in benchmark tool. It measures public Rils APIs and deliberately does
not add benchmark-only hooks to the runtime. Its Rils workloads live as editable files in
`cases/`; the runner compiles those files during benchmark setup and supplies the workload as a
function argument.

Run it from the repository root through the stable Python entry point:

```console
python tools/benchmark.py vm-integer-loop
```

Measure the fixed public bytecode-call cost before evaluating loop changes:

```console
python tools/benchmark.py vm-empty-call
```

Compare the supported counter integer types with the same workload:

```console
python tools/benchmark.py vm-counter-loop --integer-type i32
python tools/benchmark.py vm-counter-loop --integer-type u32
python tools/benchmark.py vm-counter-loop --integer-type i64
python tools/benchmark.py vm-counter-loop --integer-type u64
python tools/benchmark.py vm-counter-loop --integer-type usize
```

The tool emits one JSON record containing wall-clock and allocation metrics. The Python wrapper
adds source revision and host metadata, then writes the record beneath `target/benchmarks/`.
Those machine-specific results include the case filename and source revision in their metadata;
they are generated artifacts and must not be committed.

The initial `vm-integer-loop` scenario compiles once, warms up the bytecode VM, then measures
repeated execution of a fixed integer loop. It is intended to expose instruction-dispatch,
register, and frame-allocation regressions without including parse or compile time.

`vm-counter-loop` performs only typed comparison and increment operations. It is the appropriate
scenario for comparing `i32`, `u32`, `i64`, `u64`, and `usize` loop behavior without mixing in
accumulator overflow constraints.

`vm-empty-call` uses the same public bytecode call path but returns its integer argument directly.
Compare it with `vm-counter-loop` to separate verifier, exported-function lookup, Host linking,
and frame setup from per-iteration execution cost.
