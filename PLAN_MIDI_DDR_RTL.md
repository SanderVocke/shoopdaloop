# Cache-less MIDI RTL

Goal: match the managed MIDI functionality of the merged Rust FPGA reference in
synthesizable RTL. Target `feat/backend-replacement`. This includes shared event
storage, history and grouped grabs, state tracking, incremental differences,
ordered restoration/cleanup, recording/playback, transfers and refusal recovery.
No cache, frontend enablement, physical-board deadline or simultaneous-128-grab
timing requirement is introduced. CI results and automated review are ignored
unless requested. Functional conformance is the acceptance criterion.

## Design

Large storage belongs in DDR: event blocks, ownership tables, view descriptors,
immutable snapshots, input/output state, checkpoint records and difference maps.
Only finite command registers, small queues, temporary working buffers and the
fixed per-loop descriptors belong on chip. The latter are at most 16 KiB and use
primary BRAM, avoiding repeated DDR access for bounded loop bookkeeping. A single
ordered 32-bit memory service uses acknowledged reads
and writes; it can attach to the existing word AXI endpoint. There is no assumption
about memory latency. Responses and proposed output remain stable under pressure.
Rust is an independent test oracle, never a runtime implementation for the RTL.

Keep the existing SP's timing authority. Changes to application graph/control
capabilities are outside this component implementation. Large copies, imports,
comparison initialization and reclamation expose bounded resumable work.

## Execution

- [x] Start from the merged backend branch; establish memory and command contracts.
- [x] Implement and compare DDR state interpretation, incremental differences and
  ordered restoration against the Rust components, including pedals and resets.
- [x] Implement shared event blocks, generation-checked views/snapshots, immutable
  ranges, staged import/export and bounded retirement; verify ownership failures.
- [x] Implement input history/checkpoints, grouped grabs and loop execution with
  output acceptance, conflict handling and coordinated destination recovery.
- [x] Exercise the complete RTL path against the Rust reference under delayed DDR,
  command/output pressure, capacity failures, transfers and multiple loop quanta.
- [x] Check synthesizability and on-chip storage; run relevant Rust/build checks,
  document measured coverage and remaining hardware integration limits.
- [ ] Commit meaningful stages, push, and open PR(s) against
  `feat/backend-replacement`. Do not merge autonomously.

Keep this plan current. Implementation steps may change with evidence; the goal
and functional acceptance must not be reduced without the user's agreement.

Evidence and integration boundaries: [validation](docs/fpga/midi-ddr-rtl.md).
The standalone runtime, DDR protocol and diagrams are in
[the MIDI guide](fpga/midi/README.md). Sixteen conformance regressions and the
124-test reference/compiler gate passed; AXI32/64 and full-core synthesis were
checked. Application capability enablement and board timing remain outside this
functional milestone. No CI/review feedback was inspected.
