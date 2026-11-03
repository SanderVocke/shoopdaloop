# Direct fixed-tick primitive loops

`shoop_loops::fixed` is the primitive-loop part of the fixed-quantum redesign.
It is not yet selected by the production application. Sync/program execution,
live graph/media editing, export transport, session conversion and application
configuration remain migration work. The existing library and application path
remain available until parity is verified.

1. Prepare `rt_runtime::fixed::Config` with Q=16/32/64/128 and independent host
   capacity. `Bank::resource_store` prepares capture groups and MIDI indexes.
2. Import source snapshots into that store. Sources must cover an aligned loop
   duration; unaligned content is refused without truncating or padding media.
3. Supply `Binding`s and initial `media::Input`s to `Bank::prepare`. Ownership of
   the store and leases transfers into preparation. Construction failures dispose
   of the prepared objects outside RT.
4. Install both physical consumers per loop in the graph. `Prepared::runtime`
   returns the control producer and the fixed runtime. To include effects, wrap
   `Node` in an application processor enum and use `fixed::Runtime::prepare`.
   Forward `validate_state` when wrapping loop nodes. State/port/identity binding
   and the presence of both consumers are validated before rendering.
5. Submit copy-valued Play, Stop, Seek, Gain, Record and State commands through
   the prepared SPSC control endpoint. Poll every terminal receipt; held results
   deliberately retain admission credit. Callback ingress obeys
   `capacities.budgets.commands` once per host callback. A separate cancellation
   credit remains available when normal admission is full. Effective times round
   upward, while seek positions must already be aligned and inside content.

Primitive commands currently apply in submission order at a tick boundary.
Domain-specific supersession and sync/program request-family arbitration must be
completed before replacing the production resolver. A State reply observes the
state after earlier commands at that boundary, before graph processing.

Playback retains immutable read indices until a take is published. Positions and
durations are tick counts; MIDI packets still have sample offsets within ticks.
Audio and indexed MIDI renderers run directly against cached state, without
planner decisions or observation acknowledgements.

Recording uses two prepared audio/MIDI capture banks per loop. Starting a take
reserves metadata, versions, final snapshot credits and the first complete tick
before changing the mode. Subsequent ticks reserve their storage before any
consumer writes. Publication freezes both streams together at a boundary and
updates cached playback sources. Content revision remains distinct from the
active capture revision while previous content is monitored.

If the next complete tick cannot fit, the preceding complete prefix is frozen
with an explicit capture failure and recording stops. No partial quantum becomes
the loop length. A failed recording start leaves existing playback intact.
Required capture data work uses render accounting rather than renewing background
copy budgets. Severe renderer/MIDI-output failures fault the runtime and use its
reserved MIDI reset output; no callback can be replayed.

After callbacks stop, recover `(Bank, ResourceStore)` with `Runtime::into_parts`
and call `bank.cleanup(&mut store, frame)`. Cleanup releases internal capture
resources and source leases. Original imported resource identities remain the
caller's property. Rendering may not resume after cleanup. Pending command
shutdown/projection semantics are part of the remaining application integration.

Run the checked three-node-per-loop playback workload with:

```sh
cargo run --locked --release -p shoop_loops --example fixed_quantum_benchmark -- \
  32 64 two-bursts 8192 callbacks.csv
```

Arguments are Q, stereo loop count, `steady`/`two-seeks`/`two-bursts`, measured
callbacks and optional raw CSV path. Transition cases require Q=16/32 and place
changes at offsets 32 and 96. Every sample, terminal result, state position and
graph visit count is independently checked outside timing. All percentages use
the entire 128-frame/48-kHz callback budget; outliers and misses are retained.
Preparation, submission, validation and file I/O are outside the measured callback.
This synthetic native workload is not device acceptance or plugin coverage.
