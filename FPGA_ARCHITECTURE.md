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
transition, state, job and logical-memory models, plus a combined functional
pipeline. Transition traces are compared to the current native loop resolver;
Marlin differential tests compare the retained SP/JP RTL's jobs, state and samples
exactly for the shared audio instruction subset. In Rust, TP admits up to two
active plans per loop into SP-owned state. A new SP sync-ordered prefix processes
wraps, pulses, transitions and next cursors, saving render state for the audio pass.
This prefix is not implemented in RTL. No transition RTL, former ARM interface or
application backend is restored.

See [verification commands](src/rust/rt_fpga_testbench/README.md), the
[retirement ledger](docs/fpga/retirement-ledger.md), and the
[implementation plan](PLAN_FPGA_COMPONENT_RETIREMENT.md). Old timing and board
reports describe their recorded revisions and do not certify the current scope.
