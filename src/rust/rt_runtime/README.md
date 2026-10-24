# rt_runtime

A generic audio/MIDI runtime crate under development. It does not depend on Shoop domain types, device APIs, or browser bindings. Test support is dev-only.

**Current milestone:** validated capacities, bounded event storage, ownership transport, deterministic typed DAG compilation/validation, stable processor slots, and transactional graph execution. Sample-accurate scheduled controls, MIDI overload recovery, resources, and the real worklet fixture remain under development.

```rust
use rt_runtime::{Config, Frame, RenderRuntime};
use rt_runtime::buffer::AudioBlock;

let config = Config { audio_inputs: 0, audio_outputs: 1, ..Config::default() }.prepare()?;
let mut runtime = RenderRuntime::new(config);
let mut channel = [1.0; 128];
runtime.process(AudioBlock {
    frame: Frame(0), frames: 128, inputs: &[], outputs: &mut [&mut channel],
})?;
assert_eq!(channel, [0.0; 128]);
# Ok::<(), rt_runtime::Error>(())
```

Run `cargo test -p rt_runtime` and `cargo test -p rt_runtime --release`. Tests install a counting allocator with positive allocation/reallocation/deallocation canaries; the same assertions run in release and Wasm. Counts are measured only inside render scopes, not during test framework setup. A separate audit of callback-reachable code is required for locks/waits/I/O; allocator instrumentation does not detect those.

Run `python3 scripts/check_rt_runtime_boundary.py` from the workspace root to audit production/build dependencies on all supported targets. The shared repository test attribute is a dev-only exception. Registering the browser-test feature must not introduce test support into the production dependency tree.

## Graph preparation

`graph::Catalog` records prepared processor IDs and typed port signatures. `compile` lowers a `GraphDescription` into a deterministic immutable `PreparedPlan`, grouping incoming routes before each topologically ordered node and host-output routes last. No feedback graph is accepted. Host endpoints select inputs when used as sources and outputs when used as destinations; node endpoints select outputs as sources and inputs as destinations.

`PreparedPlan::from_descriptor` also envelopes untrusted wire descriptors on the control side. It checks only storage limits, **not execution validity**. `PlanValidator` must certify the plan against the live catalog before activation. Initialize the validator outside rendering, call `begin`, and repeatedly call `advance` with the configured validation budget while keeping the candidate and catalog unchanged. Each step performs constant work and the tests guard all advances against heap operations. Raw descriptors contain no pointers or scratch offsets; validated node/port identities will determine scratch placement inside the executor. Do not execute a merely enveloped plan.

## Graph execution

`executor::GraphRuntime::prepare` accepts prepared processor slots and returns a control endpoint plus the single-owner runtime. Node signatures are captured outside rendering. Slot scratch is allocated and touched during preparation; `Processor::process` receives planar `AudioPorts` plus bounded MIDI input/output buffers. No temporary vectors of slice references are created during processing. `Box<dyn Processor>` is supported, or use a concrete application-defined processor enum.

Submit `executor::Command::commit(expected_epoch, plan)` on the normal lane. Validation progresses under the callback's step budget while the old graph keeps rendering. A valid plan swaps atomically before that callback's samples; its displaced plan returns in the owning completion. Invalid or stale plans remain in their rejected completion for control-side disposal. Host output routing follows the last node in the same callback: no extra signal block and no clipping.

Rewiring does not recreate processors. Remove a processor only after committing a graph that no longer references it; removal returns it for non-RT destruction. Removal advances its generation and the graph epoch. Reinstall a prepared processor with the next-generation ID and identical port counts into that reserved slot. This first implementation reserves slot/port capacity up front rather than performing dynamic buffer allocation during rendering. Structural edits all use the normal lane; recovery-lane structural commands are rejected to avoid reordering hazards.

`GraphRuntime::process` takes a host `AudioBlock` and `MidiBlock`. It validates inputs, stages bounded control work, renders the graph, and publishes Copy diagnostics. Zero-frame calls do not consume control work. Invalid host shapes clear only bounded writable outputs and do not advance the clock. MIDI fan-in currently uses stable timestamp-sorted descriptor insertion with atomic source-capacity refusal; full message validation and overflow recovery are the next milestone.

## Control ownership transport

`transport::channel<T, R, S>` creates control/render endpoints outside rendering. `T` is an application-defined operation payload, `R` is an owned retirement bundle, and `S: Copy` is a coherent telemetry value. The transport never executes queued closures. `submit` is control-side only; it returns a cancellation ticket or returns the original payload on refusal.

One admission credit reserves an operation, its terminal completion, and an optional retirement bundle until the control side calls `try_completion`. The bundle can hold displaced objects prepared by the caller. Rendering borrows the current request with `next_operation`, then uses `complete` to return both ownership and a compact outcome. Errors from `complete` return the retirement bundle and must be retained rather than dropped on RT. Dropping a completion on the control side performs reclamation.

Call `begin_cycle` once per callback to snapshot visible work and set frame/epoch. Dequeues, automatic cancellation completions, and telemetry each have explicit limits; terminal and telemetry publication budgets are separate so telemetry cannot steal terminal capacity. Recovery admission is reserved independently and is serviced first; FIFO is preserved **within** each lane, not globally across them. Only use recovery for operations that do not need ordering behind normal work. A held current operation must finish before another can be borrowed, so lengthy staged transactions must not hold the dispatch slot while waiting for further commands.

Cancellation is observed once at dequeue. Cancellation after that cut, including after application, does not undo an operation. A completion is the authority, not the ticket flag. Requests submitted after the callback's queue snapshot wait for the next callback. Sequence exhaustion refuses new requests instead of wrapping.

There is no internal thread, blocking drain, or hidden executor. A disconnected control endpoint does not free render-owned objects: continue rendering or park the endpoint, then stop/join host callbacks before destroying it on a non-RT thread. A disconnected render endpoint rejects new submissions and returns their ownership. Destroying both endpoints after stopping safely reclaims held, queued, and completed payloads; clients must also release their received completions/tickets. Do not destroy the render endpoint inside a callback.

See `docs/rt_runtime/CONTRACTS.md` and `docs/rt_runtime/VALIDATION.md` at the workspace root for intended API contracts and actual milestone evidence. `PLAN.md` and `DESIGN.md` define the remaining implementation and scope. No Shoop integration is included.
