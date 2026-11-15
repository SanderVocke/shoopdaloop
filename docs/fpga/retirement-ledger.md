# Component retirement ledger

Implementation baseline: `247c4974` (`proto/fpga-rust-reference`, unchanged at
execution start). Future integration follows PR #997; this change does not merge it.

| Source / dependency | Disposition and verification destination |
| --- | --- |
| SP RTL, raw instructions and logical record input | Retain; `state` and `smoothing` suites use explicit vectors and delayed record slaves. No musical resolver in the fixture. |
| JP RTL and `audio_pipeline` | Retain; `audio` suite covers production arithmetic, streams and reset. |
| `media_audio_pipeline` and its AXI top | Retain: already TP-free SP -> JP -> media integration. `pipeline` suite covers exact samples, retained ranges, stalls, fault/reset and memory. No replacement coordinator is needed. |
| Media, metadata/cache, chunk/prefetch, AXI/DDR | Retain; media, ownership, deferred, chunk, metadata, resolver (media address resolver), direct and AXI suites. |
| Marlin bits/model/clock/sim/probes/trace/report/random | Retain; extract toolchain validation and local component packing from simulator imports; `core` checks wide ports, lifetimes, independent instances and trace cleanup. |
| Adapter images and simulator packing | Extract only instruction/state enums and their encoding into the component harness; old app images, receipts, scheduler and device lifecycle are removed. |
| TP RTL, ABI, transition fixtures/drivers | Delete; musical scheduling/arbitration/request tests intentionally retired. Production Rust scheduling is unaffected. |
| `loop_pipeline`, `loop_pipeline_axi` | Delete; retained execution behavior already has `media_audio_pipeline` coverage. TP faults, system silence and reset/reload of the complete backend are retired contracts. |
| Rust FPGA adapter, simulator and application host | Delete after extracting packing/toolchain helpers; remove backend launch/selection and optional features. Keep native host paths. |
| Production CPU testbench | Keep CPU execution, scenarios, audit and float/fixed checks; remove RTL compiler and transition oracle. Define its scenario identities locally. |
| Audio I/O and lane RAM | Retain independent audio I/O/CDC/ownership checks. Remove control packets, prepared boot/demo and TP-backed system wrappers. |
| ZedBoard | Preserve constraints, provisioning knowledge and historical evidence; remove executable old control/demo generation. |
| Synthesis | Keep standalone state/job/memory profiles and component checks. Remove TP/system tops and source collection dependencies. |
| Coverage inventory | Preserve all 75 original identities/hashes; explicitly label retired TP/host obligations, validate all retained executable destinations. |

Environment: Debian 13, initially no Rust, Verilator, CMake, Nix or Vivado.
System package helper and sudo are unavailable. A workspace-owned Rust toolchain
is installed. Debian Verilator 5.032 and CMake were extracted into the workspace;
Yosys comes from the pinned OSS CAD Suite. The original compiled SP, JP and Marlin
core baseline tests pass. Extracted SP/JP/core/smoothing and production CPU tests
pass with Rust warnings denied. DRAMsim3 builds successfully. Complete retained
simulation and native gates are in progress. No board or place/route validation
is implied by component simulation.
