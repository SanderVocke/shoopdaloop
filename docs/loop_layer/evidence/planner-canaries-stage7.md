# Stage 7 planner lifecycle controls and deliberate failures

Local source base: `c4ef8f9045a54fd08838677052db9ebc21ca6e9d`, with this milestone
implementation dirty. The containing commit supplies the intended fixture/runner,
maintained guides and evidence. Unrelated goal-state/book work is excluded. No
production library, dependency, manifest or lockfile changed. This is not final-head
CI, Miri, benchmark, hard-deadline or full-plan completion evidence.

## Real planner lifecycle, not a production injection seam

`BrowserPlannerCanary` prepares a test-only generic Planner around the actual loop
library Resolver. It does not modify the production LoopPlanner or replace the
fourteen full-facade contexts. A startup-local Play0, one-frame primitive loop and
decision-bound stereo half-gain consumer exercise the runtime planner/view/finish
lifecycle. The consumer checks exact planner/slot, frame, mode, position and cycles.
There are no external command/owning credits or media leases in this small profile.
All graph, view, observer, buffer and resource storage is prepared off-render.

Native `--planner-json` and
`planner_canary::tests::resolver_planner_control_is_partition_invariant_with_complete_boundary_tables`
compare whole/single/irregular/zero partitions against independent closed-form data:

- 128 stereo samples: left1..128, right×10; zero calls do not invoke planning/finish.
- 512 LSJ1 records/81960 bytes:128 settled boundaries,128 decisions,256 events.
- Play/Pulse at0, Wrap/Pulse at1–127, one-frame position0/cycles equal absolute frame.
- 128 plan/finish calls; no invented wrap/settlement at128 during stop.
- Full8page/4version/4snapshot recovery after explicit stopped destruction.

Planner and consumer destructors assert they run outside guarded rendering. Diagnostic
counters saturate through single-writer loads/stores, not retry loops. Source frame
arithmetic is checked. Production unsafe code was not introduced; the existing dev-only
allocator shim remains the deliberately instrumented boundary.

Shared Node/Chromium binding checks execute the complete native control report and
both fault modes. Actual offline/live successful controls additionally check every256
probe sample/channel (128 active then128 silent), every512 offline sample/channel,
all semantic bytes, acknowledged output origin, separate ordinary memories/static
markers and explicit recovery. Main marker0x51a17/worklet0 come from actual separate
instance-local state, not a constant marker implementation.

| Profile | Fixed worklet memory | Offline/live browser start |
| --- | ---: | --- |
| Debug | 1376256 bytes | 128 /0 |
| Release | 1310720 bytes | 128 /0 |

All guarded successful allocation/reallocation/deallocation counts and journal loss
are zero. These are fixture components, not total native RSS or JS/GC guarantees.

## Actual failure witnesses

`planner_canary::tests::planner_errors_and_heap_faults_have_exact_lifecycle_origins_and_are_not_successful_cleanup`
and actual unsuccessful worklet sessions verify:

| Kind | Render code | Rust planner witness | Processor witness | Plan/finish calls | Heap alloc/realloc/dealloc |
| --- | ---: | ---: | ---: | --- | --- |
| `planner` | 10 | 3 | 0 | 1/0 | 0/0/0 |
| `planner-rust-heap` | 4294901761 | 4 | 0 | 128/128 | 1/5/1 |

Both inject **inside Planner::plan after actual resolver preview**. Error mode returns
InvalidState before commit/consumer execution; output stays zero and no finish is
fabricated. Heap mode grows/drops a Vec, then completes the128 real intervals; the
Rust allocator guard rejects the contract violation. Repeated render after either
fault is refused. Native explicit abandonment verifies resource storage recovery;
it does not count as the separate successful control's cleanup.

The runtime callback cursor is128/callback count1 in both failures: the existing
RenderRuntime advances the cursor before planner execution. It is not a count of
rendered/committed intervals. The failed first plan still has zero finishes. No
rollback, frame replay or synthetic finish was added.

