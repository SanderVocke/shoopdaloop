# Retire FPGA transition processing and the current ARM interface

## Goal and branch

Reduce the FPGA implementation to reusable state processing, job processing,
media/memory/cache subsystems and their verification infrastructure. Remove the
transition processor and the current ARM/application-facing integration. Preserve
the reusable Marlin work so a subsequent iteration can build on these components.

`feat/rt-state-snapshots` defines the direction for future RT communication and
the ARM/FPGA split. This cleanup does not implement that split or port its protocol
into hardware. It deliberately leaves no complete application FPGA backend.

The requested base was called `feat/fpga-rust-reference`; upstream has only
`proto/fpga-rust-reference`. Use that verified upstream branch, currently
`247c4974c56464d0d955bd384d3c45e322812812`. The planning branch
`feat/fpga-retain-execution-components` has been created from it. Perform the
implementation on this branch and finish with a PR **into
`proto/fpga-rust-reference`**, not master or `feat/rt-state-snapshots`.

Implementation is in progress on the branch above. The retirement ledger records
file dispositions; validation results are recorded as gates finish.

## Investigated baseline

- `fpga/transition_processor` owns loop timing, synchronization, arbitration and
  scheduled requests. `fpga/loop_pipeline` integrates it with state/job execution.
- `rt_fpga_adapter` contains queue consumers, graph lowering, packed images,
  loop/media handles and an application engine bridge. Some image/packing work
  is useful independently; the adapter's execution contract is being retired.
- `rt_fpga_sim` contains Marlin-backed pipeline devices, demos and application
  fixtures. `rt_backend_testbench` combines production CPU execution with RTL
  image lowering and transition fixtures and depends on `rt_fpga_adapter`.
- `rt_fpga_testbench` already provides useful standalone SP, JP, media, metadata,
  cache and AXI checks. However, its `sim` feature pulls in all three crates above;
  `codec.rs` imports packing through the adapter and simulator. Deleting those
  crates first would break component verification.
- SP supports raw-state instructions and a read-only settled-loop record port.
  The latter can be exercised by a delayed fixture slave without running a TP.
- The Marlin suite's `model.rs`, `bits.rs`, `clock.rs`, `sim.rs`, memory agents,
  DRAMsim3 service, probes and reporting are substantially reusable. Some helpers
  also contain integrated-pipeline assumptions and need separation.
- `fpga/synthesis/subsystems.py` has separate state/job/memory profiles, but copies
  TP and loop-pipeline sources too. Synthesis and simulation source lists must be
  audited even when their top module is retained.
- Open PRs #990 and #991 target the FPGA base; #993/#994 build on related branches.
  Their interface work is not part of this cleanup and must not be merged into it.
  PR #997 is the Rust communication work used as architectural context.

## Scope and disposition

| Area | Required disposition |
| --- | --- |
| `fpga/state_processor`, `fpga/job_processor` | Preserve execution RTL, numeric behavior, RAM/FIFO machinery and independent tests. Remove only TP-specific coupling that cannot stand independently. |
| `fpga/media_store`, `fpga/chunk_memory`, `fpga/axi_memory` | Preserve media allocation/ownership, metadata and sample caches, prefetch, coherence, AXI arbitration/transfer, retirement and DRAMsim3 support. |
| `fpga/transition_processor`, transition ABI helpers | Delete executable TP implementation, wrappers, drivers, fixtures, scheduling/comparison tests and live launchers. |
| `fpga/loop_pipeline` and integrated AXI tops | Retire TP-containing coordinators. Extract reusable SP/JP/memory wiring before deletion; do not discard shared memory endpoints because they live beside an obsolete top. |
| `rt_fpga_adapter`, `rt_fpga_sim` | Retire old owning-queue devices, receipt/cancellation machinery, application engine bridges, transport packing and demos. Extract needed component instruction/state/job types and codecs first; delete crates if no independent purpose remains. |
| `rt_backend_testbench`, `scripts/rt_backend_testbench` | Remove TP/ARM-specific RTL lowering and comparisons. Preserve independent production CPU/numeric checks, scenario support and CPU benchmarks where still useful; remove adapter dependencies. Do not keep a crate solely to retain a dead interface. |
| `rt_fpga_testbench`, `third_party/marlin-verilator` | Preserve reusable native simulation infrastructure and retained component suites. Remove obsolete full-loop/ARM protocol coverage and dependencies. Preserve the local Marlin patch and regression evidence for its used capabilities. |
| Board/control/audio I/O | Remove current ARM control/register/packet interfaces and TP-backed board/demo generation. Preserve independently useful audio FIFOs, clock-domain crossing, reset, constraints, tooling and board knowledge. |
| CI, synthesis, docs, reports | Rewire active entry points to retained components. Keep historical measurements/provenance clearly labeled as historical; do not claim old integrated timing applies to the new scope. |

