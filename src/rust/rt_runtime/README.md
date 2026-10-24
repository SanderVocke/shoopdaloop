# rt_runtime

A generic audio/MIDI runtime crate under development. It does not depend on Shoop domain types, device APIs, or browser bindings. Test support is dev-only.

**Current milestone:** validated capacities, bounded event storage, ownership transport, deterministic DAG execution/activation, stable processor slots, exact-frame scheduled parameters/MIDI, and bounded MIDI overload recovery. The standalone revisioned upload/snapshot store is implemented; capture, resource/executor integration, and the real worklet fixture remain under development.

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

`GraphRuntime::process` takes a host `AudioBlock` and `MidiBlock`. It validates inputs, stages bounded control work, renders the graph, and publishes Copy diagnostics. Zero-frame calls do not consume control work. Invalid host shapes clear only bounded writable outputs and do not advance the clock. MIDI fan-in uses stable timestamp-sorted descriptor insertion. `MidiBuffer` accepts complete validated MIDI 1.0 packets, including bounded SysEx; hosts must resolve running status and separate interleaved realtime bytes first. Ordinary writer refusal is atomic. Routing overflow replaces the affected input with reserved reset packets rather than allowing a partially routed note stream to strand internal voices; downstream reset propagation is explicit in diagnostics.

## Scheduling and MIDI recovery

Submit `Command::Schedule { at, action, payload }` on the normal lane using absolute sample frames. Parameter actions call the processor's bounded `set_parameter`; MIDI actions inject a validated packet into a node input. `Outcome::Scheduled` is **admission**, including effective frame and lateness, not application success. Poll `Control::try_execution` for the separate reliable execution result; reading it returns the scheduler's slot credit at the next callback. A target removed before execution produces an explicit failure rather than addressing its replacement.

Initial admission is deliberately conservative: outstanding event/result credits are capped by `min(scheduled_events, budgets.events, budgets.subranges - 1)`. Every accepted event reserves a possible split even when several share a timestamp, guaranteeing bounds for arbitrary variable block partitions. A full far-future schedule or undrained execution results refuse new admission. No accepted event is silently shifted to fit a later callback. Late arrivals apply at the next available frame with lateness reported. Parameter values must be finite; unsupported parameters return execution errors.

MIDI payload staging shares the callback transfer-byte budget. A queued message waiting for copy budget is not yet admitted. Scheduled events are ordered by absolute frame, node, parameter/MIDI kind, parameter or port, and sequence. Graph-routed MIDI sources retain canonical source order, with scheduled local injections after those sources at equal timestamps. Rendering splits at scheduled frames; MIDI offsets are rebased into each processor subrange and back to the host block.

Each MIDI arena includes reserved space for 32 reset messages/96 bytes beyond its ordinary capacity. On internal route overflow, the processor receives CC120 then CC123 on all 16 channels through this reserved space. Ordinary events cannot overwrite it. On output/propagated overflow, the runtime suppresses ordinary host MIDI and holds an independent reset state per output; audio processing continues. Processors must handle reset messages and continue their audio work even when an ordinary MIDI writer refuses output.

The host **must service** `drain_midi_recovery(port, reserved_sink, frames)` before delivering ordinary MIDI for that output. Use a separately reserved, preallocated sink and consume accepted reset messages; full sinks leave recovery pending. Repeated calls share a per-port per-callback `budgets.events` attempt limit. Repeated overflow does not restart an unfinished reset, and normal output resumes only after recovery drains. Internal reset work is separately bounded by 32 messages per affected port; ordinary per-port capacities and graph-size bounds still apply. `Command::ResetMidi` can use reserved recovery-lane admission when normal control is full.

## Fixed-page resources (standalone milestone)

`resource::ResourceStore` owns preallocated page bytes and fixed identity/version/snapshot pools. Create a resource, begin an upload against its expected revision, send exactly ordered chunks, finish validation, and commit. Only complete versions can become active. Audio storage is interleaved little-endian `f32`; MIDI storage uses little-endian `u64` frame/`u32` packet-length records and complete MIDI 1.0 packets. Duration is explicit, not inferred from the last MIDI event.

Uploads reserve page credits before accepting data. Chunk length is limited by remaining page space and the callback transfer-byte budget; MIDI chunks also require one validation step per byte. A failed parser attempt consumes validation work but does not modify the stored prefix. `begin_callback` replenishes budgets once per callback, not once per request. `Budget` is explicit refusal, not success or implicit queuing.

`snapshot` pins an immutable uploaded revision. Use its generation-checked cursor with bounded `read` calls; each call copies at most the remainder of one page. Poll currentness to distinguish a current version, replacement, or removed identity. A snapshot remains readable after replacement/removal. Release leases explicitly, cancel unused uploads, and call `reclaim` to incrementally recycle retired pages/descriptors within the remaining maintenance/validation budget. Final store destruction belongs after callbacks stop.

`resource_slots` separately bounds resource identities, version/transfer descriptors, and snapshot leases. Long-lived readers retain old versions and can exhaust the fixed page pool; refusal is intentional. Exhausted handle generations permanently retire their slots instead of wrapping. Capture-prefix support and integration of resource commands/shared budgets into `GraphRuntime` remain pending. See `docs/rt_runtime/RESOURCES.md` for the complete stage-5 contract.

## Control ownership transport

`transport::channel<T, R, S>` creates control/render endpoints outside rendering. `T` is an application-defined operation payload, `R` is an owned retirement bundle, and `S: Copy` is a coherent telemetry value. The transport never executes queued closures. `submit` is control-side only; it returns a cancellation ticket or returns the original payload on refusal.

One admission credit reserves an operation, its terminal completion, and an optional retirement bundle until the control side calls `try_completion`. The bundle can hold displaced objects prepared by the caller. Rendering borrows the current request with `next_operation`, then uses `complete` to return both ownership and a compact outcome. Errors from `complete` return the retirement bundle and must be retained rather than dropped on RT. Dropping a completion on the control side performs reclamation.

Call `begin_cycle` once per callback to snapshot visible work and set frame/epoch. Dequeues, automatic cancellation completions, and telemetry each have explicit limits; terminal and telemetry publication budgets are separate so telemetry cannot steal terminal capacity. Recovery admission is reserved independently and is serviced first; FIFO is preserved **within** each lane, not globally across them. Only use recovery for operations that do not need ordering behind normal work. A held current operation must finish before another can be borrowed, so lengthy staged transactions must not hold the dispatch slot while waiting for further commands.

Cancellation is observed once at dequeue. Cancellation after that cut, including after application, does not undo an operation. A completion is the authority, not the ticket flag. Requests submitted after the callback's queue snapshot wait for the next callback. Sequence exhaustion refuses new requests instead of wrapping.

There is no internal thread, blocking drain, or hidden executor. A disconnected control endpoint does not free render-owned objects: continue rendering or park the endpoint, then stop/join host callbacks before destroying it on a non-RT thread. A disconnected render endpoint rejects new submissions and returns their ownership. Destroying both endpoints after stopping safely reclaims held, queued, and completed payloads; clients must also release their received completions/tickets. Do not destroy the render endpoint inside a callback.

See `docs/rt_runtime/CONTRACTS.md` and `docs/rt_runtime/VALIDATION.md` at the workspace root for intended API contracts and actual milestone evidence. `PLAN.md` and `DESIGN.md` define the remaining implementation and scope. No Shoop integration is included.