The supervisor requires a nonzero runner exit, matching specific diagnostic, integer
Rust result/origin/phase witnesses (booleans refused), processor witness0 and positive,
equal prepared/final actual worklet byte lengths. Missing/wrong origins, phase
counters, heap witnesses or memory evidence cannot pass. Existing processor/graph
canaries remain separate: processor10/origin1/heap0, primitive heap4294901761/origin2/
heap4,20,4, graph heap same code/origin2/heap1,5,1. No failure session substitutes for
successful stop/disposal/content proof.

## Commands and observed gates

Commands ran through `nix develop`, `RUSTFLAGS="-D warnings"` and
`RUSTDOCFLAGS="-D warnings"`; full commands/logs are archived.

| Gate | Observed result |
| --- | --- |
| Native fixture debug/release | 59 tests/profile, no unexpected skips |
| Node and Chromium | 20 shared groups/runtime/profile, complete control/native equality |
| Actual offline/live | 16 successful contexts/profile, six exact intentional failure gates/profile |
| Python | 24 tests; new planner phase/type/memory gate and two full-control mutation cases |
| Workspace nextest0.9.116 | 2141 passed/four existing skips,33.780s warm wall time |
| Build/docs | warning-denied workspace/fixture builds, runtime/loop/fixture rustdoc and loop doctests pass |
| Policies | formatting, test attributes, both dependency boundaries, closed tracing inventory pass |
| Documented commands | `--planner-json` and exact control test filter pass in both profiles |

Tools: rustc1.97.1(8bab26f4f), cargo1.97.1(c980f4866), nextest0.9.116(2416f2fb1),
wasm-pack0.15.0, bindgen0.2.127, Node22.22.2, Chromium/driver147.0.7727.137.
Initial and final Rust/Wasm builds plus actual full-profile runs pass. The final
failure-memory metadata was a JS/verifier-only change; its full debug/release actual
runs intentionally used `--no-build` against unchanged verified Rust/Wasm binaries.
No cold-cache, callback-timing, speedup or CI-margin claim is inferred from suite time.

The first full debug run passed all16 successful contexts/20 binding groups but
failed the planner failure **verifier**: it incorrectly expected runtime cursor0.
The retained Rust witness showed cursor128 with one plan/zero finishes. The existing
clock implementation confirms advancement before planning; the corrected strict
verifier requires128 while retaining zero finishes, permanent fault and no-output
checks. Runtime behavior/assertions/workloads were not weakened. Initial failed logs,
raw unsuccessful report and successful control reports remain in the archive.
No unresolved native/Python/browser failure remains.

## Artifacts and remaining scope

Archive `planner-canaries-stage7-logs.tar.gz`: **3941310 bytes**, SHA256
`33adec523c07a6d2501fae326432a9b5b41b2f931169807a187e8bb533cba2be`.
Its root `loop-layer-planner-canaries-stage7/` retains initial/retry/final native and
host logs, the failed verifier report, final workspace/documented-command logs and
`final-reports/{debug,release}` complete native/binding/browser/failure reports.
Parser fixture `scripts/tests/data/loop_planner_reports.json.gz` is11836 bytes
(505499 uncompressed): captured parser inputs, not fresh worklet execution evidence.

Wasm SHA256:

- Debug: `8c27bab3ac8974203e75ffea64bc8d17a10803b4b8487f13f79c39c9503b29ca`
- Release: `e2dd378ac72a86ed0349d213d69ebf633aedef31bf4bfb4b0b5a90e97e86d1b4`

Stage7 is **7/8**, not complete. Two applicable native planner cases bring pending
Stage7 debt to35 graph/export/staging/dense/stall/media/planner cases plus applicable
ingress/observer coverage. They are unregistered/uninterpreted; no per-commit Miri
campaign was launched. Workloads, guards, seeds and900-second watchdog remain intact.
Consolidated interpreter/CI artifacts, Stages8–10, final-source verification,
independent completion audit, unmerged PR, final-head green CI and review remain.
