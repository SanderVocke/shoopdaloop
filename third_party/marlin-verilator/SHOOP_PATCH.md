# Local patch to Marlin's Verilator bridge

Source: published `marlin-verilator` 0.21.1 (MPL-2.0), from
https://github.com/ethanuppal/marlin. The other Marlin crates remain pinned to
0.21.1 from crates.io.

The generated FFI used `new Vtop{}`, sharing Verilator's default context.
Verilator 5.032 registers each model's trace callback in that context and does
not unregister it in the generated model destructor. Opening a trace after an
earlier model is dropped can invoke stale callbacks: reproduced as a native
crash and corrupt VCD scope names in the Rust integration suite.

`src/build_library.rs` now allocates one `VerilatedContext` with each model,
enables tracing on that context when configured, selects it for evaluation and
trace opening, and deletes it after the model. Before deleting a context, it
clears Verilator's current-thread pointer. The Rust driver closes traces before
destroying models. This establishes model/trace/context ownership without
retaining leaked models or serializing tests to conceal the failure.

`src/lib.rs` adds a cache discriminator so old compiled bridges are rebuilt.
Two syntax-only fixes keep this local dependency warning-free on Rust 1.99
(an explicit elided lifetime and statement-position error macro).
Runtime options additionally expose bounded C++ build parallelism and explicit
source-path relocation for statically declared models. This lets the host use
configurable source/cache locations without a shell executable wrapper.
The model cache key includes runtime source and include paths so switching RTL
roots cannot reuse a model from an older source location.
Build diagnostics go to stderr so framed host readiness on stdout stays intact.
No signal layout changes. Remove this vendor patch when an
upstream bridge release provides equivalent context ownership; retain the
multiple-model, multiple-runtime and waveform regressions when upgrading.
