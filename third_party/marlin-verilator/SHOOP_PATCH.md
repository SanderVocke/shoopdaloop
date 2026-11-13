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

Deletion first selects the model's owned context: Verilator's exported-scope
destructors consult the thread's current context. Destroying several compiled
models otherwise accesses the context selected by the last evaluation, which
may already have been deleted. The core regression and AXI endpoint matrix
exercise this with overlapping models and public memory scopes.

Library initialization also avoids the legacy global `traceEverOn` call. Tracing
is configured on each owned context during model construction. Touching an
implicit context while recreating a runtime can use a deleted context and hang
in its mutex. Repeated runtime-creation and existing fault/recovery tests cover
that lifetime boundary without opening VCD files.

`src/lib.rs` adds a cache discriminator so old compiled bridges are rebuilt.
Two syntax-only fixes keep this local dependency warning-free on Rust 1.99
(an explicit elided lifetime and statement-position error macro).
Runtime options additionally expose bounded C++ build parallelism and explicit
source-path relocation for statically declared models. This lets the host use
configurable source/cache locations without a shell executable wrapper.
The model cache key includes runtime source and include paths so switching RTL
roots cannot reuse a model from an older source location.
Build diagnostics go to stderr so framed host readiness on stdout stays intact.

The full parameterized test suite also uses the dynamic model API:

- `dynamic.rs` binds typed getter/setter function pointers once, avoiding repeated
  dynamic-library symbol lookups for every pin on every clock edge.
- `pin_value` copies a short-lived packed input during the call. It checks port
  direction and packed slice length, and converts scalar values using the actual
  port width before calling FFI. This avoids calling a setter with the wrong C
  signature when, for example, a `u64` value is supplied to a one-bit port.
- `read_scalar` avoids allocation for scalar observations; `read_value` retains
  packed reads. Dynamic models can open a single VCD trace through `open_trace`.
  The trace continues to borrow the runtime that owns its model/context.

Signal layout is unchanged. These are additive dynamic APIs; existing typed
models retain their API. The native `rt_fpga_testbench` core regressions cover
129-bit temporary writes, parameterized widths, independent model instances,
internal probes, and VCD context lifetime/timescale. Remove the local changes as
equivalent upstream APIs and context ownership become available, retaining those
regressions when upgrading.

The vendored README omits comparisons with retired test frameworks; licensing
and upstream attribution are retained.
