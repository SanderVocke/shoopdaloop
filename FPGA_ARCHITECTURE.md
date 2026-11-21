# FPGA execution components

See the [RTL technical reference](docs/fpga/rtl-reference/README.md) for block
diagrams, functional chapters, and HTML and PDF build commands.

The active hardware consists of the state processor (SP), job processor (JP),
media store, metadata/sample caches, prefetch and AXI memory services. SP executes
explicit programs and emits jobs; JP dispatches audio and MIDI work through
their storage and data services.
`media_audio_pipeline` and its AXI wrapper connect these components without a
transition processor. A Rust functional reference implements the same execution
model and has an optional, restricted application host profile. Marlin exercises
the RTL components and live program loader; it has no complete application host.

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

The RTL transition processor, full-loop hardware coordinator, old ARM control interfaces,
application simulator devices and prepared board demos have been retired.
RT communication follows the immediate command/response queues and complete-state
snapshots shared with the native backend. Processing pacing has a separate owner.
The remaining hardware command/snapshot frontend is future work.

SP's raw-state instructions and read-only logical-record port remain component
contracts. Isolated record-port fixtures provide explicit records,
generation/validity bits and delayed responses, without implementing musical
scheduling. Sync integration tests instead run the SP's compiled loop prefix.
These ports and their layouts are provisional inputs for future integration, not
an ARM ABI. Local configuration and memory ready/valid ports remain available.

Fixed-point arithmetic, persistent ramps, media ownership, bounded storage,
cache coherence, transfer backpressure, retirement and reset behavior remain
verified. Hardware faults are exposed by the relevant component; there is no
complete board-level fault/reload protocol or full-loop timing guarantee. The
Rust reference separately tests its latched pipeline fault and silence behavior.

The [Rust reference](src/rust/rt_fpga_reference/README.md) provides functional
state, job and logical-memory models, plus a combined functional
pipeline. Transition traces are compared to the current native loop resolver;
Marlin differential tests compare the retained SP/JP RTL's jobs, state and samples
exactly for the shared audio instruction subset. The remaining Rust TP, command
admission and retained request/completion machinery are removed. Callers write
the two prepared plan slots directly; SP exposes per-quantum slot outcomes.
The SP sync-ordered prefix processes
wraps, pulses, transitions and next cursors, saving render state for the audio pass.
The same prefix is implemented in the existing RTL `sp_engine` FSM and datapath;
there is no separate sync execution controller. Standalone SP and `audio_pipeline`
expose owned-loop configuration and event ports; media/AXI wrappers also retain
explicit record fixtures. No separate TP RTL or former ARM interface is restored. See the
[resource optimization report](docs/fpga/sp-sync-resource-optimization.md)
for the measured loop-storage and execution costs.

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
Its application client covers recording, playback and graph changes. The profile
supports up to sixteen mono primitive loops, with restricted routing and no MIDI,
processors, media import/export or general UI integration. Native execution stays
the default. See [host usage and limits](src/rust/shoop_host/README.md).

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
their instruction encodings remain independent. The Marlin loader bridge supports
singleton switch groups, while ordinary runtime commands and snapshots still
need a hardware frontend. See [the loading contract](LOADING_DESIGN.md) and
[implementation and validation](docs/fpga/live-loading.md).

The [native CPU comparison](docs/fpga/native-performance-comparison.md) records
the measured effect of this branch on ordinary native execution. It is separate
from RTL timing and resource measurements.
