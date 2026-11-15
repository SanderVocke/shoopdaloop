# FPGA execution components

The active hardware consists of the state processor (SP), job processor (JP),
media store, metadata/sample caches, prefetch and AXI memory services. SP executes
explicit programs and emits jobs; JP performs sample work through media/memory.
`media_audio_pipeline` and its AXI wrapper connect these components without a
transition processor. This branch provides components, not an application backend.

The transition processor, full-loop coordinator, old ARM control interfaces,
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

See [verification commands](src/rust/rt_fpga_testbench/README.md), the
[retirement ledger](docs/fpga/retirement-ledger.md), and the
[implementation plan](PLAN_FPGA_COMPONENT_RETIREMENT.md). Old timing and board
reports describe their recorded revisions and do not certify the current scope.
