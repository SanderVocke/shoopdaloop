# Prepared native engine service

A bounded headless composition of the public `shoop_loop_engine` facade and the
independent `shoop_example_dsp`. One graph/render owner runs stereo live input and
loop playback through the DSP to output and capture. This is a narrow native
integration slice, **not** the application backend, a plugin host, feature parity,
or a browser/IPC protocol.

## Preparation and hosting

`prepare(config, presets, initial_preset)` returns `Prepared { client, render,
route, target }`. Preparation creates the roles, graph, preset storage, queues,
MIDI index/capture storage and an alternate **Live** capture route off-render.
This constructor starts with an empty loop. `prepare_with_content(config, presets,
initial_preset, InitialContent)` instead imports planar stereo samples, ordered
`InitialMidi` packets and their strictly-pre-first-packet `MidiState`. Loading is
off-render: audio becomes interleaved little-endian floats and MIDI uses the public
indexed resource format. Leases retain both immutable versions; mutable startup
resource handles are removed rather than leaked into stopped cleanup. Preparation
uses finite resource upload accounting, never a silent engine callback.

- `Client` is the single control-side producer. Move it to the control thread;
  do not share it as multiple producers. Its typed loop requests and `dsp()` client
  remain distinct domains.
- `Render` is the single render owner. A host supplies contiguous `AudioBlock`
  frames with two input/output channels and one MIDI input/output port.
- For each valid running nonempty callback, `Render::process` snapshots bounded
  ingress and calls the existing loop engine exactly once. The resolver remains
  authoritative; there is no service clock, nested runtime or callback splitting
  to renew quotas.
- Initial roles/graph are ready before frame zero. Initial graph epoch is **1**;
  initial resolver/topology epoch is **0**. These are different counters.
- `render.midi_recovery()` is a separate reserved output. Deliver its packets as
  well as ordinary MIDI before the next call. Ordinary playback restoration may
  include canonical pitch/hold state, not just recorded note packets.

`Client::stop` stops the loop through recovery while live DSP processing continues.
Dropping the client instead requests a separately reserved render-owned stop,
mutes the output, emits device resets, and reaches `Phase::Quiescent` after stop
settles. Keep callbacks running until that cleanup is delivered. A fatal process
error is not a replay request: stop host callbacks and consume `into_stopped()`.
`Stopped::cleanup()` reclaims owners off-render; it does not execute future work
or send packets to a device. Destruction is off-render.

## Typed control and visibility

`loop_at` admits the supported transition/selection/gain/state/export/cursor/
release and fixed-role graph operations. Native admission is not runtime admission
or actual execution:

1. A successful call reserves a service credit and returns a component-checked
   cancellation `Ticket`.
2. `try_delivery()` returns the original request or actual facade command/retired
   owners. Inspect `phase`, `outcome`, and `terminal`. Poll/dispose these **off-render**.
3. Nonterminal domain deliveries have a separate `try_execution()` record with
   the actual loop receipt. A service `Aborted` is explicitly not a fabricated
   loop execution. Drain owning and semantic streams independently.

Retaining completed tickets retains their cancellation flags. After all flag
storage is retained, new admission truthfully reports `Busy`, even if result
credits are free. Drop unused tickets off-render. Foreign tickets/routes refuse
without consuming credits.

Loop lateness follows the existing scheduler: requested, delivered and terminal
frames remain observable. The independent DSP rejects late activation instead of
silently clamping; its applied receipt names the exact frame and revision.
Callback-end work waits for a later nonempty callback. Zero-frame calls neither
snapshot ingress nor execute sample-bound work or renew quotas; reported work is
zero.

`client.dsp().select(at, preset, expected_revision)` uses the component-owned
prepared bridge, not a float parameter or a new loop operation. `read_peaks()` on
`client.meters()` atomically consumes peak-since-read telemetry without a reliable
query. Repeated state/meter reads do not drive musical processing. A loop state
cut is coherent in the loop domain; DSP receipts/meters do not create a cross-domain
atomic transaction.

`stage_route` transfers a prepared candidate. Its actual StageGraph delivery names
a Ready ID. Submit GraphActivate with current graph/topology epochs to publish the
fixed-role route at its frame. The candidate changes capture from post-DSP to live
input, preserving physical roles, the current take and loop state. Retired graph
storage is independently released or reclaimed when stopped.

