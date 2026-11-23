# Live program loading implementation

Contract: [LOADING_DESIGN.md](../../LOADING_DESIGN.md).

## Allocation and binding on ARM

`rt_fpga_compiler::loading::Allocator` owns one current and one pending layout.
It reserves loop, typed value and history allocations against the current
occupancy, assigns physical generations, and binds instructions to those
addresses. Logical identity numbers do not consume physical address space;
logical compilation checks counts and target resource budgets, and allocation
checks the simultaneous old/new occupancy. Application track and loop tables grow
with the draft; they do not impose an extra graph-size profile. Graph descriptions
may span bounded command groups. `ActivateGraph` finishes a layout upload.
`ActivateCompositeGraph` finishes an edit and attaches a key the application
can use to cancel that edit. A capacity refusal names the exhausted resource
and preserves current ownership.
Retained identities must keep compatible state roles. Revisions and
slot generations use checked increments. Cancellation burns their identifiers;
it cannot make an old image current again.

Graph compilation supplies stable value bindings by owner (strip, loop or bus),
purpose and channel. Continuing loop media channel sets must match; incompatible
channel changes require a new loop identity. Smoothing histories are independent
per channel. Immutable coefficient identities include their value, so changing a
gain, balance or mute stages fresh configuration without writing active RAM.
Unchanged coefficients and runtime loop gain values remain owned and untouched.
Media views use the independently allocated view-state addresses, not loop cursor
slots. Reference, browser and Marlin media tables match the state-value capacity.
Only fresh views are prepared; background reclamation inspects actual active
media operands before clearing an unused view.

`RustTarget::build_image` and `RtlTarget::build_image` lower the same logical
program using that layout. Only new allocations receive initializers. Carried
RAM values are never copied from compiler defaults or snapshots. Sync source,
scratch destination and history operands are encoded in instructions. History
allocation therefore survives a changed scratch layout.

ARM holds the union of old and new allocations until the renderer answers.
An applied switch needs a completion fence before `Allocator::complete`.
A cancelled or refused switch releases the pending allocations after the
renderer retires the proposed image. The APIs assume one trusted loader owner. Raw component RAM
access remains available for explicit test fixtures and is not hardware isolation.

## Rust real-time path

The reference host keeps its state processor, job processor, media pool and frame
clock alive across graph changes. Complete `GraphView` snapshots, including unsupported shapes and MIDI information,
stop at the ARM endpoint:
compilation and allocation run there, outside processing. Typed bound artifacts
cross a bounded program-memory handoff. Unused media views are prepared
incrementally. The prepared hosts use a bounded, single-request switch queue
alongside the ordinary command FIFO. The switch can wait for an eligible
restart while playback commands continue through that FIFO.

`CancelCompositeGraph` identifies an edit by its key and base revision. It
passes through the image upload channel, which also reaches the renderer
while the switch waits. When the renderer receives cancellation before the
switch, it retires the image and reports a cancelled activation. A switch
that already took effect reports its applied revision and frame.
An admitted switch also settles if the application connection closes. The
loader uses the result to settle program ownership. After reconnecting, the
client can read the current revision through its usual snapshot subscription.

At each quantum boundary, complete command groups execute in FIFO order within
each queue. A switch changes the program, plan metadata, run parameters, control bindings and
revision without replacing the persistent state pool. Public application loop
identities translate to the committed physical bindings. Continuing gains and
snapshot metadata move with the instance, rather than a temporary binding index.

The header names the expected old revision; the small `SwitchProgram` record
contains the next revision, instruction region and instruction/sync/state/job/
buffer counts. The RT protocol includes complete graph uploads and
resource-specific and feature-specific refusals.
The FPGA profile uses
the same monotonic value for graph and topology. Old-revision commands following
a switch receive `StaleConfiguration`, including commands to retained loops.

When a sync source changes, the host clears plans tied to the old source at the
switching boundary. Ordinary command failure has no rollback guarantee.
Ownership follows the actual switch result, even if another command in its group
fails.

