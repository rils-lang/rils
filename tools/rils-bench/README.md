# Rils Benchmarks

`rils-bench` is an opt-in benchmark tool. It measures public Rils APIs and deliberately does
not add benchmark-only hooks to the runtime.

Run it from the repository root through the stable Python entry point:

```console
python tools/benchmark.py vm-integer-loop
```

The tool emits one JSON record containing wall-clock and allocation metrics. The Python wrapper
adds source revision and host metadata, then writes the record beneath `target/benchmarks/`.
Those machine-specific results are generated artifacts and must not be committed.

The initial `vm-integer-loop` scenario compiles once, warms up the bytecode VM, then measures
repeated execution of a fixed integer loop. It is intended to expose instruction-dispatch,
register, and frame-allocation regressions without including parse or compile time.