Inspect mixed directories file by file. A directory name alone is not sufficient
evidence for deletion. Git history is the recovery mechanism for retired source;
do not leave disabled copies, compatibility shims or a renamed TP behind.

## Design rules and constraints

1. Do not change production Rust loop behavior, the native backend or the
   `feat/rt-state-snapshots` design. Do not remove the production resolver/planner
   or independent legacy application fixtures merely because they mention
   transitions. Hardware TP retirement has a narrower scope.
2. Do not implement a replacement command/result FIFO, register map, ARM service,
   transition scheduler or hardware ABI. No migration of TP semantics into SP,
   JP, the test harness or a new software controller.
3. Preserve component-local configuration, ready/valid, memory and observation
   ports needed to exercise retained hardware. These are component contracts,
   not a commitment to a future host interface. Separate their packing from the
   obsolete ARM protocol.
4. Keep SP raw-state behavior and reusable read-only logical-record consumption
   when it can be tested without TP. A fixture supplies explicit records and
   handshake delays; it does not calculate musical transitions or advance loop
   clocks. Remove TP-only record fields/adapters only after proving they have no
   independent consumer. Document retained provisional fields explicitly.
5. Preserve fixed-point formats, arithmetic ordering, ramp semantics, opcode
   execution, media ownership, cache coherence, error propagation and reset
   behavior of retained blocks. No unrelated optimization, opcode redesign,
   broad rename or formatting cleanup.
6. Extract small, directly used codecs/helpers into the existing component
   testbench or a justified small shared module. Do not copy the old app/ARM
   interface wholesale under another name. Numeric expectations must remain
   independently checked, rather than generated by the codec being tested.
7. Report removed coverage honestly. Update the coverage inventory/checker to
   distinguish intentionally retired functionality from retained obligations;
   never pass by suppressing all missing rows or removing the check entirely.
8. Preserve historical artifacts without treating their generated source paths
   or measurements as active build requirements. Do not rewrite measured results.

## Immutable acceptance criteria

- No active RTL top, Rust/Python executable, build target or CI job instantiates,
  imports or launches the retired TP or the old ARM/application FPGA interface.
  There is no hidden scheduler or replacement TP in retained fixtures.
- SP, JP, media storage, metadata/sample caches, prefetch, AXI memory services and
  their necessary dependencies remain usable and verifiable independently.
- The retained Marlin suite builds and runs from a clean checkout without
  `rt_fpga_adapter`, old simulator devices, TP images, old packet/register
  contracts or cached generated files. Optional simulation stays optional for
  normal native and Wasm builds.
- Independent SP/JP tests and at least one SP -> JP -> memory execution smoke
  pass using explicit instructions/state/jobs, with exact output and persistent
  state/media checks. This smoke contains no musical scheduling or ARM emulation.
- Existing retained arithmetic, ownership, coherency, backpressure and reset
  obligations continue to pass, including selected delayed AXI/DRAMsim3 cases.
  Tests formerly available only through a retired top are relocated where they
  protect retained behavior; TP-specific obligations are explicitly retired.
- Production CPU behavior and unrelated legacy fixtures remain intact; normal
  build/test entry points do not require a hardware backend or Marlin.
- Active documentation accurately describes component-only FPGA status, working
  commands, retained interfaces and deferred integration. Historical reports are
  identified as such; no current full-loop, board or 150 MHz system claim remains.
- The implementation is committed and pushed on the new branch, with a PR into
  the FPGA base. Address CI failures caused by these changes; unrelated failures
  do not block completion. Do not wait for review or merge the PR automatically.

## Staged implementation

### Stage 1 — Freeze the inventory and retained baseline

Depends on: the planning branch setup above.

- [x] Fetch the current base and inspect changes since `247c4974`; record the
  actual implementation baseline. Preserve unrelated work and avoid importing
  the open ARM-interface PRs. Resolve base movement explicitly if it overlaps.
- [x] Record a file/module disposition ledger for mixed areas: SP logical-record
  ports, AXI tops, Marlin codecs/design builders, production testbench lowerers,
  board tooling and audio I/O. Include inbound dependencies and retained tests.
- [x] Read applicable nested instructions and the build/test guidance. Check
  available Rust, Marlin/Verilator, Python, DRAMsim3 and synthesis tools using the
  repository's environment guidance; record baseline failures separately.