Exports retain immutable public leases. Obtain cursors, submit preallocated owning
read buffers, and inspect actual read replies/returned buffers. A cheap lease is
not zero-copy delivery. Do not release an export before its pending reads return.
Optional refusals are domain outcomes, not permission to stop admitted recording.

## Fixed capacities

| Storage/work | Prepared bound |
| --- | --- |
| Host block | 1–64 frames; configurable maximum no greater than 64 |
| Service normal | 16: eight domain operations plus eight bulk/graph/read requests |
| Service recovery | One; separate internal disconnect-stop pending slot |
| Native returns | 17 owning slots; ten semantic slots; retained fault spills |
| Facade callback | 16 commands/publications; 32 scheduled events; 128 validation steps; 1,024 optional-transfer bytes |
| Runtime host MIDI | 2,048 events / 8,192 bytes per port |
| Loop role MIDI | 2,048 events; 512 packet/data bytes per interval |
| Capture | Two groups/banks; stereo audio plus MIDI; 32,768 staging bytes/group |
| MIDI index | Eight versions; 32 checkpoints/version; 128-event replay interval |
| Resource pool | 32 resource/version/snapshot slots; default 4,096 × 256-byte pages |
| Export leases | Four by default; configurable off-render |
| Device recovery sink | 64 packets / 192 bytes, separate from ordinary MIDI |

DSP storage has its own checked `dsp::Config`. Finite capacity is not a promise
that every possible MIDI restoration or retained take fits. Inspect refusals,
capture failures and overflow diagnostics. No callback-deadline claim follows.

## Complete public scenario

Run the [one_track example](examples/one_track.rs):

```sh
RUSTFLAGS="-D warnings" cargo run --locked -p shoop_engine_service --example one_track
RUSTFLAGS="-D warnings" cargo run --locked --release -p shoop_engine_service --example one_track
```

A separate stopped-input case verifies a live held note observed before Record,
its exported initial state, and exact cleanup. The main scenario imports four
stereo/MIDI frames, plays frame zero, records a distinct eleven-frame take, exports
a live prefix, changes capture routing at frame 4 and selects the DSP preset at 5.
It reads/rereads the unchanged stereo/MIDI prefix and all sixteen initial-state
channels while recording continues. Double release and a stale cursor refuse
without stopping recording. Playback begins immediately at interior frame 12,
wraps, seeks at 25, selects the exported original at 35 after its resident reference
was retired, stops with MIDI cleanup at 40, and releases/reclaims every owner.

Whole-per-phase, single-frame and irregular/zero callbacks run with both valid
query declaration orders. Required off-render barriers learn graph IDs, leases
and cursors; they are not extra internal engine callbacks or warm-up blocks.
Every run checks all **42 stereo frames, 487 absolute MIDI packets, 378 copied media
bytes**, coherent cuts, metadata, results and peaks. The input event table is
prepared; input adaptation and rendering are heap/free guarded. Control-side
journaling, owner disposal and printing are outside rendering.

To verify a mismatch really fails the executable, use `-- --inject=sample`,
`-- --inject=packet` or `-- --inject=outcome`; each deliberately fails an oracle and
exits nonzero. These are negative checks, not successful scenario executions.

## Verification

Run inside the repository development environment (on Nix: `nix develop`):

```sh
RUSTFLAGS="-D warnings" cargo test --locked -p shoop_engine_service
RUSTFLAGS="-D warnings" cargo test --locked --release -p shoop_engine_service
RUSTFLAGS="-D warnings" cargo build --locked -p shoop_engine_service --target wasm32-unknown-unknown
python3 scripts/run_wasm_tests.py --profile dev --runtime node --package shoop_engine_service
```

Thirteen portable guarded tests cover startup loading, the complete public
scenario and exact full audio/MIDI partition oracles; coherent queries, typed
preset application and route cuts; zero/end/late/cancelled/refused work; retained/
foreign tickets; meter polling, stalled results/disconnect and faults before/after
publication. The public scenario asserts all pages/versions/snapshots, MIDI-index
rows and media/export/pin pools are free after idempotent stopped cleanup. Native,
Wasm compilation and actual Node execution are distinct evidence; none is a new
AudioWorklet, physical-device, latency-compensation or full-feature-parity result.