Retirement waits for `Memory::completion_fence_ready`. New Rust processing and
snapshots can continue while the completion is delayed, but the old allocation
set remains reserved. The old program object returns through a bounded reclaim
queue and is destroyed on the control side. Snapshot chunking is restarted for
the new revision; observing that revision does not prove retirement.
If the control-side reclaimer stops draining its queue, preparation waits while
audio continues. Fence errors retain the relevant image and ownership instead of
destroying its program storage during processing or returning a false completion.

`RtBackend` maintains an editable prepared graph and a committed runtime revision.
Structural mutations alter the prepared graph. The application service commits
one complete operation (including its default connections) automatically, and
tracks its completion through the ownership fence. Later structural changes
wait for the pending switch. Lower-level clients can still stage mutations and
request explicit activation. Immediate runtime commands use committed identities
and revisions. Failed preparation restores the last committed control-side mirror,
retaining monotonically allocated identities and slot generations; it does not
roll back executing processor state.

The Rust-reference host can be used with the UI through shared Dummy, CPAL or
JACK drivers. Its portable renderer has no private pacing thread. Backend
selection is a host startup setting. Unsupported operations use the existing
refusal path. The [Usage panel](../runtime_profiling.md#backend-resource-usage)
reports the active program and storage limits. See the
[host instructions](../../src/rust/shoop_host/README.md#rust-fpga-reference-with-the-ui)
for startup and the supported graph subset.

## RTL and Marlin

Enable `LIVE_LOADING=1` on `audio_pipeline`, `media_audio_pipeline` or its AXI
wrapper. This enables two instruction regions by default. `PROGRAM_DEPTH` is
capacity per region, with a 16-bit configuration address covering both regions.
The single-program component mode remains available for legacy raw fixtures.

ARM writes the inactive instruction range and fresh state slots through existing
configuration transactions. In the two-region mode, instruction writes use the
independent write port; state/loop writes use cycles where the SP does not write
that RAM. Reads stay serialized with execution. A held upload request cannot
win indefinitely over a pending run. JP configuration space 3 invalidates a new,
unused smoothing-history allocation; it does not clear continuing history.

The pipeline latches the switch descriptor at a paced boundary after the old
SP/JP run. Instruction-region selection and all run counts change together.
The global frame remains continuous. No state scan, copy, graph table, ownership
bitmap, image unpacker or second execution engine is added.

The media wrapper drains queued SP/JP hints, submits the existing media fence,
cancels unissued resolver lookups and drains issued cache/AXI work before
acknowledging. It conservatively holds the next quantum during that fence.
Completion remains asserted until accepted, while new processing may resume once
the fence itself is complete. Memory faults never become successful handoffs.

The Marlin and board upload agents consume the target's sparse image and drive
normal RAM transactions during execution. They use the dedicated switch queue
alongside the ordinary control queue. A switch group remains unread with its
response credit reserved until RTL reports completion. The hosts read the
same loop words for composite version and playhead snapshots. In simulation,
loading and carrier polling leave the simulation clock under the driver's control.
Component test fixtures can also supply initial plans directly.

## Tests and limits

CI runs compiler, functional-reference, protocol/transport and application-client
checks independently of the Marlin toolchain. The application check explicitly
enables `fpga-reference`; default host builds keep that dependency optional.
Compiler, control/backend and transport changes trigger the component workflow,
including on pushes to `feat/backend-replacement`.

The compiler, reference loader, reference FIFO and Marlin tests cover:

- Retained recording, playback, cursor/frame, gain ramps, future plans and history.
- Reordered programs, changed scratch destinations and sync-source operands.
- Fresh-state-only writes, arbitrary target address offsets, physical slot reuse
  and increasing generations, cancellation, stale revisions and deadlines.
- Preparation writes during execution and a continuously held preparation request.
- Paused processing, delayed memory fences, delayed AXI responses and completion
  backpressure. New snapshots never release old ownership.
- Real application graph replacements while recording and playing, changed input
  routing, and replacement of a user track with eight newly allocated loops.

Run the focused gates with:

```sh
cargo test -p rt_fpga_compiler -p rt_fpga_reference --all-features
cargo test -p shoop_rt_protocol -p shoop_rt_transport --all-features
cargo test -p rt_fpga_testbench --features sim --test loading --test targets --test sync
cargo test -p shoop_host --no-default-features --features fpga-reference --test prepared_graph
```

Quantum, sample rate, media geometry, physical I/O and hardware capacities stay
fixed. Only one update can be prepared or pending. The loader has no retry,
rollback, state migration, live compaction or parallel graph crossfade. Reset
requires quiescing execution and creating a new runtime/transport incarnation;
an ARM owner cannot reconstruct free space from snapshots after losing its map.

## Resource measurements

Yosys Xilinx 7-series cell mapping, with no I/O pads or clock buffers:

| Profile | LUTs | Flip-flops | RAMB36 | RAMB18 | DSP |
| --- | ---: | ---: | ---: | ---: | ---: |
| Standalone SP before live RTL, one 256-instruction region | 4,244 | 1,616 | 14 | 3 | 0 |
| Standalone SP with two 256-instruction regions | 4,213 | 1,617 | 14 | 3 | 0 |
| Current SP/JP, single-program mode, 64 instructions | 6,651 | 3,629 | 1 | 3 | 4 |
| Current SP/JP, live mode, two 64-instruction regions | 6,887 | 3,707 | 1 | 6 | 4 |

The standalone comparison uses the RTL at `f3fcf400` as its baseline, including
the inline sync-source and explicit history operands. State, loop and job depths
are all 256. The mapped LUT count falls by 31 (0.7%), with one additional
flip-flop. Both instruction regions fit the existing RAM blocks at this depth.

The combined comparison holds state/loop depth at 32, queue depth at 64, quantum
capacity at 16, sixteen scratch/history slots, one input and two outputs. Enabling
live mode adds 236 LUTs (3.5%) and 78 flip-flops (2.1%). The small program crosses
Yosys's distributed-RAM/BRAM mapping threshold: RAM64M cells fall from 118 to 86,
and three RAMB18 blocks appear for instruction storage. It does not duplicate
processor state or DSP history. This profile excludes the media/cache subsystem;
the media wrapper's fence arbitration is tested separately.

Source hashes, tool versions, parameters and cell counts are in
[live-loading-resources.json](measurements/live-loading-resources.json). Reproduce
the combined measurement with:

```sh
python3 fpga/job_processor/measure_loading_resources.py --output /tmp/loading-audio
python3 fpga/state_processor/measure_resources.py --program-regions 2 --output /tmp/loading-sp
```

These are cell-mapping estimates, not place-and-route area or timing. Preparation
has no fixed deadline, and the RTL intentionally waits for its fence before
starting the next quantum. Tests verify progress with a held upload request and
safe completion under stalls; they do not establish a board-level latency budget.

The SP advances the single global frame even when its sync section is empty.
Empty startup/retirement programs can therefore switch back into loop processing
without restarting time. The loader does not introduce another processing clock.

## Validation results

- Compiler/reference suites, including no-heap recording switches and six FIFO
  loading scenarios, pass.
- All sixteen selected RTL target, state, audio, smoothing, sync, reference and
  live-loading tests pass. The retained pipeline matrix also passes with instant,
  fixed-latency, DRAMsim3 and legacy memory.
- Protocol/transport: 36 tests pass. Native RT backend/engine: 294 pass; loop
  regressions: 224 pass. Host: 54 pass, with the previously reproduced detached
  launcher shutdown failure excluded and one device-dependent test ignored.
- Formatting, warning-denying affected-package builds, Rust test-attribute policy
  and the historical Marlin coverage guard pass.
- The complete workspace build is blocked by missing ALSA development files;
  `cargo-nextest` is unavailable. These environment limitations are independent
  of the loading changes.
