# FPGA execution components

See the [RTL technical reference](docs/fpga/rtl-reference/README.md) for block
diagrams, functional chapters, and HTML and PDF build commands.

The active hardware consists of the state processor (SP), job processor (JP),
media store, metadata/sample caches, prefetch and AXI memory services. SP executes
explicit programs and emits jobs; JP dispatches audio and MIDI work through
their storage and data services.
`media_audio_pipeline` and its AXI wrapper connect these components without a
transition processor. A Rust functional reference implements the same execution
model. The Rust reference, Marlin and ARM hosts expose a restricted prepared-graph
application profile, separate from CPU-native/default-browser execution.

SP emits MIDI Input, Read, Capture, Merge and Output work into the ordinary
JP queue. A capture prepass queues writer transitions before the data jobs.
`midi_audio_pipeline_axi` connects the shared pipeline to a directly sequenced
MIDI data-job unit. It holds sequence handles, list descriptors and reader
cursors on chip. DDR holds recorded events, shared views and scratch lists.
Default raw mode plays event bytes. The optional expression-state extension
adds per-view starting snapshots, current input/output state, per-sequence/output
difference maps, and explicit shared-JP restore/cleanup jobs. Bulk state stays
in DDR; small binding/directory/count tables and working channels use on-chip
memory. Audio keeps its caches. See the [musical-state chapter](docs/fpga/rtl-reference/manual.md#midi-expression-state-and-restoration)
and [state ABI](docs/fpga/rtl-reference/opcodes.md#midi-expression-state-abi)
for the supported controllers, pedal/note behaviour, encodings and limits. The application's MIDI host
wiring and SoC embedding remain separate. See the [MIDI protocol](fpga/midi/PROTOCOL.md),
[implementation and validation](docs/fpga/midi-data-jobs.md), and
[64-loop measurements](docs/fpga/midi-data-jobs-benchmarks.md).

The retired RTL transition processor and full-loop coordinator are not restored.
Rust RT communication uses paired immediate command/response FIFOs and a separate
latest-complete snapshot publisher; it is not literally a snapshot-only FIFO.
Processing pacing has a separate driver/audio owner.

The production ARM/FPGA bridge currently uses legacy mailbox ABI 15, including
CPU control windows and staged state patches. Those are workarounds, not an
FPGA-owned semantic consumer. The replacement
[ABI-16 semantic FIFO contract](fpga/host_bridge/semantic_fifo.md) is frozen for
Stages 4–5 of [the authorized plan](FPGA_CMD_FIFO.md); implementation, integrated
resource/timing acceptance and delivery remain pending. It specifies four complete
32-command banks, reserved reliable results, one group per quantum, local event
effects and independently nonblocking coherent snapshots. No reader readiness or
partial ARM transaction may hold rendering.

SP's raw-state instructions and read-only logical-record port remain component
contracts. Isolated record-port fixtures provide explicit records,
generation/validity bits and delayed responses, without implementing musical
scheduling. Sync integration tests instead run the SP's compiled loop prefix.
These ports and their layouts are provisional inputs for future integration, not
an ARM ABI. Local configuration and memory ready/valid ports remain available.

Fixed-point arithmetic, persistent ramps, media ownership, bounded storage,
cache coherence, transfer backpressure, retirement and reset behavior remain
verified at their recorded scopes. The board bridge provides fault-safe semantic
receipts, retained first-fault diagnostics and always-on runtime recovery, alongside
watchdog/thermal protection. These are not whole-loop timing, physical-output or
thermal-trip qualification. The replacement frontend still needs fresh integrated
cycle/resource and protected-route evidence. The Rust reference separately tests
its latched pipeline fault and silence behavior.

The [Rust reference](src/rust/rt_fpga_reference/README.md) provides functional
state, job and logical-memory models, plus a combined functional
pipeline. Transition traces are compared to the current native loop resolver;
Marlin differential tests compare the retained SP/JP RTL's jobs, state and samples
exactly for the shared audio instruction subset. The retired Rust TP and its request/completion machinery are not restored.
Raw component fixtures may write the two plan slots directly. The reference
endpoint resolves semantic controls against execution-owned state; legacy ARM
control helpers still perform live read/patch work until Stages 4–5 remove them.
The measured isolated sequential edit kernel is not yet the production frontend.
SP retains pulse/countdown/scheduled execution and per-quantum slot outcomes.
The SP sync-ordered prefix processes
wraps, pulses, transitions and next cursors, saving render state for the audio pass.
The same prefix is implemented in the existing RTL `sp_engine` FSM and datapath;
there is no separate sync execution controller. Standalone SP and `audio_pipeline`
expose owned-loop configuration and event ports; media/AXI wrappers also retain
explicit record fixtures. No separate TP RTL or former ARM interface is restored. See the
[resource optimization report](docs/fpga/sp-sync-resource-optimization.md)
for the measured loop-storage and execution costs.

Composite audio programs extend the sync prefix with bounded schedule rows in
program RAM. The SP settles nested and shared children at the quantum boundary,
and their media uses the existing playback and capture jobs. See the
[composite audio reference](docs/fpga/rtl-reference/manual.md#composite-audio-loops)
for supported transport and recording. Initial static composites and their
ordinary controls remain supported. Live replacement involving either an active
or candidate composite refuses, even while stopped; primitive-only replacement
retains continuity and real ownership fencing. Composite replacement requires an
explicit quiesced/new-incarnation lifecycle, not preview, cut search or automatic
stop/retry. CPU-native/default-browser editing remains unchanged; optional worklet
`fpga-reference` shares the reduced FPGA profile.

See [verification commands](src/rust/rt_fpga_testbench/README.md), the
[retirement ledger](docs/fpga/retirement-ledger.md), and the
[implementation plan](PLAN_FPGA_COMPONENT_RETIREMENT.md). Old timing and board
reports describe their recorded revisions and do not certify the current scope.

The optional Rust `rt-interface` exposes a concrete `carrier::Endpoint` for
command groups, responses and snapshots, with a separate driver-owned `Processor`
executing a compiled mono graph. Legacy requests, bulk and resync are ignored;
processing pacing is independent of message delivery and polling. Native
CPU preparation uses the same channel factory. The reference endpoint accepts
loop gain, immediate/two-step mode plans and plan clearing; its SP owns ordered
pulse-plan execution. Production native comparisons validate responses and loop
snapshot records. The optional `shoop_host` profile accepts prepared graph edits
through `RtBackend` and supports explicit activation and live replacement.
Its application client covers recording, playback and graph changes. The shared prepared profile admits up to 128 loop slots subject to actual
program/state/job/buffer resources, one physical input and two outputs. Static
composites are supported as described above; application MIDI, hosted processors
and media import/export remain outside it. Native execution stays the default.
See [host usage and limits](src/rust/shoop_host/README.md).

Shared preparation lives in `rt_fpga_compiler`: its restricted graph frontend and
fixed fixtures create logical sync/audio operations, state slots, initial values
and bindings.
`StateProgramTarget` separates that description from typed Rust programs and
packed RTL configuration images. Existing carrier and SP/JP test configurations
use these targets, including tests with different state addresses in Rust and RTL.

The ARM-side allocator binds continuing instances to persistent addresses and
reserves unused slots for new instances. Only fresh state receives initialization
writes. Programs are staged in an inactive instruction region and selected by an
ordered `SwitchProgram` at a quantum boundary. Execution returns ownership only
after earlier memory work is fenced. Rust and RTL share these lifetime rules;
their instruction encodings remain independent. The existing adapters retain legacy loader/control lanes during migration.
ABI 16 orders small switches with semantic commands, with the switch last, without
passing large uploads through that queue. Its settled snapshot gate requires a
complete sync prefix before audio work; standalone component interleaving is not
that profile. Selection, SP idle and a new-revision snapshot never substitute for
the JP/cache/DMA ownership fence. See [the loading contract](LOADING_DESIGN.md),
[implementation and validation](docs/fpga/live-loading.md), and the ABI contract.

The [native CPU comparison](docs/fpga/native-performance-comparison.md) records
the measured effect of this branch on ordinary native execution. It is separate
from RTL timing and resource measurements.
