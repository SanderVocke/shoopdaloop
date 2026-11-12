# Reference retirement coverage ledger

Baseline: `e5c015be` on `proto/fpga-rust-reference`. The active plan is
`PLAN_UNIFY_BACKENDS.md`. Entries below are migration destinations, not completed
validation claims. Preserve each behavior before removing its previous evidence.

| Existing source/tool | Destination / disposition | Coverage required before removal |
| --- | --- | --- |
| `rt_fpga_reference/src/fixed.rs` | Small `rt_audio_math` numeric policy/specification | Raw gain/mix/ramp vectors, independent widened formula, float semantics |
| Reference compiler/state/jobs/pipeline | Delete execution models; production-plan export and RTL lowering in new runner | Full TP/SP/JP samples/state/media and arithmetic-order tests |
| Reference transition plan/engine/index/signal/types | Delete; production `shoop_loops` compiler/resolver/planner | Native-directed and randomized transition comparisons, scheduling, generations, outcomes |
| Reference storage/chunk_memory | Delete parallel store | Production media exports plus RTL range/ownership/cache invariants |
| `rt_fpga_testbench`, `fpga-reference` binary | Replace with `rt_backend_testbench` production driver/schema/image export | Batch run, schema validation, strict JSON, replay and negative contracts |
| `scripts/fpga_testbench`, generated schema/examples | Replace with `scripts/rt_backend_testbench` common spec/helpers | Rust/Python roundtrip/rejection tests; semantic cases replacing hardware-shaped inputs |
| `scripts/tests/test_fpga_testbench.py` | New common-contract tests | Integer bounds, malformed/duplicate/nonfinite fields, versioning, mismatch reporting |
| `fpga/transition_processor` comparison code | Direct production resolver adapter, retain driver/packing | Scheduling, sync cycles, request retention, standalone process and faults; no removed preview protocol |
| `fpga/state_processor` comparison code | Supported instruction vectors and production-derived full-pipeline coverage | Opcode/state bounds, FIFO pressure, persistent ramps, shared logical reads, reset |
| `fpga/job_processor` comparison code | Shared production arithmetic vectors and local stream invariants | All sample bits, one-/two-cycle streams, ramp extremes, readiness, resets and faults |
| `fpga/media_store` and resolver tests | Logical production media exports and local ownership/protocol checks | Slices, capture/retakes, read/write/read, shared data lifetime, malformed descriptors, exhaustion |
| `fpga/chunk_memory` tests | Retain functional memory/protocol/coherence checks without full reference store | Dirty forwarding, prefetch supersession, pinned reads, retirement, reuse, fences, errors/reset |
| `fpga/axi_memory`, DRAMsim3 wrapper/service | Retain; update common runner imports only as needed | Word/burst and AXI32/64, backpressure, visibility, reset, timing/utilization |
| `fpga/loop_pipeline` cases/runner/profiling | New behavioral scenarios + existing real RTL driver | All common matrix rows, automatic BeginCapture, sticky faults, TP-inclusive 64-loop timing |
| `rt_runtime::AudioRenderMode::FpgaReference` and mode parser | Remove; Current with compile-time numeric type replaces oracle | Production float/fixed tests and deliberate legacy-mode rejection |
| `rt_runtime/src/fpga.rs`, `executor_fpga.rs`, `process_fpga_state`/`finish_fpga` hooks | Delete executor/hooks; export only necessary compiler data through normal APIs | No alternate per-quantum Rust job executor; production plan ordering verified |
| `rt_runtime/resource/elastic_audio*` and reference branches in capture/group/resources | Remove reference-only integration | Production capture/storage/pool regressions remain; fixed media never float-round-trips |
| Engine endpoint/queue/jobs/description/lib special cases; loop planner/processor hooks | Remove reference-only branches; generic supported production DSP | Float engine controls, loops, resources, structures and existing render variants |
| Native host/worklet/host settings, mode lists and associated tests | Remove retired selector without changing normal float defaults | Native build, mode parsing/config tests, Wasm build where available |
| `scripts/benchmark_audio_chunks.py`, host chunk benchmark | Retain general benchmark; remove retired mode | Matched float baselines; new fixed production/RTL comparisons |
| `benchmark_elastic_cpu.py`, `elastic_cpu_benchmark` and reference-only reports | Remove obsolete executable/comparison; retain historical evidence labeled | Replacement production CPU/heap measurements with stated workload and units |
| `FPGA_ARCHITECTURE.md`, active runtime/FPGA READMEs | Update architecture/oracle/numeric/profile/run instructions | Clean-checkout commands work without old binaries or schemas |
| Historical chunk/reference results | Label historical, preserve provenance | No active instructions imply retired tools are available |
| `shoop_app_reference`, legacy engine/worklet and parity fixtures | Retain; unrelated to this retirement | Existing feature-parity protections remain unchanged |

Completion requires no executable dependency on the retired crates, no alternate
implementation hidden behind a renamed mode, and no old binary/schema import
needed by a retained runner. Small numeric, packing and serialization helpers
remain only where used and independently verified.