- [x] Run the original compiled standalone SP/JP and Marlin core baseline targets;
  use the retained suite after extraction for unchanged media/cache/AXI coverage. Save seeds/configurations and baseline results outside
  tracked generated-source paths; avoid full musical/64-loop campaigns.
- [x] Identify retained behavior currently tested only through a TP-backed top;
  assign a component-level destination or explicitly classify it as retired.

Verification: review the ledger against Cargo feature/dependency graphs, RTL
source manifests, launcher imports and CI commands. Every preserved subsystem
must have an executable verification destination before deletion starts.

### Stage 2 — Make reusable Marlin support independent

Depends on: stage 1 inventory and baseline.

- [x] Separate retained bits/ports, clocks, edge ordering, model cache/build,
  reset, multi-instance lifetime, tracing/probes, watchdogs, seeds and reports
  from pipeline/device assumptions. Retain used `third_party/marlin-verilator`
  fixes and focused regressions.
- [x] Extract instruction/state/job packing used by SP/JP and component drivers
  from `rt_fpga_adapter`/`rt_fpga_sim`; remove their imports from retained codecs.
  Preserve width checks, signed values and independently specified bit vectors.
- [x] Keep AXI32/64 agents, native SV backing memory, fixed/stalled memory modes
  and the DRAMsim3 wrapper/service available without a loop-pipeline device.
- [x] Separate generic image/memory helpers from TP topology, action/request
  images, app scenarios and hardcoded integrated hierarchy probes.
- [x] Rewire `rt_fpga_testbench` features/dependencies so component-only tests
  compile without the soon-to-be-retired crates. Keep this stage buildable while
  obsolete integration paths still exist.

Verification: run Marlin core lifetime/port/trace checks, SP instruction/FIFO and
JP arithmetic checks, plus one AXI memory test; inspect `cargo tree` for retained
targets to confirm the old adapter/simulator is no longer a dependency.

### Stage 3 — Preserve execution components without transition hardware

Depends on: stage 2 independent harness.

- [x] Remove SP compile-time includes/type dependencies on TP, if present;
  preserve useful raw-state and read-only record interfaces with local definitions.
  Exercise the latter with static records, delayed responses and invalid records.
- [x] Extract reusable SP/JP/media/cache wiring from `loop_pipeline`, AXI tops
  and simulator wrappers into a minimal test-only execution harness. The harness
  supplies instructions, state, buffers and explicit run boundaries; it has no
  request tickets, loop scheduler, ARM packets or replacement application ABI.
- [x] Retain JP/media BeginCapture and other useful operations as execution
  primitives; test them directly rather than deriving work from transitions.
- [x] Relocate retained integrated tests for sample flow, capture/readback,
  prefetch, retirement, memory faults and reset to the appropriate component or
  execution harness. Keep production math or independent formula oracles.
- [x] Run the preserved small acceptance smoke: SP emits ordered jobs, JP executes them
  against retained memory/cache, and exact samples plus persistent state/media
  match explicit expectations across several runs, stalls and reset/reload.

Verification: run focused state/audio/media/chunk/metadata/ownership/AXI suites
and the new smoke. Check the harness source list contains no TP or old host
endpoint; demonstrate no scheduler is needed to reproduce its expected output.

### Stage 4 — Delete TP and the retired integration contracts

Depends on: stage 3 replacement destinations passing.

- [x] Delete TP RTL, transition ABI packers/generators, TP-specific drivers,
  request fixtures/comparators, topology images and associated launchers.
- [x] Delete TP-containing coordinators and obsolete pipeline tops after their
  reusable wiring has been extracted. Remove obsolete source-list entries.
- [x] Remove old ARM/application adapters, owning-queue devices, request/receipt
  cancellation and media-handle bridges, simulator devices and full-loop demos.
  Delete empty `rt_fpga_adapter`/`rt_fpga_sim` crates and update Cargo.lock.
- [x] Separate useful production CPU/numeric scenario and benchmark support in
  `rt_backend_testbench` from obsolete RTL/transition lowering; remove remaining
  imports of retired crates without altering production DSP/loop code.
- [x] Remove old control/register/packet endpoints, generated firmware/demo
  images, board runners and TP-dependent system audio wrappers. Keep independent
  audio I/O, CDC/reset/constraints and board provisioning knowledge where useful.
- [x] Remove dead CLI subcommands, tests, fixtures and Python aliases. No
  successful no-op replacements, disabled executable copies or stale demos.

Verification: run dependency/reference searches with a reviewed historical-doc
allowlist, Cargo metadata and retained testbench build/tests. Verify old commands
are absent or give a clear unsupported-command error, never apparent success.

