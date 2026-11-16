# FPGA execution components

The active hardware consists of the state processor (SP), job processor (JP),
media store, metadata/sample caches, prefetch and AXI memory services. SP executes
explicit programs and emits jobs; JP performs sample work through media/memory.
`media_audio_pipeline` and its AXI wrapper connect these components without a
transition processor. This branch provides components, not an application backend.

The RTL transition processor, full-loop hardware coordinator, old ARM control interfaces,
application simulator devices and prepared board demos have been retired.
Future RT communication and the ARM/FPGA split follow the Rust work in
[PR #997](https://github.com/SanderVocke/shoopdaloop/pull/997); their replacement
hardware interface is outside this change.

SP's raw-state instructions and read-only logical-record port remain component
contracts. Fixtures provide explicit records, generation/validity bits and delayed
responses. They do not implement musical scheduling or logical clock progression.
These ports and their layouts are provisional inputs for future integration, not
an ARM ABI. Local configuration and memory ready/valid ports remain available.

Fixed-point arithmetic, persistent ramps, media ownership, bounded storage,
cache coherence, transfer backpressure, retirement and reset behavior remain
verified. Faults are exposed by the relevant component; there is no active
application-level sticky-silence/reset protocol or full-loop timing guarantee.

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
expose owned-loop configuration and event ports; media/AXI wrappers retain explicit
record mode. No separate TP RTL, former ARM interface or application backend is
restored. See the [resource optimization report](docs/fpga/sp-sync-resource-optimization.md)
for the measured loop-storage and execution costs.

See [verification commands](src/rust/rt_fpga_testbench/README.md), the
[retirement ledger](docs/fpga/retirement-ledger.md), and the
[implementation plan](PLAN_FPGA_COMPONENT_RETIREMENT.md). Old timing and board
reports describe their recorded revisions and do not certify the current scope.

The optional Rust `rt-interface` exposes a concrete `carrier::Endpoint` for
command groups, responses and snapshots, with a separate driver-owned `Processor`
executing a fixed mono graph. Legacy requests, bulk and resync are ignored;
processing pacing is independent of message delivery and polling. Native
CPU preparation uses the same channel factory. The reference endpoint accepts
loop gain, immediate/two-step mode plans and plan clearing; its SP owns ordered
pulse-plan execution. Production native comparisons validate responses and loop
snapshot records. Graph updates and full application backend selection remain
outside this first endpoint; the same concrete carrier endpoint can serve a
simulator driver when the RTL command/snapshot frontend is implemented.

Shared preparation now lives in `rt_fpga_compiler`: a fixed-fixture emitter creates
logical sync/audio operations, state slots, initial values and bindings.
`StateProgramTarget` separates that description from typed Rust programs and
packed RTL configuration images. Existing carrier and SP/JP test configurations
use these targets, including tests with different state addresses in Rust and RTL.
Graph-description input and runtime graph delivery remain future work.