### Stage 5 — Reconcile synthesis, CI, coverage and documentation

Depends on: stage 4 final retained surface.

- [x] Update `fpga/synthesis` source collection and Tcl tops for standalone
  state/job/memory builds. Remove live integrated TP/board synthesis targets and
  accidental dependencies on archived source manifests.
- [x] Update `.github/workflows/fpga_simulation.yml` and any affected build/Zynq
  workflows/path filters. Keep component simulation and default-build isolation
  gates; replace full-loop/ARM demos and 64-loop campaigns with retained suites
  and the execution smoke. Preserve independent audio/CPU numeric gates.
- [x] Update `coverage.json` and `scripts/check_fpga_testbench_coverage.py` with
  explicit retained, moved and retired obligations and real executable targets.
- [x] Update `FPGA_ARCHITECTURE.md`, `SIMPLIFY.md`, active plans/READMEs, component
  docs, verification contract/results and launcher help. Clearly supersede old
  architectural plans without implying their former requirements still apply.
- [x] Keep historical timing/board reports and hashes with a retirement notice
  and revision provenance. Document deferred communication/ARM split work and
  link PR #997 as context; do not specify a replacement hardware protocol.

Verification: validate workflow syntax, coverage mapping and Python checks;
elaborate/lint standalone state/job/memory tops using available tooling. Exercise
documented commands from clean generated-output directories. Run synthesis if
available; distinguish elaboration checks from new place/route measurements.

### Stage 6 — Final end-to-end validation of the retained scope

Depends on: stages 1–5 complete.

- [ ] Use a fresh worktree/output/cache directory to build and run all retained
  Marlin component tests and the SP -> JP -> memory smoke, not cached old tops.
  Run instant/fixed/stalled AXI32/64 cases and selected DRAMsim3 cases covering
  delayed transfers, dirty forwarding, retirement/reuse and coordinated reset.
- [ ] Check exact signed arithmetic and ramps, FIFO pressure, malformed operands,
  partial failure, media ownership/reference counts and memory fault propagation.
  Use the retained inventory rather than rerunning retired musical workloads.
- [ ] Run `cargo fmt --all -- --check`, `python3 scripts/check_shoop_test_usage.py`
  when Rust tests changed, and `RUSTFLAGS="-D warnings" cargo build --workspace`.
  Run `cargo fmt --all` before Rust commits and inspect for unrelated formatting.
- [ ] Run the repository's complete native Rust gate before the final behavior
  push: `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features
  shoop_engine/app_backend,shoop_backend/native-fx --profile ci`. Use the required
  toolchain/environment. Rely on CI for other platforms unless failures require
  targeted reproduction; check native/Wasm dependency isolation there.
- [ ] Confirm removal via source/dependency/source-manifest audits and review
  every ledger row. Record tool versions, seeds, commands, results, retired
  coverage and any unavailable gates without describing missing checks as passes.

Verification: all immutable criteria have concrete evidence. This is a component
execution end-to-end gate, not a complete loop/backend/board acceptance claim.

### Stage 7 — Push, PR and change-related CI follow-up

Depends on: stage 6 evidence and a clean intended diff.

- [ ] Commit each completed stage or meaningful milestone and update this plan
  throughout implementation. Push `feat/fpga-retain-execution-components`.
- [ ] Create a PR into `proto/fpga-rust-reference`. Explain what was removed,
  what remains reusable, the intentional loss of the complete FPGA backend,
  the relation to #997 and the validation results. Use a draft while incomplete.
- [ ] Inspect CI results and artifacts; investigate failures to determine whether
  these changes caused them. Fix change-related failures and rerun affected
  checks. Report unrelated/pre-existing or infrastructure failures without
  treating them as completion blockers; do not require globally green CI.
- [ ] Do not wait for human or automated review, poll for approval, or require
  review completion before finishing this task.
- [ ] Recheck base movement/conflicts, final diff and change-related CI results
  after the last push. Mark the PR ready when complete and report its URL and
  any unrelated CI failures. Leave merge to the user; do not merge autonomously
  or modify other branches/PRs.

## Testing cost and execution contract

During extraction, use component/package targets and fixed seeds to catch broken
contracts quickly; do not run broad application matrices or expensive route/scale
campaigns at each step. After deletion and wiring settle, run one complete retained
simulation pass, selected realistic-memory cases and the native regression gate.
CI covers platform/build isolation; only failures caused by these changes require
fixes before completion. Unrelated failures do not require globally green CI.
Do not wait for review. New code or CI fixes justify rerunning affected tests;
repeat broad gates only when their conclusions could have changed.

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Do not implement the cleanup as part of the planning task.
