# Loop-layer implementation and validation evidence

## Current reading path and evidence boundary

The runtime [book/reference](../../src/rust/rt_runtime/docs/README.md) and loop
[task guides](../../src/rust/shoop_loop_engine/README.md) describe the current APIs,
including prepared configuration and paired running-program/graph publication.
[STAGE6_READINESS.md](STAGE6_READINESS.md) maps implementation acceptance at its
recorded source; it is not a new callback audit or final branch/platform certification.
The independent loop fixture now supplies direct, primitive-wire and bounded
two-thread replay plus actual non-isolated offline/live AudioWorklets for its small
primitive scenario, including canonical semantic timeline journals matched against
an independent native table. A prepared graph payload receiver now additionally
has native facade and public-Wasm binding coverage. Opt-in native graph commands
now stage/activate/release and recycle owners through bounded results; graph command
Wasm bindings/MessagePort transport, program/resource wire operations, exports and
broader host scenarios remain pending.
No loop benchmark or newer performance/deadline campaign is supplied.

The following sections are chronological records, including old open-stage counts,
while-running jobs, source/hash checks and the then-required goal pause/commit/worker
workflow. Those statements apply at their checkpoints, not to the current dirty
checkout or to documentation authoring. Former docs-changes-loop-layer references
now resolve under [the implementation-review archive](../../src/rust/rt_runtime/docs-previous/implementation-review/README.md);
the [documentation plan](../../DOCS_REWRITE_PLAN.md) owns completion without new
handoffs/checkers/jobs. Historical reports, failures and raw measurements are unchanged.

## Stage 7 native graph command/return integration

Opt-in `Bridge::with_graphs` connects `LGP1` descriptors to96-byte `LLG1` requests
and128-byte `LLS1` results. Two graph credits are separate from the primitive
four-normal/one-recovery credits. Stage/Release return owning completions; Activate
retains separate admission/execution outcomes. A bounded client graph/bank mirror
checks scoped IDs, epochs and next bank generations before changing credits.

Owning candidates and retired graphs recycle into their original prepared banks.
Unexpected owning completions return intact to quarantine and fault the outer
bridge. Collection uses the fixed seven-credit bound per result stream once per
nonempty callback, not renewed internal musical allowances.

Eight additional native tests cover independent complete codec tables, phase/
identity/epoch/generation rejection, actual serialized route cuts and retirement
across three partitions, all four normal/two graph credits stalled with independent
Stop recovery,128 candidate rounds, malformed/queued/Ready disconnect, local Full
owning return and unexpected-owner quarantine. Complete stereo/MIDI/semantic bytes,
exact receipts and256/16/16/0 recovery remain checked separately.

Fresh results:39 fixture tests per native profile; eleven existing binding groups
per Node/Chromium profile; actual primitive offline/live worklets and failure gates
in both profiles; five Python tests;2141 workspace passes/four existing skips;
warning-denied builds/rustdoc, formatting and applicable policies. Worklet memory
remains1,835,008 debug/1,769,472 release bytes. [Graph-wire evidence](evidence/graph-wire-stage7.md)
retains source, commands, raw results, hashes and exact scope.

The graph command path is native verified and Wasm compiled, **not executed through
new graph Wasm bindings or a worklet**. Two-thread graph payload transport and
structured graph-host reports remain open. No further Stage7 row is complete:
2/8 remain verified; program/resource payloads, exports, broader hosts, consolidated
Miri and Stages8–10 still remain.

## Historical Stage 7 prepared graph payload foundation

`LGP1` is a canonical96-byte payload record format with two prepared local plan
owners, checked generations, full-width sessions/sequences/epochs and explicit
planning metadata. Ordered materialization and whole-record refusal never compile
or allocate a graph on the callback. Seal is not runtime validation/admission.
Owning handoff/recycle retains rejected inputs; retired plans truthfully expose
their actual capacities for later declarations.

Seven native tests cover full-width layout/all truncations/padding, transactional
ordering/refusal, storage/identity/exhaustion,128 candidate lifecycle rounds, actual
facade graph activation and retirement, full ingress, stalled returns and queued/
Ready disconnect disposal. The route cut at4 bypasses FX only at the host output:
three partitions agree on all13 stereo samples,134 MIDI packets, exact receipts/
epochs and the unchanged semantic timeline, followed by full resource recovery.
Two additional shared public-binding checks cover canonical payloads, Wasm32 index
overflow refusal and128 local owner exchanges without memory growth.

Fresh gates:31 fixture tests per native profile, eleven binding checks in each
Node/Chromium profile, actual primitive offline/live worklets and failure canaries
in both profiles, five Python tests,2141 workspace passes/four existing skips,
warning-denied builds/rustdoc, formatting and applicable policies. Worklet memories
remain1,835,008 debug/1,769,472 release bytes. See [payload evidence](evidence/graph-payload-stage7.md)
for source, raw reports, commands, hashes, retained initial cleanup-API compilation
failure and scope. Applicable Stage7 interpreter cases remain pending consolidation.

This does not add graph command/result envelopes, response credits, two-thread
payload transport or actual worklet graph changes. No further Stage7 checklist row
is complete; the verified count stays2/8. Program/resource payloads, exports,
broader musical/adversarial hosts, CI artifacts and Stages8–10 remain open.

## Historical Stage 7 semantic host journals

Prepared `LSJ1` observation records now preserve event/discontinuity boundaries,
all public decision-state fields except opaque origin IDs/private watermark, and
ordered semantic events. Ordinary callback cuts are omitted. The primitive script
produces2440 bytes: four boundaries/four decisions/seven events at0,4,8,12. Native
direct/wire/two-thread and actual non-isolated offline/live worklets match every
byte of an independent table, in both profiles. Stop suppresses the due frame12
wrap/pulse; a separate native/Node/Chromium binding case preserves the eventless
same-position seek decision without inventing a transition or pulse.

Observer storage refuses only whole diagnostic batches. A72-run native matrix of
observer/resolver journal capacities and host partitions preserves complete media,
receipts and resource recovery. Saturating counters distinguish observer loss from
the resolver loss high-water mark; incomplete data cannot pass the report validator.
Full-width scalar/event encoding, optional-field padding, all truncation lengths of
a synthetic report, and malformed counts/flags/identities are checked separately.

Fresh results:24 native fixture tests per profile; nine public-binding checks in
each of Node/Chromium per profile; actual offline/live hosts and real failure gates
in both profiles; five Python validator/gate tests;2141 workspace passes/four
existing skips; warning-denied builds/rustdoc, formatting and applicable policies.
Worklet memory remains fixed at1,835,008 debug/1,769,472 release bytes. The report
is now `LLH2`; historical `LLH1` artifacts remain unchanged.

[Semantic-host evidence](evidence/semantic-host-stage7.md) records source, commands,
raw reports, hashes and the retained initial Rust lifetime error. The fixture and
actual-context/reference-comparison checklist rows are verified; Stage7 remains
open for payload staging, exports, broader musical/adversarial scenarios and CI
artifacts. No new Miri, benchmark, final CI or review pass is claimed.

## Historical Stage 7 primitive loop AudioWorklets

The browser milestone adds host-only Wasm bindings, a unified runner and actual
OfflineAudioContext/live AudioContext execution using the real loop facade. Both
native/Wasm debug and release profiles pass:19 native fixture tests, seven shared
public-binding checks in each of Node/Chromium, full actual downstream stereo
samples/MIDI/terminal receipt comparisons, fixed independent ordinary memories,
returned transfer buffers and complete256page/16version/16snapshot recovery.
OfflineAudioContext's full512-frame output is independently checked against the
probe. Native direct/wire/two-thread replay still agrees across four partitions.

Actual worklet processor and Rust heap failures are separately witnessed and each
make the child runner exit1. Specific diagnostics alone are insufficient: the gate
requires the real processor fault witness and matching allocation/reallocation/
deallocation counts. Two Python tests reject missing/misleading evidence. Worklet
memory stays1,835,008 bytes in debug and1,769,472 in release. These sizes and the
short live callback counts are observations, not deadline or efficiency claims.

Fresh workspace regression is2141 passes/four existing skipped failure canaries;
warning-denied build/rustdoc, formatting and applicable policies pass. See
[browser-host evidence](evidence/browser-host-stage7.md) for complete reports,
commands, tool versions and the retained setup/acknowledgement failures.

Stage7 remains partial. Live commands are staged before acknowledged clock start;
dynamic live ingress and broader musical/ownership scenarios remain open, as do
payload staging, exports and semantic boundary journals. No Miri invocation,
benchmark, hosted application smoke or final-head CI pass is claimed here.

## Historical Stage 7 primitive wire bridge and two-thread replay

The next fixture slice adds fixed64-byte requests/96-byte responses, four normal
wire credits plus independent recovery, prepared cancellation flags and guarded
owning completion collection/recycling. Separate admission/runtime/domain outcomes
survive serialization, including early domain failures before a resolver request is
installed. Direct, wire and two-thread drivers agree across four partitions on all
13 stereo frames/134 MIDI packets/three receipts and complete pool recovery.

Sixteen tests pass per native debug/release profile, including512 requests over128
slot-reuse rounds,1024 full-width codec round trips, malformed/stale/duplicate
messages, result stalls, cancellation, recovery, late/future requests and permanent
fault cleanup. All six driver/profile combinations pass. Debug/release Wasm builds
are compilation only. Fresh workspace regression is2141 passes/four existing skips;
warning-denied build/rustdoc, formatting and applicable policies pass.

[Wire bridge evidence](evidence/wire-bridge-stage7.md) records the corrected early
`Reply::Failed` handling, precise failure-origin expectations and raw logs. This is
still partial Stage7: graph/program/resource payloads, exports/journals, broader
musical scenarios, actual browser hosting and the unified runner remain outstanding.
No new Miri, browser, benchmark or CI pass is claimed.

## Historical Stage 7 native offline foundation

`tests/loop_engine_host` now has its own workspace/lockfile and uses the actual loop
facade for guarded stereo/MIDI record → immediate play → FX → stop. Four callback
partitions agree with independent complete sample/packet expectations and exact
transition receipt frames. Both native profiles pass five tests, including three
negative heap canaries, and replay verifies13 stereo frames/134 MIDI packets/three
transitions with complete256page/16version/16snapshot recovery and zero media refs.
The fixture builds for Wasm but has not executed there or in an AudioWorklet.

Fresh regression gates pass:2141 workspace tests/four existing ignored failure
canaries, warning-denied workspace build and both-crate/fixture rustdoc, formatting,
test policy, production boundaries and tracing coverage. No new Miri, browser,
benchmark or CI pass is claimed. The codec/prepared bridge, two-thread host, actual
worklet, broader scenarios/exports/journals and unified runner remain Stage7 work.
[Native foundation evidence](evidence/native-host-foundation-stage7.md) records
commands, scope, the two corrected fixture failures and retained raw logs.

## Historical Stage 6 completion and user checkpoint

Consolidated job `20261027T025325-e065d48941` completed and the supported collector
accepted its terminal pass: **81 commands/228 executions,128 runtime+100 loop**, from
immutable `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea`. Pinned nightly-2026-05-04 /
miri0.1.0(ad3a598ca4), isolation/seed settings, workloads, heap/destructor guards and
the900-second command watchdog are unchanged. Ended2026-10-27T04:55:23.943226Z;
supervisor elapsed7318.292420581012s. No second routine interpreter run was started.

Evidence: `evidence/miri-stage6.json` (ordered per-command results),
`miri-stage6-collected.json` (immutable manifest/terminal collection),
`miri-stage6-provenance.json` (independent integrity/freshness checks), and
`miri-stage6-logs.tar.gz` (all81 raw case logs plus result report). The result hash is
`d4c8fa7b5e0caf610783a56d178ae4343288b413f63e2841184847ae15df3ae0`.
Verified all671 snapshot entries including executable/symlink metadata, reproduced
the git archive hash, checked the supervisor, exact package/target/filter/count/seed
inventory, pinned toolchain/version and all82 result/log hashes. Reopened the raw
archive and verified every member independently. Current123 scoped Rust and10
configuration/runner inputs plus supervisor/helper hashes still match the snapshot.
Post-snapshot commits only changed documentation/evidence; this is not a whole-tree
clean/fresh claim over concurrent authored drafts and untracked examples.

The independent provenance script's first attempt compared a digest with the whole
manifest entry object rather than its sha256 field; the failed log is retained at
`target/loop-layer-stage6-completion/first-verification.log` and hashed in provenance.
After correcting the driver to verify hash/executable/symlink fields, all checks
passed. The supported collector had already passed; this was not a changed source,
interpreter failure, rerun or weakened test. Collection/verification logs are in
`target/loop-layer-stage6-completion/`.

The dense128-boundary case passed in877.7475190730183s, loop MIDI in675.5860766569967s,
and the final nested lifecycle in590.2424795530096s. These are shared-host interpreter
observations, not native callback deadlines, speedup or guaranteed CI margin. The
historical interrupted171/178 job and all while-running checkpoint counts remain
partial historical evidence; this new complete run does not rewrite them.

Together with source-matched153-loop native debug/release/Node/Chromium profiles,
2141 workspace passes/four existing skips,19 scoped gates and8 positive/23 negative
borrowing proofs, the collected gate closes every Stage6 row mapped in
STAGE6_READINESS.md. `evidence/stage6-completion.json` records the combined evidence
hashes and rechecked19 prior command logs, proof/filter report hashes and prototype
sources. Its first inline count check expected a scalar instead of the actual
package-keyed native-count object; the corrected check passed without rerunning or
changing tests. Both evidence-driver schema mistakes are distinguished from real
interpreter or implementation failures. No new native/Wasm/host/benchmark/CI execution is claimed by
collection. At this implementation checkpoint the user required a pause after its
completion commit, before Stage7. Later implementation models/audits/performance,
independent hosts and final delivery were not certified by this collection.
That historical workflow is not a pause/commit/PR or separate-worker requirement
for the current documentation plan.

## Historical repository runtime and architecture handoff review

Read all seven Stage9 repository runtime documents, three root proposal/design
files, the governing INTER_LOOP_PROCESSING decision, current runtime README and
generic-host README. `repository-runtime-followups.md` and `architecture-followups.md`
now give actionable destination corrections: current versus historical split/counter
bounds, required versus legacy resource work, prepared capture/graph/loop integration,
held-normal recovery, actual API names, host scope and publication/navigation.
INTER_LOOP_PROCESSING needs no semantic change; its historical open questions are
answered by the implementation/SEMANTICS without weakening its decision. Actual new
callback audit, performance/host evidence and later Stage7–8 documentation remain due.

`repository-baseline.json` records **warning-denied rustdoc for both crates and the
draft-book checker**, passing in0.5376039350230712s of warm commands. It verifies123
scoped Rust inputs,10 configuration/runner inputs,11 archives,15 protected baseline
inputs, nine historical runtime reports, the existing golden journal's exact raw
SHA256 and unchanged protected working-tree diff. DONE_INITIAL_CRATE is untouched.
Raw artifacts: `target/loop-layer-docs-repository/`. No native/Wasm suite, browser,
benchmark, CI or interpreter was rerun for this document review. The old absent
src/rust/rt_runtime/CONTRACTS.md is not recreated. These checks do not certify prose
publication or turn historical measurements into current results.

All named Stage0–6 book/loop/repository documentation impacts now have reviewed
handoff instructions. Stage9 itself remains open for later implementation impacts,
actual audits/evidence, new examples, separate-worker publication and final checks.
At this checkpoint the existing consolidated Miri job remains **running,75 completed
commands/220 of228 executions**, processing the unchanged dense128-boundary case
under its900-second watchdog. No terminal collection or Stage6 completion is claimed.

## Historical loop-guide and facade-contract handoff backfill

Read all11 loop README/guide destinations and the complete SEMANTICS,
RUNTIME_INTEGRATION and FACADE_DESIGN documents. New `loop-foundations-backfill.md`
and `loop-runtime-backfill.md` under the runtime handoff directory give per-destination
corrections, preparation/commands/composites/media and state/export/shutdown recipes,
and explicit attribution of current prepared-template configuration and paired
program/graph implementation. Authored guides/architecture, example and archives
remain untouched. Repository runtime architecture/audit/performance/fixture review,
later-stage deltas and separate-worker publication remain open.

`loop-guides-baseline.json` records **coordinated_loop debug/release and warning-denied
loop rustdoc**, all passing; raw logs are `target/loop-layer-docs-loop/`. The three
warm commands took5.4142942399776075s in isolated reused documentation output, not a
cold build or performance measurement. The example checks stereo arrays, transitions,
initial held note, exact exported audio/MIDI bytes and256pages/16versions/16snapshots/
zero media references at teardown. It has no allocator guard itself: the existing
guarded integration evidence is separate. All123 scoped Rust/10 configuration-runner
inputs and11 runtime archive hashes remain unchanged. Prior153-loop native/Wasm and
2141-workspace evidence is referenced, not freshly rerun.

Source review also corrected chapter14's overly broad recovery wording: recovery
must resolve at its delivery frame before positive interval advancement, not merely
before callback end. The loop guide needs the full current recovery-safe set; plain
LoopId/RequestId lack an instance-scope field; AfterInterval is conceptual rather
than an actual facade Cut member; imported channel padding requires explicit host
preparation rather than an unimplemented facade option. These are documentation
clarifications, not production semantic changes or acceptance-scope reductions.

At this natural checkpoint the consolidated job remains **running,70 completed
commands/215 of228 executions**, processing the stopped-input/half-open-prefix runtime
case (`70-runtime-seed0.log`). No second interpreter job was started. This is partial
progress, not terminal collection or Stage6 completion.

## Control/planning handoff backfill

Chapter5–8/13–15 notes now complete the runtime book's earlier-slice backfill:
owning command/resource returns, ordinary routing, approved exact-parameter prose,
packet/reset MIDI, positive planner intervals, retained typed outcomes and scoped
graph publication. All19 chapters have actionable review notes. Full loop-guide/
architecture backfill, reference tables, authored examples/publication and final
LL-A12 acceptance remain outstanding. Six reviewed destinations remain empty;
chapter7's approved prose/source marker remains unchanged.

`src/rust/rt_runtime/docs-changes-loop-layer/control-baseline.json` records **four
fresh warning-denied example executions** (gain_graph/planned_gain in debug/release)
and the drafting-only book check. The prior operation-baseline's188 native cases per
profile are explicitly reused, not rerun: all123 scoped Rust inputs,10 configuration/
runner inputs,11 archive hashes and prior command-log hashes still match. Raw fresh
logs: `target/loop-layer-docs-control/`. The five warm commands took0.5183642870106269s
in the existing isolated documentation build directory; this is neither a cold build
nor performance/deadline evidence.

The review identifies an important example limitation: gain_graph guards callbacks
and checks exact samples/owning teardown, but planned_gain is one-consumer and has
no heap guard. Its32sample/five-interval/typed-result assertions pass; they do not
supply the proposed guarded two-consumer or pending-receipt book examples. Notes
require the documentation worker to implement those without rewriting approved
parameter prose or claiming missing publication. No authored chapters/examples,
archives, worker plan or production Rust were changed.

At this checkpoint the existing consolidated job remains **running,67 commands/
207 of228 executions completed**, executing the loop MIDI target. This remains
partial interpreter progress, not a collected pass. No second Miri job was launched;
Stage6/task completion is still withheld pending terminal provenance verification.

## Capacity/prepared-ingress/shutdown handoff backfill

The chapter17/19 notes now include Stage3–6 storage/work/ownership/extraction
requirements, and chapter18 has a complete actionable prepared-ingress example
specification. All19 chapters have notes, but earlier chapter5–8/13–15 slices still
need their complete backfill and separate-worker publication/example verification
remains pending. The three Part V destination chapters are still empty.

`src/rust/rt_runtime/docs-changes-loop-layer/operation-baseline.json` records current
source/archive/destination hashes and twelve checked commands: **188 rt_runtime tests
per native debug/release profile**, seven existing `bounded_runtime` workloads at
**1024 measured callbacks each per profile**, six deliberate allocation/reallocation/
deallocation canaries producing exit101 with the expected guard diagnostic,
warning-denied rustdoc, and drafting-only book checks. The bounded example retains
its sample/MIDI/content/ownership/shutdown assertions. All11 archive files,123 scoped
Rust files and relevant configuration/runner inputs remain unchanged. Raw logs:
`target/loop-layer-docs-operation/`.

These are prescribed documentation smokes, not reduced replacements for the full
benchmark campaign, independent loop benchmark or actual loop AudioWorklet. Native
timings share the machine with the ongoing Miri job and carry no deadline/cold-cache/
CI-margin claim. The bounded example's capture uses legacy quota-based I/O; it does
not measure the new required/group/loop paths. No new prepared-ingress/shutdown book
example or authored chapter was created here.

The review also found the crate rustdoc introduction still refers to “split” credits
and describes all resource exchange as quota-based. Cross-cutting handoff instructions
now explicitly require correcting that stale prose; passing rustdoc compilation does
not prove semantic prose accuracy. Current loop configuration/program-graph integration
supersedes the original cross-cutting notes' blanket future-integration statements.
No production Rust was changed during this review.

At its natural checkpoint, consolidated job `20261027T025325-e065d48941` remains
running with **53 commands/178 of228 executions completed**, executing the paired
cancellation/released-generation case. This is new-job partial progress, not the
historical178-selection's completion or a terminal collected pass. Stage6 stays open.

## Resource/MIDI/capture handoff backfill while consolidated Miri runs

Five new actionable notes cover chapters9–12 and16 under
`src/rust/rt_runtime/docs-changes-loop-layer/`. Reviewed the still-empty destinations,
full archived resource/MIDI-index/MIDI-state/capture-group guides, public format,
render/index/state/capture APIs and stamped group lifecycle. The notes specify small
public-API examples with independent bytes/state/cut/capacity/refusal/cleanup checks;
no teaching chapter or new example has been authored by this worker.

`media-baseline.json` in the handoff directory records reviewed hashes, all11 archive
files unchanged from the initial handoff, all123 scoped Rust source hashes and relevant
configuration/runner inputs unchanged, and fresh **63 existing tests per native
debug/release profile across15 targets**. Native discovery verifies63 cases. The
book checker still uses drafting-only `--allow-pending` with14 pending chapters.
Raw logs: `target/loop-layer-docs-media/`. The separate worker must still implement,
execute and publish the chapter examples and verify the final book. No fresh Wasm,
whole-package, independent-host, or interpreter acceptance is inferred here.

The review explicitly distinguishes raw-ID same-store preconditions from opaque
scoped views; physical resource revisions from loop take revisions; page-byte,
event and strict pre-target state positioning; state reconstruction from renderer
restoration; optional snapshots from promised group final leases; and first-interval
preflight from publication. It corrects obsolete archive claims of missing loop
integration without claiming the independent loop AudioWorklet is already delivered.

At this natural checkpoint, the existing immutable Stage6 job remains running:
**37 completed commands/162 executions**, with the configuration input-policy case
in progress. This is partial progress, not a collected pass. No second interpreter
job or implementation change was made. Authored drafts/examples/archive/other-worker
plan and unrelated working-tree changes remain untouched.

## Consolidated Stage6 job and concurrent Part I handoff review

Readiness inputs were committed as `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea`.
Job `20261027T025325-e065d48941` launched with81 commands/228 selected executions
from its immutable export; the first documentation-review checkpoint verified the
supervisor running. Subsequent progress recorded8 completed commands/49 executions,
with `midi_state` in progress; the handoff commit checkpoint subsequently recorded
22 commands/111 executions and `capture_group_model` in progress. This is partial
progress, not terminal collection,
full Miri acceptance or Stage6 completion. See `MIRI_JOBS.md` for manifest identity
and required collection. No second routine job is launched.

Concurrent useful work fully reviewed the four populated Part I book chapters,
`first_graph`/`prepared_processor`/`callback_adapter`, the documentation worker's plan,
and archived getting-started/integration/planning/approved parameter style. Four new
change notes under `docs-changes-loop-layer/part-1-process-audio/` preserve the working
examples and identify focused authority, required-I/O, interval and fault-policy
clarifications. In particular, generic `FailOnce`/`NoPlanner` continuation is not a
promise that arbitrary domain planners can resume after partial publication.

`docs-changes-loop-layer/part-one-baseline.json` records unchanged prior drift hashes
for all four drafts, all11 archive files unchanged from the initial handoff baseline,
reviewed source hashes,23 checker-test passes, the drafting-only `--allow-pending`
book check, and all three examples executed in debug/release with warnings denied
and isolated Cargo output. Raw logs: `target/loop-layer-docs-part-one/`. No authored
chapter/example/archive/separate-worker plan was edited or staged. Fourteen pending
book chapters remain allowed by that draft check; it is not full-book acceptance or
separate-worker publication. The ongoing Miri snapshot is unaffected.

## Latest Stage 6 nested lifecycle verification and readiness audit

`evidence/nested-lifecycle-stage6{,-provenance}.json` records153 loop cases per
native debug/release/Node/Chromium profile,2141 workspace passes/four existing skips,
19 scoped gates,123 Rust source hashes/120 unchanged prior inputs, and refreshed
8 positive/23 expected negative borrowing proofs. The new guarded interloop case
predicts all four-port audio/MIDI, every public state at all20 boundaries, full or
truncated journals/drop counts, countdown cancellation and three immutable exports.
Outer seeks cross an explicit inner program's play/record/stop spans; direct Stop
wins over an inherited Record, direct child playback survives parent Stop, and
restart creates revision2 without a phantom intervening take. All expected states
must be present. Channel0 initial MIDI projection is checked; multi-channel protocol
coverage remains in its separate tests. Whole/single/irregular partitions, declaration
reversal and journals0/1/1024 agree. Cleanup twice recovers256 pages,64 versions,
64 snapshots and zero ledger references. The original137-page exhaustion test remains
unchanged in capacity/workload and passes separately.

The planner skips the pending graph lookup when no program is selected. There is
no measured speedup, changed musical rule or new public API claim. A first19-gate
pass before that adjustment is archived with its planner source under
`target/loop-layer-nested-lifecycle-stage6/first-broad/`; final gates/proofs/filter
mirrors were rerun against final source hashes. The initial fixture's open-ended
range needed whitespace before `=>`; failed output is retained as `initial.log`,
not a complete failed-tree reproduction. Final19-gate warm duration246.60s is not
a deadline, cold-cache result or CI margin.

Miri **228=128runtime+100loop** remains registered, not interpreted. All28 topology
and both interloop cases now use individual exact filters, with unchanged workloads,
seed0, guards and900-second watchdog. Native discovery matches all30 names and each
filter executes exactly one test. Historical interloop wall time468.43s and grouped
suite growth motivate granularity, not a claim of a new timeout. Native mirrors and
14 runner-helper passes are not interpreter evidence.

[STAGE6_READINESS.md](STAGE6_READINESS.md) maps all remaining stage requirements to
implemented paths and actual cases. Consolidated committed-snapshot Miri remains the
stage-level gate; Stage6 is not complete. Broader Stage8 models/stress/performance,
independent loop hosts, separately published documentation and final CI/review remain
required. Handoff notes/baseline: `docs-changes-loop-layer/loop-nested-lifecycle.md`
and `nested-lifecycle-baseline.json`; authored guides were not modified.

## Stage 6 paired running-program/graph milestone

`evidence/program-graph-stage6{,-provenance}.json` records paired zero/stopped-cut
publication through `ProgramGraphActivate`/`ProgramGraphChanged`. A crate-private
selection query requires a valid uncommitted resolver preview; the facade holds the
generic graph permit through real capture preflight and resolver publication before
committing graph ownership/epoch. Family refusal leaves both candidates unpublished.
Released/reused graph generations and independent graph activation fail the pending
pair, rather than retargeting it or undoing separate operations.

Four guarded cases verify independent full stereo/MIDI/state/journals across callback
partitions, declaration order and journals0/1/1024; real FX audio6/60 and MIDI capture,
prefix/full exports and initial held-note state versus resource-family refusal;
cancellation and six owner lifecycle branches; sixteen domain slots/unread outcomes
and displaced owners through128 cycles; recovery, disconnect and render fault after
publication. Existing probe tests add invalid/valid selection-query phase checks.

All19 scoped gates pass:152 loop tests each in native debug/release/Node/Chromium,
2140 workspace tests/four existing skips, warning-denied build/rustdoc/examples,
boundaries/policies and14 helper tests. Eight positive lifetime programs and23
expected compile failures were refreshed.122 source hashes include114 unchanged
preceding inputs; concurrent documentation examples are separate. Warm gate time
252.59s does not imply a deadline. Miri227=128runtime+99loop is registered only.

Raw logs: `target/loop-layer-program-graph-stage6/`. The initial capture fixture's
E0609 used `Endpoint.port_type` rather than `kind`; the compiler log is retained,
not an archived failed source tree. No assertion/workload/guard was weakened.
Separate-worker changes and reviewed-guide hashes are in
`loop-program-graph-activation.md`/`program-graph-baseline.json`. Stage6, broader
nested/mixed lifecycle verification, actual loop hosts and final delivery remain open.

## Stage 6 configuration milestone

`evidence/configuration-stage6{,-provenance}.json` records coordinated stopped-owner
configuration over prepared physical roles: local owning snapshot/source staging,
callback-shared MIDI header validation, generation-checked edit banks, graph/timing/
media publication, explicit input projection policy and capture-claim checks.
Seven integration cases and a private exhaustion/failure case cover exact independent
stereo/MIDI/public-state/journals and retained old/new exports; kind change and spare
consumer removal/reuse; saturated requests/recovery; all-bank refusal including the
final resolver step; batch capacity without retirement-credit reuse; and eight
unread/disconnect/publication/fault teardown phases. All callbacks/lifecycle work are
under heap guards. Prepared physical identities/shapes remain fixed.

Nineteen scoped gates pass:148 loop tests each in native debug/release/Node/Chromium,
2136 workspace tests/four existing skips, warning-denied build/rustdoc/examples,
boundaries/policies and fourteen Miri-helper tests. Eight positive prototypes and
23 expected compiler refusals were refreshed.119 source hashes include105 unchanged
preceding inputs; concurrent documentation examples are separately hashed. Warm
suite time252.77s is not a deadline claim. Miri223=128runtime+95loop is registered,
not interpreted. Full Stage6 and independent loop hosts remain incomplete.

Logs are in `target/loop-layer-configuration-stage6/`. Retained fixture failures:
range-token syntax, source-packet versus restored-note channel ordering, cumulative
journal-drop count, reusing an unread recovery receipt, Timeline versus Runtime
classification of a foreign topology ID, initial diagnostic epoch before any callback,
and two resource slots providing two snapshot credits. Assertions/workloads were not
weakened. `initial-reference.rs` preserves the initial MIDI-reference fixture;
other failed intermediate trees are not claimed reproducible from logs alone.
See the separate-worker `loop-configuration-activation.md` handoff and reviewed-guide
hash baseline. No authored guide was rewritten for this slice.

## Scope and provenance

`PLAN_LOOP_LAYER.md` governs this implementation. Its LL-A1–LL-A12 criteria are immutable without user approval. The initial runtime record in `DONE_INITIAL_CRATE.md` and `docs/rt_runtime/evidence/` is preserved; passing baseline tests is not evidence that new loop-layer behavior exists.

Execution was originally authorized through confirmed goal `mvnjf70y-cjcfar` and carried forward by confirmed goal `mvo6lnim-1xzkd7`. The new tracker carries completed Stages 0–3 forward without restarting those implementations. Stages 4–5 are now verified; Stage 6 is current and Stages 6–10 remain incomplete. The implementation branch is `feat/loop-execution-layer`, based on `cc6ccb4ac35114d61ef7e1c03ecc7f2fe81bb867`. At branch creation, [runtime PR #878](https://github.com/SanderVocke/shoopdaloop/pull/878) remained open against `master`, with that same head. The delivery PR should initially target `feat/generic-rt-runtime` unless the parent merges before delivery; recheck rather than assuming its future status.

The pre-existing edits to `DESIGN_RT_ENGINE.md`, `PROPOSAL_RT_ENGINE.md`, and `PROPOSAL_LOOP_LAYER.md`, plus the new plan, are the intended architecture baseline. Unrelated `.pi/` state is left untouched and untracked. No production application integration is included.

## Acceptance-to-evidence map

These are required test/evidence groups, not claims of implementation. Add concrete test names and artifacts as stages complete.

| Criterion | Planned implementation/evidence | Status |
| --- | --- | --- |
| LL-A1 | Four-target dependency audits; both crate builds; native and Node/Chromium execution; host-only browser bindings | Compiler milestone: both boundary audits pass; new crate executes natively and in Node/Chromium. Full integration/CI pending |
| LL-A2 | Generic synthetic planner dependency/view tests; resolver ownership tests; graph receiver validation | Resolver, generic planner lifecycle and coordinated primitive facade/role validation verified; live topology/program integration pending |
| LL-A3 | Same-frame A → B → C and nested cascades; one-sample loops exceeding old subdivision allowances | Pure primitive/composite execution, actual facade 128 one-sample media boundaries and integrated A→B→C/delayed nested activation verified; broader lifecycle and independent hosts pending |
| LL-A4 | Semantic progress proof; valid mutual sync; rejected instantaneous recursion; generated valid/invalid programs | Stage 2 compiler/resolver proof and tests pass; integrated audit pending |
| LL-A5 | Independent reference journals/audio/MIDI across callback partitions and declaration orders; exact end-boundary cases | Pure semantic journals and coordinated primitive sample/packet/content/journal oracles match across partitions/orders; integrated composites and independent hosts pending |
| LL-A6 | Debug/release/Wasm allocation/reallocation/deallocation canaries and guarded callback scopes; callback audit | Generic planner/resources and full primitive facade guarded in native debug/release/Wasm and selected Miri; later whole-layer audit and independent-host canaries pending |
| LL-A7 | Owning return/cancellation/generation/saturation/disconnect/teardown models; selected Miri and concurrency tests | Generic ownership models plus integrated media pins/exports/cancellation/recovery/faulted teardown verified; live replacement and later stress remain pending |
| LL-A8 | Stereo/MIDI record-to-play, seek/replacement/cleanup/export; FX capture graph; common-prefix exhaustion | Stage 5 complete: indexed audio/MIDI, exact record→play/seek/replacement/cleanup, coherent export, actual separate FX capture graph, common-prefix exhaustion and public lifecycle example |
| LL-A9 | Integrated prepared composites, primitive/composite interactions, and rendering-observation boundary tests | Countdown, A→B→C, delayed regular/explicit nesting and exact capture outcomes verified; live replacement and broader lifecycle remain pending |
| LL-A10 | Independent render/maintenance accounting, indexed access, capture reservations, counter/trace exhaustion, pool recovery | Required accounting/indexing/groups and coordinated preflight/settlement verified, including optional export Budget refusal without musical interruption; later mixed integration/audit pending |
| LL-A11 | New independent native/worklet fixture journals, actual Web Audio output, fixed-memory and failure gates; raw release measurements | Pending new fixture |
| LL-A12 | Updated runtime/loop guides and executable examples, acceptance audit, milestone commits, final-head CI and review record | Pending |

## Stage 0 — Verified pre-implementation baseline

Milestone: `6ad71196b63e61956610ec2a71ecc48369d7a040` — `docs(loop-layer): establish execution plan and verified baseline`. No runtime, processor, resource, browser-host, or test behavior was changed in this milestone.

### Environment

Local Linux x86_64, repository `nix develop` environment:

- Rust/Cargo 1.97.1; installed targets `x86_64-unknown-linux-gnu` and `wasm32-unknown-unknown`.
- Prescribed cargo-nextest **0.9.116**, from `$HOME/.cache/rt-runtime-nextest-0.9.116`; the shell default 0.9.143 was not used for the suite.
- Node **22.22.2**, wasm-pack **0.15.0**, wasm-bindgen **0.2.127**, wasm-bindgen-test **0.3.77**.
- Chromium and ChromeDriver **147.0.7727.137**, with `CHROME_BIN` explicitly set.
- `RUSTFLAGS="-D warnings"` set for the baseline runner and child builds.

Exact environment overrides, commands, durations, exit statuses, log hashes, and package counts are in [`evidence/baseline-stage0.json`](evidence/baseline-stage0.json). Raw logs remain under `target/loop-layer-baseline/`. The runner was invoked as `nix develop --command python3 /tmp/loop-layer-baseline.py`; its individual commands are recorded in the report so the temporary orchestration file is not required for reproduction.

### Results

| Gate | Observed result |
| --- | --- |
| Main workspace and independent-host formatting | Passed |
| `cargo test --locked -p rt_runtime` | 93 passed, 0 failed/ignored |
| Same runtime tests, release | 93 passed, 0 failed/ignored |
| Warning-denied locked workspace build | Passed |
| Prescribed nextest full workspace, `shoop_engine/app_backend`, CI profile | 1893 passed, 4 existing ignored tests |
| Shared test attribute policy | Passed |
| Runtime production/build boundary, Linux/Windows/macOS/Wasm target trees | Passed |
| Boundary-audit Python tests | 2 passed |
| Runtime Wasm-test dependency isolation | Passed |
| Closed tracing inventory | Passed, 169 modules classified |
| Native host offline and two-thread replay, debug and release | Passed; exact response/sample checks |
| Fresh non-isolated offline/live AudioWorklet plus native replay | Passed |
| Deliberate processor and Rust heap worklet failure gates | Both detected, each child exited 1 as required |
| Fresh runtime Node and Chromium package suites | 89 passed each, 0 failed/ignored |

The four full-workspace ignored tests were enumerated from the nextest JSON test list: `intentional_failure_publishes_trace`, `native_result_failure_canary`, `shared_failure_canary_is_ignored_by_default`, and `shared_result_failure_canary_is_ignored_by_default`. They are existing opt-in failure canaries, not newly skipped loop/runtime coverage. Their binary IDs are retained in the report.

The Wasm package suites reused the runner's input-hash-validated application asset bundle; its metadata still records its earlier build SHA. The package tests were freshly executed, but this stage does not claim a fresh production-application browser smoke. The independent runtime worklet was separately built and executed.

### Actual worklet and native agreement

[`evidence/hosts-stage0.json`](evidence/hosts-stage0.json) records a fresh actual browser run:

- Offline: **2601 callbacks / 332928 frames**; sample hash **480331717**, MIDI hash **3308430490**, graph hash **487351092**, **34 MIDI events**. Both native offline and two-thread replays agree.
- Live: **13444 callbacks / 1720832 frames**, with acknowledged-time checks. Live message timing is not equated with deterministic offline timing.
- Independent instances and `cross_origin_isolated: false`; memory starts and ends at **1507328 bytes** with per-callback growth checks.
- Observed core work maxima: **1042 copied bytes / 512 validation steps** under the current runtime contract.
- Five withheld transfer credits, continued rendering for 2048 frames, duplicate/oversized refusal, delayed delivery, successful resource/ownership disposal, and both intentional failure sessions were exercised by the fixture.

The fresh uncompressed journal SHA-256 is `fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237`, matching the initial golden journal. Neither the golden artifact nor historical evidence was replaced. [`native-debug-stage0.json`](evidence/native-debug-stage0.json) and [`native-release-stage0.json`](evidence/native-release-stage0.json) separately identify native-only replay as replay, not browser execution.

### Limits of this evidence

This establishes a clean local regression baseline for the inspected source, not the new loop-layer acceptance criteria. Windows/macOS execution, new loop-library behavior, new Miri cases, updated documentation/examples, and final PR CI remain future stage requirements. No browser/device hard-real-time or performance-improvement claim follows from these results.

## Stage 1 — Semantics and integration contract

Milestone: `7dfd657c7b908d79d6a95e6cb9e2c66c6a5bcb3b` — `docs(loop-layer): specify timing semantics and planner ownership contract`.

- [`SEMANTICS.md`](SEMANTICS.md) specifies primitive/composite state, exact boundary ownership, typed request outcomes/cancellation, ready-versus-clock-reactive arbitration, pulse coalescing, source-stop behavior, seeking/replacement, media/MIDI discontinuities, and legacy behavior mapping.
- Its termination model accepts pure sync strongly connected components, rejects internal state-changing action feedback/recursive containment, and walks the resulting component DAG. Clock delivery and pending eligibility have finite prepared identities, not a wave quota. Pre-clock and reactive phases distinguish suppression of an as-yet uncreated clock edge from attempted retraction of a delivered one.
- The normative boundary table states expected results for record-to-play, cascades, callback ends, dense wraps, valid/idle mutual sync, rejected instantaneous feedback, source stop, direct conflicts, reactive stops, countdowns, nesting, capture failure, source stalls, and MIDI seeks. These expectations were reviewed against the stated phase/progress rules; executable production/reference conformance is still required in Stage 2 onward.
- [`RUNTIME_INTEGRATION.md`](RUNTIME_INTEGRATION.md) specifies generic planner types and borrow phases, explicit graph bindings, one callback admission lifecycle, retained pending receipts, recovery-safe cancellation, prepared program staging, separate rendering/maintenance accounting, indexed resources, exclusive capture claims, group reservation/preflight, coherent state, and the native/browser adapter boundary.
- Resource preflight uses unpublished shadow state and provisional reservations. A failed preview must latch a new finite group/activation failure before retrying; no externally visible action or generation is consumed twice. The failure table distinguishes ordinary refusal, incomplete capture, invariant/programming faults, and consumer stalls.
- A recording may explicitly monitor a retained immutable take while capturing a distinct revision, allowing playback → FX → recording without an artificial signal cycle or in-place overdub. Both playback and capture positions are resolver-owned and tied to the same absolute interval.
- `DESIGN_RT_ENGINE.md` and `docs/rt_runtime/CONTRACTS.md` now describe these as specified-but-unimplemented extensions. Initial runtime contracts/evidence remain labeled historical. No LL-A goal or acceptance criterion was weakened.

### Compiled type/lifetime experiment

[`prototypes/interval_lifecycle.rs`](prototypes/interval_lifecycle.rs) is a safe-Rust non-musical prototype, not production runtime code. [`evidence/contracts-stage1.json`](evidence/contracts-stage1.json) records source hash, commands, outcomes, log hashes, and limitations.

Native debug and optimized runs each pass six assertion groups: a single callback with a decision boundary; irregular callbacks with reversed consumer bindings; 128 one-frame callbacks; 128 internal intervals in one callback; invalid binding refusal; and observation writer exhaustion preserving the already written value. Samples and consumed-observation totals match the independent scalar expected values. Callback admission counters advance only on nonempty host callbacks, not per interval.

The deliberately invalid live-view-reuse configuration fails with `E0502` (`cannot borrow planner as mutable because it is also borrowed as immutable`). Ordinary native and Wasm builds pass with warnings denied. The Wasm binary was compiled, not executed; no AudioWorklet claim is made for this prototype. It has no allocator instrumentation and does not prove real resource transactions, graph validation, scheduler receipts, or musical behavior.

Workspace formatting, the warning-denied locked workspace build, and the shared test-attribute policy pass. Full production/reference/heap/native-browser integration evidence remains pending as shown in the acceptance map; Stage 1 is a verified contract/prototype milestone, not completion of any end-to-end loop feature.

## Stage 2 — Compiler milestone (stage remains in progress)

Milestone: `3a764206bf7f72609fabefb6e257f2c559e6d88b` — `feat(loop-layer): add portable crate and prepared timing compiler`. Evidence: [`evidence/compiler-stage2.json`](evidence/compiler-stage2.json). Source hashes identify the tested implementation on top of the Stage 1 commit; raw logs are under `target/loop-layer-compiler/`.

Implemented interfaces in `src/rust/shoop_loop_engine`:

- Distinct generation-checked `LoopId`/`RequestId`, compact errors, checked `Limits`, workspace lint/test conventions, and Wasm-test metadata. Production dependencies contain only `rt_runtime` and its generic transitive dependency; shared test support is dev-only.
- `TopologyDescription` → `PreparedTopology`: stable identities, target validation, canonical dependency ordering, iterative strongly connected component analysis, accepted pure synchronization cycles, rejected action-feedback components, and rejected recursive composite containment.
- `ProgramDescription` → sparse `CompiledProgram`: sequential sections, parallel/delayed/repeated entries, child-first nested duration derivation, separate regular/explicit-mode classification, same-mode continuation normalization, half-open ranges, deterministic overlap selection, and binary-search lookup without scanning event history or allocating per tick.
- A separate loop dependency audit and Python tests. The original runtime audit is unchanged and still rejects the loop crate as a runtime dependency. Five new source modules have explicit tracing-inventory classifications; no application tracing dependency was added.

Verification in the repository Nix environment:

| Gate | Observed result |
| --- | --- |
| New crate, native debug/release | 19 tests passed per profile; no failures/ignored tests |
| New crate, Node/Chromium | 19 tests passed per runtime; no failures/ignored tests |
| Independent graph reachability oracle | 4096 generated cases; accepted component membership/order and invalid action feedback checked |
| Independent dense program table | 1024 generated programs; sparse lookup matches every represented tick/target |
| Deep topology | 2048-node action chain; iterative compilation, not a wave-limited executor |
| Long sparse program | One segment represents one billion ticks; guarded immutable lookup |
| Shared heap guard | Positive allocation/deallocation canaries pass; value validation and immutable lookup are heap-free |
| Full prescribed nextest workspace suite | 1912 passed; same four existing opt-in failure canaries skipped |
| Both four-target production/build boundary audits | Passed; target trees are metadata checks, not Windows/macOS execution |
| Python boundary-policy tests | 5 passed |
| Wasm test-dependency policy, Rust test policy, formatting, warning-denied workspace build | Passed |
| Closed tracing inventory | Passed; 174 modules classified |

These tests validate compilation and lookup only. They do **not** establish resolver interval/partition invariance, callback cascade execution, reliable pending requests, resource transactions, or loop AudioWorklet behavior. Compiler preparation is allowed to allocate; the guard does not falsely impose callback constraints on it. At this compiler milestone, Stage 2 remained open for the resolver, fixed scratch/state, synthetic observations, independent musical simulator, and guarded rejection/replacement paths.

## Stage 2 — Primitive resolver milestone (stage remains in progress)

Milestone: `9f8c60f5d7896a155a45dff70da154434ddd9267` — `feat(loop-layer): resolve primitive timelines with transactional previews`. Evidence: [`evidence/primitive-stage2.json`](evidence/primitive-stage2.json). Exact commands, source/log hashes, counts, runtime summaries, and environment overrides are retained there; raw logs are under `target/loop-layer-primitive/`.

### Implemented interfaces and ownership

- `command`, `state`, `decision`, and `resolver` provide typed primitive operations, fixed pending/terminal records, immutable interval decisions, synthetic boundary observations, and authoritative logical positions.
- `Resolver::preview` copies into prepared shadow storage and settles the boundary without publishing results, state, or pulses. `commit` swaps the prepared storage; `finish` advances only the selected nonempty interval. Invalidated/failed previews cannot be committed or expose valid decision views. Resource preflight integration is still pending; no retry work quota was introduced.
- Prepared sync components are visited in canonical order. Each seeded member delivers its clock once; pending countdowns consume that edge once. Direct intent precedence, pre-clock stop masks, reactive starts/stops, automatic wraps, and same-frame trigger cascades share this algorithm. Extra host cuts do not seed clocks.
- Pending and unread terminal records retain their slots. Cancellation and result recycling check generations; ordinary result stalls do not stop automatic progression. These are domain bookkeeping records, not a second owning transport. Runtime receipt/recovery-lane integration remains pending.
- Topology replacement checks the expected epoch, preserves retained logical states, rejects active removal, revalidates pending targets/sources, and returns the old owner. Per-slot generation high-water marks prevent resurrection after removal. Returned/rejected descriptors are destroyed outside the guarded scope.
- Storage sizing includes shadow states, receipts, decisions, pulse/eligibility marks, and both journals. Preparation initializes/touches storage; processing does not resize it. Journal overflow is saturating diagnostic loss, not a musical-work refusal.

Three primitive edge cases are now explicit in `SEMANTICS.md`: supplied prepared initial metadata may establish a stopped loop's length; a fresh recording emits one start edge but no periodic recording clock; an empty finish-to-play fails transactionally without freezing, so the caller can continue recording or explicitly stop. The latter preserves atomic precondition failure and avoids fabricating a zero-duration playing state. Logical capture serials retain a high-water mark when an older content revision is selected. These clarify the specified behavior without changing scope or LL-A criteria.

### Verification and limits

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | 36 tests per profile: 19 compiler + 17 resolver; no failures/ignored tests |
| Existing runtime native debug/release | 93 tests per profile; no failures/ignored tests |
| Loop crate Node/Chromium | 36 tests per runtime; no failures/ignored tests |
| Independent single-sample primitive model | 256 generated scripts × 32 requests × 64 frames; complete journals, per-sample logical decisions, every terminal outcome, and final state compared across three partition/order profiles |
| Same-frame execution | 256-node cascade under heap guards; direct/reactive conflict, valid/idle mutual sync, and source-stop tests |
| Dense progression | One-sample loops over 128 frames; exact callback-end treatment and irregular/one-frame partition agreement |
| Failure/ownership paths | Repeated unpublished preview/refusal, duplicate-observation rejection, cancelled/stalled receipts, stale identities, owner-returning replacement, empty finish, maximum countdown, frame/revision exhaustion |
| Optional diagnostic storage | Disabled journal gives the same logical samples/state; diagnostic loss accounts for omitted events |
| Full prescribed nextest workspace suite | 1929 passed; four unchanged existing opt-in canaries skipped |
| Formatting, warning-denied workspace build/rustdoc, both dependency audits, five Python policy tests, Wasm/test/tracing policies | Passed; tracing inventory now covers 178 modules |

Allocator guards cover each production resolver operation, including submission, preview, commit, finish, cancellation, recycling, and replacement/refusal, in native debug/release and Wasm. Preparation and the allocating test oracle/journal collection are deliberately outside those scopes. This is not yet a whole AudioWorklet callback safety claim.

The independent reference advances one sample at a time and does not call the production resolver, component compiler, or interval selector. Its generated execution coverage is currently **single-primitive**; multi-loop timing has explicit expected-case and partition tests, not a completed generated multi-loop/composite simulator.

Logical recording tests prove positions and observation timing, **not captured audio/MIDI or actual resource transactions**. At this primitive milestone, composite descriptions compiled but were explicitly rejected by the interim resolver. Composite execution/reference tests, required render effects and media binding, generic planner/receipt integration, Miri, actual hosts, and final CI were still pending. No further Stage 2 checklist item or end-to-end LL-A criterion was marked complete on that partial milestone.

## Stage 2 — Complete pure primitive/composite resolver

Milestone: `feat(loop-layer): execute prepared composites and verify pure resolver` (the commit introducing this section). Evidence: [`evidence/resolver-stage2.json`](evidence/resolver-stage2.json); raw logs: `target/loop-layer-stage2/`. The report identifies the Stage 2 primitive base commit and hashes every tested Rust source, including both independent simulation implementations.

### Implemented composite contract

`State::composite` contains iteration, pass count, and iteration-entry frame. Actual clock delivery remains separately recorded in `last_pulse`; composites never advance a synthetic sample playhead or mint periodic clocks from nominal units. `PlayIteration` and `SeekIteration` select prepared snapshots without replaying earlier events. Non-retriggering seeks reposition already-playing descendants without inventing new clock edges; fresh activations retain their normal start edge.

The same component walk now executes regular repeating and explicit-mode one-pass programs, sequential/parallel/delayed/repeated entries, finite nesting, authoritative start/seek/stop, empty children, and primitive/composite interactions. Snapshot intents, explicit actions, inherited playback, direct requests, and natural progression use one precedence model. Program/cleanup/OnPulse ordinal ranges are disjoint; declaration order is irrelevant. Adjacent desired-mode continuations do not restart media at each clock tick or regular pass boundary.

Nested authoritative snapshots carry the originating request for atomic preview refusal. Ordinary OnPulse actions and natural program advancement do not incorrectly claim a coincident request as their transaction origin. Actual automatic resource refusal remains a Stage 4 integration requirement, not something these logical tests pretend to execute.

The contract now explicitly distinguishes iteration entry from actual clock time, and clarifies that stopped loops may relay incoming clocks while a genuinely idle source stalls its followers. This reconciles the separate clock/media rules rather than inventing a nominal fallback timer.

Stopped program replacement resets prepared nominal metadata/cursors and returns the displaced owner. A changed running program is safely refused without state/ownership loss. **Stage 6 still must stage running replacements for the next iteration-zero boundary**; that integrated lifecycle requirement is not marked complete here.

### Progress and storage argument

Preparation bounds node slots, relations, program entries/segments, pending records, decision state, and scratch. The compiler rejects action edges inside strongly connected timing components and recursive containment, but accepts pure-sync components containing primitives or composites. Inter-component action order is acyclic.

At a boundary, each seeded member's delivered bit changes at most once; each pending request consumes its single prepared source edge at most once. Composite target snapshots/differences are streamed once to downstream components. Preview retries publish nothing until committed. Every selected interval is nonempty and advances toward its finite ceiling. Thus dense work is finite and application-dependent, without an arbitrary wave/subrange counter or required callback-sized transition history. Optional journal loss cannot influence arbitration or state.

### Verified gates

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | **53 passed per profile**: 19 compiler, 17 primitive, 17 composite tests; no failures/ignored tests |
| Existing runtime native debug/release | **93 passed per profile** |
| Loop crate Node/Chromium | **53 passed per runtime**; no failures/ignored tests |
| Independent primitive single-sample simulation | 256 generated scripts × 32 requests × 64 frames; three partition/order profiles |
| Independent composite dense simulation | 256 generated regular/explicit programs × 64 frames; full absolute journals, every logical media sample, and iteration/pass/entry-frame state match across three partition/order profiles |
| Compiler references retained | 4096 independent reachability cases and 1024 sparse-versus-dense lookup cases |
| Dense/deep actual execution | 65 nested program levels × 128 one-sample ticks, reversed declarations, `[128]` versus `[1 × 128]`; existing 256-node trigger cascade also passes |
| Indexed extremes | A `u64::MAX`-tick sparse program starts near its end, seeks its child offset, and wraps without overflow or history replay |
| Failure/replacement guards | Atomic descendant preview refusal; stale/removed identities and epochs; stopped replacement; running replacement refusal returns its owner |
| Diagnostic saturation | Nested execution with one retained journal event per boundary preserves complete logical output/state |
| Full prescribed nextest workspace suite | **1946 passed**, four unchanged existing opt-in canaries skipped |
| Formatting, warning-denied build/rustdoc, dependency/test/tracing policies | Passed; both four-target boundary audits and five Python policy tests pass |

The independent composite oracle interprets raw sections into a dense desired-state table and advances all clock/media state one sample at a time; it calls no production compiler, resolver, or sparse lookup. Generated references cover flat regular/explicit programs. Nested seeks, source stalls, valid cyclic sync, conflict priorities, and mixed composite/primitive cascades additionally have explicit expected cases and full-journal partition/declaration comparisons. Integrated randomized resource/media/host cases remain required in later stages.

All Stage 2 checklist items are now verified. This completes the **pure logical layer only**: no generic runtime planner API, real media renderer, resource transaction, Miri extension, actual loop AudioWorklet fixture, production migration, or final-head CI is claimed. The initial runtime and historical evidence remain unchanged.

## Stage 3 — Planning metadata milestone (stage remains in progress)

Milestone: `feat(rt-runtime): validate explicit planner dependencies` (the commit introducing this section). Evidence: [`evidence/graph-stage3.json`](evidence/graph-stage3.json), with the initial workspace failure, investigation, and fresh host reports in [`evidence/graph-stage3-provenance.json`](evidence/graph-stage3-provenance.json). Source hashes identify the tested implementation on top of `e66a00b7124de4b90e201aa6ec01925ffe7787c7`; raw logs are under `target/loop-layer-graph-stage3/`.

### Implemented structure

- Generic `PlannerId` and planner/consumer signatures declare decision slots, observation storage, and payload limits without any loop-domain dependency.
- `Catalog::with_planner` and `compile_planned` handle explicit bindings independently of the signal DAG. Several consumers may share one decision slot; required consumers need exactly one binding. Bindings are sorted by consumer identity, not incidental signal traversal order.
- `PreparedPlan` owns a separate `PlanningDescriptor`. Keeping `PlanDescriptor` signal-only avoids an unnecessary legacy fixture wire-format change. Reusable staging preallocates/touches binding storage and clears it without allocation or deallocation.
- Incremental receiver validation checks identities/generations, complete planner signatures, graph membership, unique/canonical bindings, slot bounds, and aggregate observation reservations. The single planning step is required before signal step zero; backward planning dependencies are invalid rather than simulated with signal edges. Existing signal DAG cycle/order validation remains independent.
- Signal-only catalogs reject unsolicited planning metadata. No-planner compilation keeps its existing path; the existing host journal and golden evidence are unchanged.

Eight new shared `planning_graph` tests cover canonicalization, validation-budget partition invariance, malformed/stale/missing/duplicate bindings, prefix/signature mismatch, aggregate observation storage, absent graph members, empty clock-only metadata, legacy compatibility, and guarded staged reuse/refusal. Validation and staged edits run under allocation/reallocation/deallocation guards; allocating control-side compilation is intentionally outside them.

### Verification and retained failure

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **101 passed per profile**, including eight new planning-metadata tests |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **97 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Full prescribed nextest workspace rerun | **1954 passed**, four unchanged opt-in canaries skipped |
| Warning-denied workspace build/rustdoc, workspace/host formatting | Passed |
| Both four-target boundary audits, five Python boundary tests, shared test/Wasm/tracing policies | Passed |
| Existing native fixture, debug/release offline and two-thread replay | Passed |
| Fresh existing-runtime offline/live non-isolated AudioWorklet | Passed; fixed **1507328-byte** memory, native/browser offline agreement |
| Processor and Rust heap worklet failure canaries | Both detected with expected child exit 1 |

The **first** full workspace attempt had 1953 passes and one failure: unchanged JACK test `external_midi_fanout_respects_each_tracks_passthrough_mute` failed connecting `monitored_dry_midi_send` to `monitored` with error 22. The error message says a source or destination may not belong to an active client. Twenty isolated reruns passed without code changes or a missing-backend allowance; the subsequent unchanged full suite passed. The engine normal/build dependency tree contains neither new library. These facts do **not** establish the failure's cause or claim a fix. The original failure and all repeat log hashes remain in the provenance report; no test, timeout, or gate was weakened.

Fresh host regression evidence retains **2601 offline callbacks / 332928 frames**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, and **34 MIDI events**. The raw journal matches the historical golden byte-for-byte. This proves compatibility with the **existing runtime fixture**, not a new loop AudioWorklet. Package Wasm tests also remain distinct from actual worklet execution.

The current executor still constructs signal-only catalogs. Planner views/observations, typed operations and pending runtime receipts, callback-wide accounting with internal intervals, integrated activation/retirement tests, and selected new Miri cases remain outstanding. **Stage 3 is not complete**, and no end-to-end LL-A criterion is claimed from this structural milestone.

## Stage 3 — Planner views and interval lifecycle milestone (stage remains in progress)

Milestone: `feat(rt-runtime): execute prepared planner intervals` (the commit introducing this section). Evidence: [`evidence/interval-stage3.json`](evidence/interval-stage3.json) and [`evidence/interval-stage3-provenance.json`](evidence/interval-stage3-provenance.json); raw logs: `target/loop-layer-interval-stage3/`. The report hashes tested Rust sources on top of `f26fd217` and records exact commands, profiles, counts, and environment overrides.

### Implemented lifecycle and compatibility

- `Planner`, `NoPlanner`, `PlanContext`, `FinishContext`, `DecisionView`, and `ObservationWriter` implement the production safe-Rust borrow phases. `GraphRuntime`, `Processor`, `PrepareProcessor`, `PreparedNode`, `Command`, and `Control` carry a defaulted planner type; existing ordinary clients retain the no-planner path.
- Preparation checks fixed decision dimensions and associated element-size arithmetic, initializes/touches observation storage, and captures processor planning signatures. Replacement must preserve both ports and planning access. The planner owns its prepared decision/state storage; consumers borrow only their validated immutable slot.
- One callback snapshots ingress/recycles results and resets accounting. Its internal loop delivers due external actions, asks the planner for a positive interval no later than the next action/host end, traverses the signal DAG, and finishes observations. Empty signal graphs can still advance a configured planner. Zero-frame callbacks do nothing; end-boundary actions settle in the next nonempty callback.
- Observation writers stamp producer identity and enforce each producer's declared allowance. Refusal preserves prior observations and another producer's reserved space. Each interval resets only the written length; stale values are not exposed or freed.
- Invalid intervals and changed decision-slice length are explicit render errors before consumers run. Stale planner candidates return their owner while the prior graph/bindings keep rendering. Programming errors retain the existing output-clearing/advanced-outer-clock contract, not rollback.
- External event/result capacity is now `min(scheduled_events, budgets.events)`. `Budgets::subranges` remains source-compatible but is ignored, including zero. This intentional migration removes the old coupling between external admission and musical interval counts. Existing event/result reservations and half-open timing remain intact.
- Traversal/event/MIDI-overflow diagnostics saturate with `counter_overflow`. Configuration no longer refuses otherwise representable storage based only on a worst-case diagnostic work estimate. Frame/identity arithmetic remains checked and semantic.

The new standalone crate guide `src/rust/rt_runtime/docs/planning.md` and `planned_gain` example describe the actual API and remaining limits. Historical initial scheduler evidence remains labeled historical; live scheduling/capacity/integration guides describe the new behavior.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **109 passed per profile**: seven new executor tests and one diagnostic-overflow unit test |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **105 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Full prescribed nextest workspace | **1962 passed**, four unchanged existing opt-in canaries skipped |
| Selected pinned Miri suite | **53 passing executions**, including all eight planning-metadata and seven planner-executor tests; existing lifecycle stress seeds 0/1/2 retained |
| Production-trait lifetime proof | Positive executable compiles/runs; retained decision borrow across planner mutation fails with **E0502** |
| Planned-gain example | Native debug/release checks exact samples, five intervals, and frame-13 parameter execution |
| Formatting, warning-denied workspace build/rustdoc, dependency/test/tracing policies | Passed; both four-target audits and five Python policy tests retained |
| Existing native and actual non-isolated offline/live AudioWorklet fixture | Passed in fresh runs; both failure canaries detected; historical journal unchanged |

The synthetic pipeline binds source and capture-side consumers to the same decision around an ordinary FX node. Full samples and absolute MIDI events match across `[64]`, `[1 × 64]`, irregular partitions, reversed declarations, adaptive intervals, and one-sample intervals. A **128-interval callback with one external event credit and `subranges = 0`** processes all 384 consumer/FX calls under heap guards. Tests exercise per-consumer overflow isolation, stale graph refusal, malformed intervals, callback-end ownership, and zero callbacks.

A prepared test producer enqueues work from inside traversal without allocating or locking. Its requests and returned event credits wait until the following host callback, never the next internal interval; telemetry is also published only once per callback. A separate planner borrows resource I/O on every interval: the 64-byte callback allowance is consumed once, not refreshed 128 times. This verifies preservation of the **current** resource quota, not completion of the required-render accounting change planned for Stage 4.

The first Miri tool invocation timed out after 900 seconds while preparing/executing the synthetic cases with unnecessarily large default media pools. The report retains that partial attempt. Synthetic tests now use smaller prepared media capacities on **all** platforms; interval counts, assertions, and selections are unchanged. Full native/Wasm/host gates and all selected Miri cases subsequently passed. The Miri runner removes stale success reports at startup and retains partial output for per-case timeouts.

Fresh existing-host evidence again has 2601 offline callbacks / 332928 frames, fixed 1507328-byte worklet memory, native/browser sample/MIDI/graph agreement, and a byte-identical historical golden. This is runtime compatibility evidence, **not a loop-library worklet**.

### Outstanding Stage 3 work

Typed domain operation delivery, retained pending runtime receipts, recovery-safe domain cancellation/stop under normal saturation, and their malformed/stale/ownership tests remain unimplemented. `operation_bytes` currently validates signatures only. Planner/consumer resource access still uses the initial shared `ResourceIo` quotas; required-render indexing and capture transactions remain Stage 4. The production loop resolver is not yet attached to this generic lifecycle. No Stage 3 completion or end-to-end LL-A claim follows from this milestone.

## Stage 3 — Complete generic planning and typed delivery

Milestone: `feat(rt-runtime): retain typed receipts with independent recovery` (the commit introducing this section). Evidence: [`evidence/delivery-stage3.json`](evidence/delivery-stage3.json) and [`evidence/delivery-stage3-provenance.json`](evidence/delivery-stage3-provenance.json). The source base is `6028f8f8`; raw logs are under `target/loop-layer-delivery-stage3/`. Earlier Stage 3 reports remain unchanged historical milestones.

### Typed operations, receipts, and recovery

`Planner::Operation` and `Planner::Result` define compact Copy/Send data. `Action::Planner` and `Action::Processor` use distinct generational target identities; ordinary parameter/MIDI actions remain available. Prepared signatures bound payload bytes, scheduler storage checks account for concrete operation/result sizes, and a larger typed-payload stride does not increase MIDI packet limits. `Action::target()` exposes the target kind; `node()` now returns an optional processor identity rather than inventing a node identity for a planner.

Processor operations return immediate typed results. Planner operations receive a borrowed payload/resource/receipt context and return `Complete` or `Pending`. Delivery removes the event from the due queue while retaining its terminal reservation. A token combines runtime-instance scope, slot, and per-lane monotonic sequence; stale, foreign, duplicate, and premature completion attempts are explicit errors. Disjoint payload and receipt borrows allow safe completion during delivery without unchecked pointers or aliasing. Scope allocation is preparation-only; tokens are not portable capabilities between independent Wasm instances.

Execution records distinguish requested, optional delivered, and terminal frames. Delivery lateness does not count legitimate pending time. Generic runtime success is separate from the domain terminal value, including refusal/cancellation/supersession. Queued typed cancellation produces no delivery and prevents subsequent execution; delivered pending cancellation completes the retained token. Ticket cancellation retains its original pre-dequeue cut. Original owning payloads return through command completions; Copy terminal records release their credits only through receipt and callback-boundary recycling.

One additional recovery receipt is independent of all normal pending/unread results. Only classified payload-free, nonstructural planner recovery actions at or before the current frame are admitted. They must finish immediately or in planning at that frame; an unresolved recovery receipt faults before interval execution. Domain adapters must also reserve their own recovery state independently. Per-lane admission sequence checks permit priority recovery to precede older normal dequeue without incorrectly rejecting that normal command. Structural edits remain outside this lane, and finite held validation is not preempted.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **127 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **123 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1980 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **71 passing executions**, including seven receipt-model and eleven typed-executor tests; existing stress seeds retained |
| Production-trait lifetime proof | Positive build/run and deliberate **E0502** refusal still pass with receipt contexts |
| Planned-gain example, debug/release | Exact 32 samples, five intervals, typed processor result at frame 13 |
| Warning-denied workspace build/rustdoc, formatting, boundary/test/Wasm/tracing policies | Passed; both four-target audits and five Python policy tests retained |
| Existing native host debug/release and fresh offline/live AudioWorklet | Passed; both intentional failure canaries detected; unchanged golden journal |

The independent receipt model runs **8192 generated steps** natively and in Wasm (64 explicitly reduced steps under Miri), with seed `0xa36b20472f5881d9`. It models three slots, queued/delivered/terminal/read states, normal/recovery reservations, and a terminal FIFO independently of scheduler internals. Every accepted action/result, requested/delivery/completion frame, cancellation, credit snapshot, and final return to full capacity is checked. Oracle allocation stays outside guarded production calls.

Typed executor tests compare complete samples and terminal records across whole, one-sample, and irregular host partitions. They cover delayed completion, callback ends/zero callbacks, malformed payloads, domain refusal versus runtime error, exact-frame planner/processor/parameter ordering, queued and pending cancellation, stale planner IDs, processor retirement/reinstallation, and owning payload return. Recovery is exercised with every normal receipt pending or unread, including a full domain pending array and completion during planning before the first affected sample. Unsafe/future/payload-bearing recovery is refused; deliberately nonterminating recovery faults explicitly. Runtime `Full` from a typed operation does not masquerade as MIDI overflow.

The prior interval tests continue to verify frozen ingress and credit snapshots, one telemetry publication per callback, shared resource allowances across 128 internal intervals, observation isolation, and stale-plan preservation. Source review confirms command/maintenance and host MIDI recovery allowances are reset only in the outer callback; delivery and receipt writers cannot replenish them. Native/Wasm guards cover success, refusal, cancellation, stale receipts, and owner retention; Miri exercises the corresponding borrow and ownership paths.

### Compatibility and completion boundary

The independent runtime fixture deliberately retains `NoPlanner` and its `RTR1` wire format; no new operation is disguised as a parameter or claimed to cross that legacy wire. Fresh native/browser runs still agree on **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, fixed **1507328-byte** memory, and a byte-identical historical journal. The fixture documentation now explicitly states that boundary.

All Stage 3 checklist items are verified. This completes the **generic coordination layer**, not loop/media integration or any end-to-end LL-A acceptance. Required-render resource accounting/indexes/capture transactions remain Stage 4; the loop adapter, program staging, coherent domain state, and new actual loop AudioWorklet remain later stages. No hard callback deadline, production migration, or final-head CI is claimed.

## Stage 4 — Required resource accounting foundation

Milestone: `feat(rt-runtime): separate required render work from background quotas`, source base `c8414b68`. Evidence: `evidence/accounting-stage4.json` and `evidence/accounting-stage4-provenance.json`; raw gate logs and archived Miri case logs are under `target/loop-layer-accounting-stage4/`.

### Implemented boundary

`ResourceIo::render()` and `ResourceStore::render_io()` lend restricted `RenderIo` access for sequential reads and active single-resource audio/MIDI capture. A private explicit work class selects accounting in the same checked implementation, rather than inflating or temporarily resetting quotas. Required data operations cannot return `Budget` from background copy/validation exhaustion. They still validate shapes, frames, IDs/leases, semantic arithmetic and fixed physical capacity. This capability cannot upload, allocate a lease, change structural capture lifecycle, reclaim, or replenish allowances.

`RenderWork` reports attempted data operations, copied bytes, and charged MIDI validation units; counters saturate and latch overflow. The executor publishes separate render counters and merges overflow into its non-semantic diagnostic flag. Both work classes reset only once per host callback. Direct legacy `ResourceIo` methods keep their quota/refusal behavior, headroom reservations, owning returns, and immutable snapshot semantics. Optional snapshot tails, transfers, validation and reclamation remain budgeted.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **133 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **129 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1986 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **76 passing executions**, including all four required-resource tests and eight planner-execution tests |
| Production-trait lifetime proof | Positive build/run; deliberate retained-borrow mutation rejected with **E0502** |
| Warning-denied workspace build/rustdoc; formatting; boundary, test, tracing and Wasm policies | Passed |
| Planned-gain example debug/release | Passed |
| Existing native debug/release and actual offline/live AudioWorklet | Passed; processor/heap failure canaries detected; historical journal unchanged |

Four new heap-guarded tests cover required audio and MIDI work beyond exhausted background credits, unchanged legacy refusal, separate validation accounting, stale released cursors, immutable partial snapshot tails across continued capture/removal, bounded reclamation back to full pools, invalid shapes/frames, and explicit physical-capacity exhaustion retaining the complete prefix. A unit test verifies saturating diagnostic arithmetic never causes a budget refusal.

The additional synthetic planner test executes **128 one-sample intervals per callback**, across two callbacks. Required reads complete **512 bytes per callback** while legacy reads retain exactly **64 bytes** of allowance and **112 refusals**. Render operation counts remain 128, with no allowance/counter refresh at interval or capability reborrow boundaries. The original legacy-only test remains intact through the shared fixture.

Fresh existing-host replay retains **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**, and fixed **1507328-byte** Wasm memory. Native and browser journals remain byte-identical to the historical golden.

### Remaining work and limits

Only the first Stage 4 checklist item is complete. Reads still use existing sequential cursors; captures are active single-resource appends, not coordinated group transactions. Indexed audio/MIDI positioning, checked new index ownership, prepared lifecycle/lease reservations, exact-frame freeze-to-play, group preflight/commit, and interval failure observations remain required. Existing IDs/cursors still have the documented same-store precondition; this milestone does not add cross-store identity enforcement.

The existing worklet fixture still uses legacy access, so it is regression evidence, not actual-worklet execution of required `RenderIo` or the loop library. Package Node/Chromium tests exercise the new path in Wasm. No loop integration, group atomicity, deadline guarantee, full Stage 4 completion, final CI, or reviewer approval is inferred.

## Stage 4 — Append-maintained page indexes and scoped reads

Milestone: `feat(rt-runtime): index resource pages for scoped positional reads`, source base `effc7ba4`. Evidence: `evidence/index-stage4.json` and `evidence/index-stage4-provenance.json`; logs under `target/loop-layer-index-stage4/`, with the first broad failure preserved in `target/loop-layer-index-stage4-first-run/`.

### Implementation and design choice

Each data-page slot now embeds a balanced append-only index node; each version owns a fixed root/count. Increasing logical page ordinals permit single right-heavy rotations. Metadata storage is O(total prepared pages), avoiding a version-by-page cross product. Lookup and insertion are logarithmic in version pages; existing sequential links remain available. No index scan/build occurs at lease preparation, seek, wrap, capture stop, or frozen-content access. Pages and their index nodes recycle together under existing incremental reclamation.

`read_index(snapshot_id)` creates a Copy handle referring to an existing lease in constant metadata work; it does not acquire ownership. `RenderIo::cursor_at` / `ResourceStore::cursor_at` accept checked byte offsets, including EOF, and create scoped cursors. Index and cursor scopes reject cross-store reuse even when raw slot/generation pairs coincide. Released/reused leases fail generation checks. Legacy raw resource/snapshot IDs still require their same-store precondition; opaque handles are not serialized capabilities between independent Wasm instances.

`IndexWork` counts background/render metadata steps and saturating overflow; the executor exposes it in diagnostics. Background index insertion is bounded overhead of already admitted page materialization, not an additional content scan or work quota. Required lookup/capture indexing consumes no background allowance. Instance-scope allocation is preparation-only and refuses exhaustion. The representation choice and rationale are recorded in `RUNTIME_INTEGRATION.md`.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **140 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **136 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1993 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **81 passing executions**, including all five indexed-resource tests with their full workloads |
| Lifetime proof | Positive build/run and intentional **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/boundary audits, examples | Passed |
| Existing native and actual offline/live worklet regression; intentional failure canaries | Final broad rerun passed; earlier failures retained below |

The flat-byte oracle compares every one of **4097 byte offsets**, then **4096 generated wrap/seek positions** using seed `0x0b29778f3a19fbe5`, identically on native, Wasm, and Miri. Reads respect page/prefix/output limits and at most 21 index visits. Guarded cases cover empty/audio/raw-MIDI data, partial immutable tails across further appends/tree rotations, immediate frozen-content access without index work, old leases after replacement/removal, stale/foreign handles, cancelled partial uploads, slot reuse, and full pool recovery. A separate native/Wasm unit test checks parent links, in-order ordinals, stored heights, AVL balance, and equivalence to the independent sequential page chain after every one of **1024 insertions**. Scope/counter exhaustion never wraps.

The synthetic planner now performs an indexed seek and read at each of 128 one-sample intervals: 256 required operations and 512 copied bytes per callback, while legacy reads still receive only 64 bytes and report 112 refusals. Index counters reset once per callback and remain within the tree-height bound.

### Failed runs and investigation

The initial focused test compile passed a `Frame` to the existing resource-relative `u64` MIDI encoder; the test argument was corrected without changing production API, workload, or assertions. A reconstructed diagnostic note is retained.

The first broad run passed all earlier gates but Chromium disconnected during the actual-worklet check. One isolated unchanged-artifact repeat reproduced `invalid session id`. Driver failures at **17:47:57.044** and **17:50:02.780** coincide with host suspend events. This is temporal evidence, not a proven cause or fix. No power settings or artificial resource constraints were changed. Ten subsequent isolated unchanged-artifact runs passed; the complete broad gate was then rerun successfully with both intentional failure canaries. Failed logs, driver diagnostics, kernel-event evidence and repeat records/hashes are retained. No timeout, flag, assertion, or test selection was weakened.

The failed run also exposed stale success reports from a previous fixture run. The runner now deletes the selected result and browser summaries before starting; the failed isolated repeat confirmed these remained absent. This fixes report hygiene, not the browser disconnect.

### Compatibility and limits

The golden journal remains byte-identical: **2601 offline callbacks / 332928 frames**, **34 MIDI events**, sample/MIDI/graph hashes **480331717 / 3308430490 / 487351092**. Additional preallocated index/handle metadata increases fixture Wasm startup memory from the preceding **1507328** to **1703936 bytes**; it stays fixed throughout offline/live rendering. This is preparation growth, not callback allocation or a memory-growth exemption.

The second Stage 4 checklist item remains incomplete: byte positioning is not MIDI event/time indexing or bounded note/controller restoration. Prepared capture lifecycle/lease reservations, group preflight/commit, and failure observations also remain pending. The existing actual-worklet fixture exercises index maintenance through legacy resource operations, not the new indexed-seek API or a loop-library worklet. No full Stage 4 or end-to-end LL-A completion is claimed.

## Stage 4 — Fixed MIDI checkpoint-state foundation

Milestone: `feat(rt-runtime): add fixed MIDI channel and key state`, source base `bf94631f`. Evidence: `evidence/midi-state-stage4.json` and `evidence/midi-state-stage4-provenance.json`; raw gates and archived Miri logs under `target/loop-layer-midi-state-stage4/`.

`midi_state::MidiState` adds fixed Copy metadata for sixteen channels and 128 keys/controller slots per channel. It distinguishes held and pedal-latched keys, retains latest velocities and supported pressure/program/pitch/controller values, treats velocity-zero note-ons as note-offs, and records reset baselines without inventing unknown device-specific defaults. Repeated note-ons remain separate media packets; the state projection is per key, not per synthesizer voice. No clock, loop, renderer, device, or host dependency is introduced. Packet validation precedes mutation; already validated messages avoid redundant validation. State copying and all updates are heap-free and bounded by protocol shape.

The state/reset policy and MIDI Association references are recorded in `src/rust/rt_runtime/docs/midi-state.md`. It includes sustain/hold/sostenuto interactions, immediate all-sound-off versus pedal-aware all-notes-off, retained bank/program/volume/pan/sound/effects values on controller reset, known reset values, unknown defaults, and system-reset markers. Opaque SysEx/device effects and RPN/NRPN parameter histories are not synthesized by this protocol projection; original packets are not discarded from media. This is a state foundation, not completed capture/checkpoint or restoration behavior.

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **145 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **141 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **1998 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **86 passing executions**, including all five new state tests at full workload |
| Lifetime proof | Positive build/run and intentional **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/audits, examples | Passed |
| Existing native debug/release and actual offline/live worklet; failure canaries | Passed; golden journal unchanged |

The independent ordered-map oracle runs **4096 operations**, seed `0xad473b29385cc291`, with separate pedal/pitch arrays and full channel/key comparisons after every 64 operations. The same workload runs natively, in both Wasm runtimes, and under Miri. Targeted tests cover pedal interactions and velocities, channel isolation, reset/mode behavior, pressures/program/pitch, Copy snapshots, invalid indices, malformed/capacity refusal without mutation, and non-channel packets. All production updates/copies are heap guarded; reference allocation is outside the guard.

The existing host remains a regression fixture, not an execution test of this new state type or a loop AudioWorklet. Offline results retain **2601 callbacks / 332928 frames**, **34 MIDI events**, hashes **480331717 / 3308430490 / 487351092**, fixed **1703936-byte** memory, and the byte-identical golden journal. There were no gate failures in this milestone; earlier index-milestone browser failures remain recorded above, not rewritten.

Stage 4 remains incomplete. Checkpoint storage and its reservations, incrementally built event/state positioning with a declared local replay bound, capture group lifecycle/transactions, and truthful failure observations are still pending. Actual restoration emission belongs to the later renderer work. No additional Stage 4 checklist item or end-to-end LL-A acceptance is marked complete.

## Stage 4 — MIDI event/checkpoint indexing

Milestone: `feat(rt-runtime): index MIDI events and checkpoint state` (the commit introducing this section), source base `df25525231d09023b567c65f65ae763f5914c1b5`. Evidence: [`evidence/midi-index-stage4.json`](evidence/midi-index-stage4.json) and [`evidence/midi-index-stage4-provenance.json`](evidence/midi-index-stage4-provenance.json); raw gates and archived Miri logs are under `target/loop-layer-midi-index-stage4/`.

### Interfaces and preparation

`ResourceStore::with_midi_indexing` opts into fixed `MidiIndexConfig` storage: indexed-version slots, a complete checkpoint row per version, and a positive local event-replay span. Legacy stores allocate no heavy MIDI rows. `begin_indexed_midi_upload` and `begin_indexed_midi_capture` attach an explicit initial protocol state; normal upload/capture lifecycle remains in use. Each version pins its row until the final reference is released. Reuse resets source identity/used length without scanning or clearing checkpoint state; data pages still retire incrementally. This deliberately declared version-by-checkpoint storage tradeoff is documented in [`MIDI_INDEX_DESIGN.md`](MIDI_INDEX_DESIGN.md) and the crate's `docs/midi-indexing.md` guide.

Complete-record summaries incrementally maintain state and checkpoints during uploads/capture. A candidate upload parser and checkpoint capacity are checked before publishing any bytes or state from a chunk. Capture preflights metadata capacity before appending. Every packet counts toward replay spacing, including opaque packets. With C checkpoints and span K, a row supports at most C×K−1 packets; byte capacity can exhaust earlier. Valid content may exceed declared checkpoint capacity before its byte reservation fills, producing an explicit refusal rather than an unbounded scan. This is single-resource atomicity, not a coordinated capture transaction.

`RenderIo::{midi_cursor,midi_next,midi_read}` provides scoped event iteration and complete original packet reads, including legacy validated MIDI content. `midi_seek` requires checkpoints, selects the last checkpoint strictly before the target within the immutable snapshot prefix, and replays fewer than the declared span. It returns projected state before the target and leaves all equal-frame events for playback. Header/short-packet lookup uses the page index; reconstruction skips long opaque bodies without removing them from playback. There is no whole-content build at freeze or position change. Output restoration emission remains a renderer responsibility.

Opaque MIDI cursor/event handles validate store scope, lease/version, packet/prefix bounds and offsets. They refer to an existing lease, not a new owner. Old leased prefixes remain readable after capture appends, replacement, and removal. Raw legacy IDs retain their same-store precondition. `MidiIndexWork` reports indexed/replayed records and initial/checkpoint state copies; saturation is diagnostic, and counters reset once per host callback rather than per interval.

### Verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **155 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **151 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2008 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **95 passing executions**, including eight MIDI-index tests and nine planner-execution tests |
| Production-trait lifetime proof | Positive build/run and deliberate **E0502** refusal |
| Warning-denied build/rustdoc, formatting, policies/audits, examples | Passed |
| Existing native debug/release and actual offline/live worklet; both failure canaries | Passed; historical golden journal unchanged |

The independent seek oracle uses **273 packets**, dense equal-time groups, supplied initial state, and a **257-byte SysEx** spanning pages. It compares positions/original packets and independent ordered-map key/pedal/program state at all **81 target frames** of an 80-frame resource with span 8. Bytewise uploads exercise streaming boundaries. The full workload runs unchanged under Miri. Additional guarded cases cover partial capture snapshots and later appends, no-build freeze, upload/capture checkpoint exhaustion before prefix mutation, malformed-chunk atomicity, foreign/stale leases, old revisions, row reuse and full recovery, empty content, maximum-frame duration, and explicit legacy/unprepared refusal. A unit test separates saturating diagnostics from semantic event-count exhaustion.

The synthetic planner performs 128 MIDI seeks in one callback with span-one checkpoints, checks state around frame 64, and observes exactly 128 checkpoint-state copies with no replay. Required work remains independent of the legacy 64-byte allowance and its 112 refusals. Counters reset at the next callback, not at each interval or resource borrow.

Existing host results retain **2601 offline callbacks / 332928 frames**, **34 MIDI events**, hashes **480331717 / 3308430490 / 487351092**, and fixed **1703936-byte** memory. The actual runtime worklet is regression evidence only: its legacy configuration does not opt into the new MIDI pools and is not the required loop AudioWorklet. Package Wasm tests exercise the new APIs. No failed gate occurred in this broad run; earlier failures remain recorded in their own milestones.

The first two Stage 4 checklist items are now verified. Prepared lifecycle/version/lease tokens, exact-frame reserved freeze-to-play, coordinated audio/MIDI transactions, and truthful integrated failure observations remain incomplete, as do all later loop renderer/host/final-delivery stages. This milestone does not complete Stage 4 or an end-to-end LL-A criterion.

### Carried-forward checkpoint verification

At confirmation of goal `mvo6lnim-1xzkd7`, the Stage 0–3 evidence files were compared byte-for-byte with their introducing commits, all of which are ancestors of the current branch. Recorded Rust hashes matched those milestone commits: 14 for the completed Stage 2 resolver, 50 for planning metadata, 52 for interval execution, and 54 for completed Stage 3 delivery. Current MIDI-index evidence matched all 63 recorded Rust source hashes, all 21 successful gate-log hashes, and all archived Miri case hashes (95 executions). This verifies provenance without presenting historical results as fresh executions or redoing completed implementation. The latest broad MIDI-index suite separately verifies current runtime/loop regressions. The plan and new goal tracker carry Stages 0–3 forward and continue Stage 4; unrelated `.pi/` data remains untracked.

## Stage 4 — Prepared capture lifecycle and staged group transactions

Milestone: `feat(rt-runtime): reserve and stage coherent capture groups` (the commit introducing this section), source base `127290dc`. Evidence: [`evidence/capture-stage4.json`](evidence/capture-stage4.json) and [`evidence/capture-stage4-provenance.json`](evidence/capture-stage4-provenance.json). Raw logs and archived Miri cases are under `target/loop-layer-capture-stage4/`. This is a substantial Stage 4 implementation milestone, not completion of the stage's remaining model/disconnect requirements.

### Interfaces and invariants

- `CaptureGroupConfig` opts into fixed group/member rows and staging regions. `bind_capture_group` validates every member before acquiring exclusive claims. Legacy remove/replace/start/stop/raw append cannot mutate managed resources; existing read leases remain usable. Release requires an idle group and preserves active frozen content until explicitly removed.
- `prepare_capture_take` reserves all version, MIDI-row and final-snapshot slots before changing active content. `CaptureTakeId` carries store/group scope and a checked take sequence. Cancelled or completed takes cannot retarget a subsequent take. Unpublished reserved snapshot slots cannot be consumed by optional snapshots or read as ready leases.
- Exact-frame activation validates the whole group before publishing new revisions. Freeze publishes the already reserved immutable leases without a mutable-tail copy, history scan, or new capacity acquisition. All members share the cut/incomplete reason. Empty freeze is a generic storage operation; empty record-to-play refusal remains a domain rule.
- `CaptureIntervalId` adds checked interval identity. Preflight reserves exact audio bytes and conservative MIDI producer/event/payload/checkpoint bounds for every member before any data is staged. Failed preflight leaves all published prefixes unchanged. Unused reservations return on commit/cancel.
- Consumers copy validated contributions into prepared staging. Missing/duplicate/malformed/over-bound members do not partially publish resource data. One exclusive commit updates all actual bytes, indexes, revisions and the common duration; no graph reader can interleave with intermediate publication or index rotations. Producer-bound violations after successful reservation are interface faults, not normal retroactive exhaustion observations.
- Safe borrowing reuses the existing page append routine via a fixed 64-byte stack chunk rather than moving owning storage or duplicating page/index invariants. Three media-data copies are counted (input→staging→stack→pages); checkpoint copies are reported separately. No zero-copy or performance improvement is claimed.
- Planner contexts now use `PlannerResourceIo`, dereferencing to existing resource methods and adding `CaptureBoundaryIo`. The runtime stamps frame/ceiling authority: activation/freeze at the current boundary, reservation within the current planning window, commit at the reserved end. Ordinary `ResourceIo` cannot acquire this capability; `RenderIo` adds only staged audio/MIDI writes. No capability can reset accounting or reclaim storage.

The design and public guide are `CAPTURE_TRANSACTIONS_DESIGN.md` and `src/rust/rt_runtime/docs/capture-groups.md`. Source remains safe Rust, with no new host/application dependency or callback-side owning allocation.

### Verified gates

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **167 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **163 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2020 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **107 passing executions**, including all twelve new capture-group/boundary/planner cases |
| Lifetime proof | Positive build/run and deliberate **E0502** refusal with the new planner resource context |
| Capture authority proof | Planner capability compiles/runs; a consumer `ResourceIo::capture()` attempt fails with **E0599** |
| Warning-denied build/rustdoc, formatting, boundary/policy checks, planned-gain debug/release | Passed |
| Existing native debug/release and actual offline/live worklet; processor and heap canaries | Passed; fixed **1703936-byte** memory and unchanged historical journal |

New tests cover atomic claims, earlier upload refusal, all unrelated version/lease/page-credit saturation after successful preparation, zero remaining background copy allowance, complete cancellation/reuse recovery over 32 cycles, foreign/stale groups and take/interval tokens, occupied freeze outputs, maximum-frame exhaustion, and explicit opt-in/shape limits. Immutable audio/MIDI prefix leases survive staging, later appends, and page-index rotations.

Independent scalar content checks compare complete stereo bytes and original MIDI timestamps/packets across `[8,8]`, sixteen one-frame intervals, and `[3,2,5,6]`; bounded checkpoint seeks agree with the expected last-note state. Conservative checkpoint exhaustion retains the exact 24-frame common prefix. Partial/invalid staging remains unpublished and cancellable; all ordinary test teardowns restore page/version/snapshot/MIDI-row credits.

The actual generic-runtime synthetic planner uses unknown downstream MIDI within its declared bound. At frame **24**, preflight exhaustion changes the dependent output **before sample 24**, freezes the common prefix, and leaves unrelated audio running. Full outputs agree across `[32]`, thirty-two one-frame callbacks, and `[7,9,5,11]`. Reserved interval staging is committed in `finish`; exported prefix/state and eventual credit recovery are checked. This is a generic observation-boundary proof, not loop-domain adaptation or a new loop AudioWorklet.

### Retained development failures and remaining work

Three initial focused attempts failed due to test setup: a missing `unwrap()` on `MidiBuffer::new`, a transfer allowance below the existing minimum page size, and insufficient declared staging space for the test's two MIDI members. The fixes supplied the correct buffer type, increased allowance and deliberate consumption together to 16 bytes (still leaving zero background credit), and provided the 448-byte staging shape needed to reach the intended checkpoint-row refusal. Workloads/assertions were not removed or weakened; original logs and hashes remain in provenance. The later complete broad run passed without retrying a failed gate.

The existing worklet fixture remains legacy/no-planner regression coverage and does not opt into capture groups. New APIs execute in the package Wasm tests and synthetic native/Wasm planner, not yet in the required independent loop host. Complete generated group-state/ownership and disconnect-at-each-phase coverage, Stage 4 acceptance review, loop renderers/restoration, final-head CI and review remain pending. The remaining Stage 4 checklist items stay open until that verification is complete; successful core APIs alone do not close them.

## Stage 4 complete — Generated capture ownership and lifecycle-cut disconnects

Milestone: `test(rt-runtime): verify capture ownership and disconnect cuts` (the commit introducing this section), based on core transaction commit `9aef981c`. Evidence: [`evidence/capture-ownership-stage4.json`](evidence/capture-ownership-stage4.json) and [`evidence/capture-ownership-stage4-provenance.json`](evidence/capture-ownership-stage4-provenance.json). Raw logs and archived Miri cases are under `target/loop-layer-capture-ownership-stage4/`. Earlier milestone evidence and failures remain unchanged.

### Independent ownership/content model

`capture_group_model` runs **1,024 actions each with 64 and 16 physical pages**, seed **`0x7a49c83d502e19b6`**. The complete 2,048-action workload is unchanged on native, Wasm, and Miri. An independent reference ledger tracks prepared/current/pinned versions, mutable-tail private pages, conservative interval-page reservations, reserved final leases, MIDI rows, revisions, capture metadata and currentness. It predicts admission/refusal before calling the store and checks all pool counters and visible metadata after each action. Retirement is drained through bounded maintenance calls rather than treated as immediate callback work.

Original stereo samples and MIDI packets/timestamps are generated independently of the storage encoding. Every 32 actions, all retained snapshots are compared against their original prefixes, including state strictly before each seek frame. Reads use guarded required I/O. Repeated old leases, new takes, partial staging, cancellation and slot reuse must not change those immutable contents. The final teardown returns every page, version, snapshot and MIDI row.

| Action | 64-page model | 16-page model |
| --- | ---: | ---: |
| Snapshot release / acquisition | 115 / 82 | 112 / 73 |
| Take cancellation / activation | 6 / 20 | 9 / 23 |
| Interval cancellation | 43 | 17 |
| Audio / MIDI staging | 95 / 79 | 48 / 39 |
| Commit / freeze | 68 / 20 | 36 / 23 |
| Interval reservation / refusal | 111 / 0 | 53 / 10 |
| Take preparation / refusal | 26 / 310 | 32 / 504 |

The 64-page run does not claim physical interval exhaustion; the additional 16-page run supplies that coverage. Conservative checkpoint exhaustion remains covered by the existing exact-24-frame tests. Neither generator nor assertions are reduced under Miri.

### Disconnect and quiescent teardown

`capture_group_disconnect` exercises fourteen stable lifecycle cuts: unbound, bound, prepared, activated, interval-reserved, audio-staged, all-staged, committed, frozen, first/final lease released, group released, prepared take cancelled, and interval cancelled. It queues a payload-free classified recovery stop, **drops the actual control transport before delivery**, then processes the stop with no callback allocation/reallocation/deallocation. Pending intervals are cancelled before publication; prepared takes are cancelled; committed prefixes freeze at the runtime's actual boundary. Every reserved lease/page credit settles even though the receiver of completion results has disconnected.

This verifies queued recovery plus disconnect, not automatic stop merely because a producer disappears. Group release/resource removal remain explicit owner cleanup rather than structural work smuggled into recovery. A prepared unbound processor is retained during callbacks and destroyed exactly once off-render. New `GraphRuntime::into_resources()` consumes a stopped runtime off-render and returns its store; other owners are destroyed. It does not discover or cancel domain handles lost by a planner: settle those first or retain handles. Explicit cleanup of the returned store restores all pools and group slots at every cut.

### Final Stage 4 gates and checklist review

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **169 passed per profile** |
| Loop crate native debug/release | **53 passed per profile** |
| Runtime Node/Chromium | **165 passed per runtime** |
| Loop crate Node/Chromium | **53 passed per runtime** |
| Prescribed full nextest workspace | **2022 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **109 passing executions**, including full generated model and all disconnect cuts |
| Lifetime / authority proofs | Positive compilation/execution; deliberate **E0502 / E0599** refusals |
| Formatting, warning-denied build/rustdoc, examples, boundary and test/tracing/Wasm policies | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; unchanged historical journal and fixed **1703936-byte** memory |

All 21 broad gates passed on the first complete run. The preceding focused model/disconnect runs also passed; there is no new failed run being omitted. The ignored Nix SQLite-busy warning did not prevent the Miri run from succeeding.

| Stage 4 checklist obligation | Evidence |
| --- | --- |
| Required work versus background quotas, one callback envelope | `accounting-stage4`; repeated-interval planner tests; independent saturating diagnostics |
| Prepared audio/MIDI indexes, bounded positioning and scoped identities | `index-stage4`, `midi-state-stage4`, `midi-index-stage4`; unchanged full page/event/state oracles and Miri |
| Reserved lifecycle/version/MIDI-row/final-lease capacity; exact activation/freeze and immediate frozen access | `capture-stage4`; lifecycle saturation, stale tokens, empty/nonempty freeze/read, authority proofs |
| Coordinated audio/MIDI preflight/staging/commit; unknown downstream MIDI bounds and common-prefix failure | `capture-stage4`; three content partitions, malformed/partial staging, conservative checkpoint limits; new independent 16-page model |
| Failure observations before affected samples, no rollback | `capture_group_planner`: dependent sample changes at frame **24** under three host partitions, while unrelated audio continues |
| Immutable leases/currentness/old revisions, tail isolation, incremental retirement, cancellation/disconnect/full recovery | Prior index/group cases plus the 2,048-action independent model and fourteen real transport disconnect cuts |

All Stage 4 checklist items are verified. This closes the **generic resource layer**, not loop renderers/restoration, loop-domain control/adaptation, actual loop AudioWorklet execution, final-head CI, or reviewer approval. Stage 5 is current. No hard callback deadline or performance improvement is inferred from these correctness gates.

## Stage 5 — Prepared audio rendering and capture foundation

Milestone: `feat(loop-engine): add prepared audio rendering and capture` (the commit introducing this section), source base `37d13073`. Evidence: [`evidence/audio-stage5.json`](evidence/audio-stage5.json) and [`evidence/audio-stage5-provenance.json`](evidence/audio-stage5-provenance.json); raw logs and archived Miri cases are under `target/loop-layer-audio-stage5/`. This is a partial Stage 5 milestone, not completion of its integrated checklist.

### Interfaces and evidence

- `shoop_loop_engine::audio::{AudioRenderer, AudioCapture, Playback, Smoothing}` allocate/check/pre-touch fixed conversion and per-channel DSP storage at preparation. All channel/frame/aggregate-byte arithmetic uses the common native/Wasm ceiling. The renderer does not own mode, playback position, wrap occurrence, resource leases, or capture lifecycle authority.
- Playback receives a scoped index, authoritative source position, finite gain and explicit discontinuity. The new generic `RenderIo::snapshot(index)` validates store/lease identity and returns pinned-prefix metadata without creating a lease or spending background transfer credits. Format/duration and complete output shape are checked before output/DSP mutation; little-endian interleaved data is read into fixed scratch and converted to planar output. Source intervals cannot cross wraps.
- `None` source is immediate silence and resets smoothing. A zero-frame call has no DSP effect. Default/zero-length smoothing is exact raw gain. Linear restart retains the last actual output as its anchor, advances its ramp only for rendered samples, survives host cuts, and restarts on start/retrigger/seek/wrap/replacement. Refusal leaves output and DSP state unchanged, but does not authorize replaying a failed runtime callback.
- Capture converts planar inputs into fixed interleaved scratch and stages one already reserved group member. It cannot activate/freeze/commit. Invalid shapes, duplicate contributions and stale interval tokens do not publish a prefix. Runtime work counters cover resource storage access, not all conversion/DSP instructions or a deadline guarantee.

Six guarded loop-crate tests establish:

1. A separately implemented scalar source-position/sample/ramp oracle agrees with the production resolver plus audio renderer over **40 frames**, gain 2, smoothing 0/4, start/seek/stop/restart/replacement and natural wraps. Host partitions are `[40]`, forty one-frame callbacks, and `[7,6,4,9,14]`. Required I/O continues after actual public legacy reads exhaust the callback's background allowance; it never refreshes it.
2. Mono/stereo/**five-channel** interleaved regions cross 16-byte pages correctly, with negative gain. Retained leases remain playable after resource removal; metadata remains that of the pinned prefix.
3. Stale/foreign scopes, wrong format, empty content, out-of-range/maximum positions and non-finite gain leave prior output/ramp state intact. Zero frames do not reset a pending ramp; Stop is immediate silence; invalid preparation arithmetic is refused.
4. Planar capture over `[5]`, five one-frame intervals and `[2,3]` freezes and reads immediately. Separate old-playback/gain/new-capture operations double the new take while preserving the immutable old take, without background credit. This is an operation-level proof, not yet the required separate-node runtime graph.
5. **128 one-sample intervals** produce every sample and exactly **127 interior wraps** under `[64,64]`, 128 one-frame callbacks and `[7,53,19,49]`. The wrap at 128 remains pending for the next nonempty interval; the renderer does not invent a clock or impose a subdivision quota.
6. Missing/mismatched/oversized/zero-frame capture inputs, wrong reserved duration, duplicate staging and stale tokens retain an unpublished empty prefix. Cancellation and explicit teardown return all credits.

All six workloads run unchanged in native debug/release, Node/Chromium and pinned Miri. The Miri runner now records the package for each case and includes the loop audio target; it does not claim that the full loop test suite has run under Miri.

### Verified gates and retained failures

| Gate | Observed result |
| --- | --- |
| Loop crate native debug/release | **59 passed per profile** |
| Runtime native debug/release | **169 passed per profile** |
| Loop crate Node/Chromium | **59 passed per runtime** |
| Runtime Node/Chromium | **165 passed per runtime** |
| Prescribed full nextest workspace | **2028 passed**, four unchanged opt-in canaries skipped |
| Selected pinned Miri | **115 passing executions**: 109 runtime plus six complete loop audio cases |
| Lifetime/authority proofs | Positive builds/runs; deliberate **E0502/E0599** refusals |
| Warning-denied build/rustdoc, formatting, examples, boundary/policy gates | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; unchanged historical journal and fixed **1703936-byte** memory |

Three focused development failures are retained with hashes: using the store method name on `RenderIo` instead of its existing `stage_audio` API; ambiguous `Request` imports/private budget charging in the test (replaced with explicit import and real public legacy reads); and incorrectly treating the resolver's interval journal as cumulative. The last fix accumulates each committed interval's events and preserves the **127-wrap** assertion. No musical workload/assertion was reduced. All 21 subsequent broad gates passed in their first complete run; the ignored Nix SQLite-busy warning did not prevent success.

The user-facing API contract is `src/rust/shoop_loop_engine/docs/audio.md`. The existing runtime worklet remains regression coverage and does not execute the new loop renderer; package Wasm tests do. MIDI playback/capture/restoration, planner/processor adapters, transactional loop/resource binding, coherent mixed-media export, the full separate-node playback→FX→recording graph, the public executable lifecycle example, and independent loop hosts remain pending. Stage 5 stays current; no end-to-end LL-A or final-delivery completion is claimed.

## Stage 5 — Authoritative media discontinuity decisions

Milestone: `feat(loop-engine): publish authoritative media discontinuities` (the commit introducing this section), base `287de2c2`. Evidence: [`evidence/decisions-stage5.json`](evidence/decisions-stage5.json) and [`evidence/decisions-stage5-provenance.json`](evidence/decisions-stage5-provenance.json); logs are under `target/loop-layer-decisions-stage5/`.

The audio foundation exposed a necessary integration distinction: the diagnostic journal may be absent/full, and a successful same-position Seek or identical Replace may not change visible `State`. Neither journal entries nor state inequality can therefore be the source of renderer restart semantics. This milestone implements the already specified discontinuity behavior without changing pulse precedence, request outcomes or timeline authority.

`Decision::playback_discontinuity` is accumulated in prepared per-loop shadow scratch for successful primitive transitions and natural wraps, then gated by the final state's `plays()`. Start/retrigger/seek/replace and newly starting monitored playback set it; idempotent Play, continuing monitored playback and monitored-capture failure do not. Final Stop/non-playing state clears the published flag. Composite operations propagate through ordinary primitive decisions rather than pretending composites render media. Checked storage accounting includes the new scratch byte and decision layout.

Every preview clears/rebuilds these flags before arbitration. Repeated previews are identical; refused/losing/cancelled work cannot leak an earlier candidate restart. Flags are committed with the immutable interval decision and do not depend on external receipt or diagnostic capacity. `SEMANTICS.md`, the crate README and audio guide document the interface; the actual runtime adapter remains pending.

### Fresh scoped verification

- The existing scalar sample/ramp and 128 one-sample audio cases now consume authoritative flags with **journal capacities 0, 1 and 1024**, retaining all prior host partitions and fully provisioned cases. Output is unchanged even with absent diagnostics; every one-sample interval has the required restart flag, while the final wrap at callback end remains pending.
- Four new guarded `media_decisions` tests cover a twenty-frame primitive expected-flag table, same-state Seek/Replace with no transition journal entry, idempotent Play, retrigger, capture monitoring/failure, Stop/empty-content refusal, repeated previews, losing/cancelled/refused intents, recovery of a later natural wrap, reactive final Stop/restart arbitration, and nested authoritative snapshots without a journal.
- **63 loop tests per native debug/release profile; 63 per Node/Chromium runtime.** Fresh prescribed workspace nextest: **2032 passed**, four unchanged opt-in canaries skipped.
- **Ten fresh selected loop Miri executions** (six expanded audio tests and four media-decision tests), with unchanged full workloads. The runner now supports `--package` and keeps filtered reports in separate package directories; default combined execution still selects both packages. No claim that a combined 119-case run occurred in this milestone.
- All **15 scoped gates** passed: formatting, loop native debug/release, warning-denied workspace build/rustdoc, runtime/loop boundary checks and helper tests, test/tracing/Wasm dependency policies, Node/Chromium, pinned nextest version and full workspace tests. The focused tests passed on their first run. The ignored Nix SQLite-busy warning did not prevent success.

No `rt_runtime` implementation changed here. Standalone runtime release/Wasm tests, native/actual-worklet fixtures, lifetime/authority prototypes and generic runtime Miri were **not freshly rerun** for this scoped milestone; their preceding full `audio-stage5` evidence remains historical. Future integrated/final gates still require their prescribed fresh runs. Stage 5 remains current: MIDI emission/restoration, planner/processor adapters, mixed-media capture/export, the separate-node graph, public full-lifecycle example and independent loop hosts remain required.

## Stage 5 — Projected MIDI cleanup/restoration emission

Milestone: `feat(loop-engine): emit ordered MIDI cleanup and restoration` (the commit introducing this section), base `c3526ff1`. Evidence: [`evidence/midi-restore-stage5.json`](evidence/midi-restore-stage5.json) and [`evidence/midi-restore-stage5-provenance.json`](evidence/midi-restore-stage5-provenance.json). Logs and archived Miri cases are under `target/loop-layer-midi-restore-stage5/`. Stage 5 remains incomplete.

### Ordering refinements and interfaces

The first emission implementation exposed information that latest-value controller arrays alone cannot express. A later bank-select packet may be pending for a future program change; using it before restoring the current program selects the wrong bank. Likewise both RPN and NRPN selector arrays may contain values while only the last family governs subsequent Data Entry, and opposed omni/mono modes must not be replayed as if all were current. These are representation refinements needed for the existing restoration contract, not a new timeline or device-state guarantee.

The generic `MidiState` now retains the bank selectors observed at program change, a selector-family ordering hint, and an explicit untracked-parameter-history marker. Only the current member of each CC124/125 and CC126/127 pair remains cached. CC96/97 are momentary relative commands rather than persistent values. CC121 retains loaded/pending banks and untracked parameter history while establishing null selector defaults; System Reset clears the projection. Original recorded packets are unchanged, including parameter and opaque SysEx packets.

`shoop_loop_engine::midi_restore::{cleanup, restore}` borrow state and emit complete packets through a bounded sink, with no allocation or state mutation. Cleanup releases active holds and projected keys rather than indiscriminately issuing normal-operation all-sound-off. Restoration orders reset markers, loaded bank/program, pending banks, controls/modes, selector state, pitch/pressure, holds, correctly latched sostenuto notes, remaining notes, released pedal-held keys, and poly pressure. Data Entry values are emitted only while selectors are temporarily null; known selectors are then restored with the last family last. No relative parameter command is synthesized.

`Emission` exposes attempted/accepted packet counts, first error and untracked parameter history. After a sink refusal it completes its fixed traversal/counting without further sink calls or retries. A prefix is not success: the eventual processor must propagate Full through the runtime's reserved reset path after other required work. The helper itself does not deliver recovery. Conservative protocol bounds are **2096 cleanup events** and **8449 restoration events**, with three bytes/event sufficient; these are capacity bounds, not musical-work quotas.

The existing projection does not reconstruct synthesizer voices/envelopes, vendor SysEx or RPN/NRPN parameter-value histories. The new warning makes parameter history explicit; temporary selector nulling prevents accidental writes rather than pretending to recover missing history. Protective selector nulling can affect previously unknown components; only known values are restored afterwards, and unknown components may remain null. This is not a claim that unknown state was reconstructed. Detailed API/ordering/failure contracts are in `src/rust/shoop_loop_engine/docs/midi-restore.md` and the runtime MIDI-state guide.

### Verified behavior and gates

Two new runtime tests cover program-bank snapshots versus pending selectors, selector family ordering, relative-command treatment, opposed mode replacement, reset retention and immutable copied state. Four loop tests cover:

- Cleanup and sustain/hold/sostenuto restoration against an **independent key/pedal machine**. Only the recorded subset is latched; released pedal-held keys are not turned into held keys.
- Loaded versus pending program bank and RPN/NRPN ordering against an **independent selector/bank machine**, including zero accidental parameter writes, correct reset ordering and omission of obsolete/relative commands.
- Every channel/key/control/pressure path under the conservative fixed bounds, with known-state comparisons; the maximal cleanup emits exactly 2096 packets.
- Zero/tiny-capacity and disconnected sinks, exact accepted/attempted counts, no retry after first failure, unchanged source state, and no false claim that the helper already invoked recovery.

All complete workloads execute under heap guards, native debug/release, Node/Chromium and pinned Miri. The existing indexed-state, generated capture ownership and expanded journal-independent audio cases also rerun with the updated representation.

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **171 passed per profile** |
| Loop native debug/release | **67 passed per profile** |
| Runtime Node/Chromium | **167 passed per runtime** |
| Loop Node/Chromium | **67 passed per runtime** |
| Prescribed workspace nextest | **2038 passed**, four unchanged opt-in canaries skipped |
| Fresh combined selected Miri | **125 executions**: 111 runtime plus fourteen loop cases |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Formatting, warning-denied build/rustdoc, examples, boundary/policy checks | Passed |
| Existing native debug/release and actual offline/live worklet; both failure canaries | Passed; historical golden journal unchanged |

The focused tests and all 21 broad gates passed on their first runs; no new failed run is being omitted. This full run supersedes the preceding scoped milestone's lack of fresh generic-runtime/host evidence, without rewriting its historical claims.

These helpers do not yet implement indexed MIDI event playback/rebasing, input tracking/capture, integration with runtime reset delivery, the loop planner/processor facade, mixed-media export, the separate-node graph/example or the independent loop AudioWorklet. Those remained required Stage 5/later work at this milestone. No full Stage 5, external device-state restoration, deadline, final CI or review approval is claimed.

## Stage 5 — Indexed MIDI playback, input tracking and runtime recovery

Milestone: `feat(loop-engine): render and capture indexed MIDI intervals` (the commit introducing this section), source base `f9f041ea`. Evidence: [`evidence/midi-stage5.json`](evidence/midi-stage5.json) and [`evidence/midi-stage5-provenance.json`](evidence/midi-stage5-provenance.json). Logs and twenty archived selected loop Miri executions are under `target/loop-layer-midi-stage5/`. This is a partial Stage 5 milestone.

`midi::{MidiRenderer, MidiCapture, Playback, RenderReport}` now provide:

- Prepared, pre-touched packet scratch and boxed channel projections. Positions/discontinuities remain supplied by the resolver; a scoped cursor and lookahead only validate contiguous media use. Source ownership stays with the caller.
- Cleanup of the previous projection, bounded indexed state strictly before the supplied offset, ordered restoration, then complete original events with stable equal-time ordering and interval-relative offsets. Half-open intervals retain end-boundary events; explicit duration preserves trailing silence.
- Logical media advancement through every original event despite output refusal. Reports distinguish attempted/accepted packets and first failure, with no retry or partial-delivery success claim. The processor must finish other required work and propagate output errors to runtime recovery.
- Continuous input tracking while stopped, capture-start initial state, original-packet staging into an already-reserved group, and recovery-form input independent of ordinary input limits. Invalid input leaves tracking unchanged; staging faults do not retroactively undo valid observed input.

Six guarded tests verify:

1. An independent absolute packet oracle over whole/single/irregular partitions: initial held note, 257-byte SysEx crossing pages, equal-time note/pressure records, original release velocities and note-on-zero, bank/program changes, trailing silence and one-shot stop cleanup.
2. Exact note ordering at seeks, wraps and replacement: cleanup, restoration of state before the target, then original events at that same offset. The replacement has a distinct initial held note and a release at offset zero.
3. Input observed while stopped supplies a captured take's initial state. Frozen state/packets are readable immediately, with the interval's note-off excluded from the initial state but present at offset zero; staging publishes nothing before commit.
4. Position overflow, discontinuity/continuity mismatch, stale lease, malformed interval bounds, capacity and preparation refusals; zero frames preserve renderer/tracker state.
5. A real `GraphRuntime` producer→consumer graph: two original note-ons advance despite output overflow; reserved reset packets clear a seeded downstream held note even with only one ordinary event/three ordinary bytes admitted by the tracker; audio remains exactly 0.25; all 32 host reset packets drain. This is an operation/runtime integration proof, not the full loop facade or an independent loop host.
6. **128 one-sample intervals** driven by the production resolver with journal capacity zero, compared against an independent complete-packet oracle over three host partitions. There is no mandatory timing quantum or journal dependency.

| Fresh scoped gate | Observed result |
| --- | --- |
| Loop native debug/release | **73 passed per profile** |
| Loop Node/Chromium | **73 passed per runtime** |
| Prescribed workspace nextest | **2044 passed**, four unchanged opt-in canaries skipped |
| Selected loop Miri | **20 executions**: audio6, media-decisions4, restoration4, indexed-MIDI6 |
| Formatting, warning-denied workspace build/rustdoc, boundary/test/tracing/Wasm-dependency policies | Passed; all **15 scoped gates** succeeded |

The first focused native run aborted with a stack overflow while processors contained large inline MIDI projections. The revised representation puts those projections in fixed prepared boxes, reducing their footprint in generic processor/command types without raising stack limits, reducing workloads or weakening heap guards. The exact overflowing stack frame was not captured, and the pre-fix source tree was not separately archived; the failure log and change rationale are retained rather than presenting it as a passing run. The subsequent focused run passed; the final complete-packet workload passed in all scoped configurations above.

No generic runtime implementation changed. Standalone runtime release/Wasm, generic Miri, native/actual-worklet fixtures and lifetime/authority prototypes were **not freshly rerun** here; the preceding full `midi-restore-stage5` milestone remains their historical baseline. Twenty fresh loop Miri executions are not a fresh 131-case combined run. The full loop planner/processor adapter, coherent mixed-media binding/capture/export and public outcomes, separate-node playback→FX→recording graph/example, and independent loop AudioWorklet remain required. Stage 5 and final delivery remain open.

## Stage 5 — Reserve a new take's first interval before activation

Milestone: `feat(rt-runtime): reserve capture intervals before activation` (the commit introducing this section), source base `23a1f799`. Evidence: [`evidence/prepared-interval-stage5.json`](evidence/prepared-interval-stage5.json) and [`evidence/prepared-interval-stage5-provenance.json`](evidence/prepared-interval-stage5-provenance.json). Logs and archived Miri cases are under `target/loop-layer-prepared-interval-stage5/`.

Facade integration exposed an ordering gap: the original interval reservation API required an active take, so discovering an initial append refusal could follow publication of new active revisions. `reserve_prepared_capture_interval` and planner `reserve_prepared` now use the existing fixed pools to reserve all first-interval page/checkpoint capacity without publication. `PreparedReserved` rejects staging/commit, preserves current content and revisions, and preflights both activation and append revision increments. Exact-frame activation retains the same interval token/credits and transitions to `Reserved`; cancellation returns to `Prepared` without losing lifecycle reservations. The legacy activate-then-reserve path remains available. Preparing a new take clears obsolete previous-take frame metadata.

Three new guarded tests verify:

- Old indexed audio/MIDI revisions remain current through reservation and rejected staging/commit/wrong-frame activation. Exact activation then staging/commit/freeze produces independent expected bytes, initial MIDI state, packet state, duration and revisions; old leases remain unchanged. Unused conservative reservation credit and every pool recover.
- **32 alternating two-group rounds** with a five-page store and an independent three-page-per-interval byte ledger. Competition refuses atomically; cancellation makes capacity available to the other group; stale tokens cannot cancel replacements; every active resource revision stays unchanged.
- Foreign scope, frame overflow, zero/oversized intervals, planner window and wrong activation frame, plus insufficient MIDI-checkpoint capacity, all refuse without publishing active media or losing credits.

Real transport disconnect coverage grows from fourteen to **seventeen cuts**, retaining the original workload and adding prepared-first-interval reserved/cancelled/activated states. Recovery is queued before disconnect, then delivered to cancel/settle owners; this still does not claim automatic musical stop on disconnect.

| Fresh gate | Observed result |
| --- | --- |
| Runtime native debug/release | **174 passed per profile** |
| Loop native debug/release | **73 passed per profile** |
| Runtime Node/Chromium | **170 passed per runtime** |
| Loop Node/Chromium | **73 passed per runtime** |
| Prescribed workspace nextest | **2047 passed**, four unchanged opt-in canaries skipped |
| Combined selected Miri | **134 fresh executions**: 114 runtime plus twenty loop cases |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Format, warning-denied build/docs, examples and policy/boundary gates | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; historical golden journal unchanged |

The focused run and all 21 broad gates passed without a new failed run. The three new cases and expanded disconnect workload are unchanged under native/Wasm/Miri. This fresh full run supersedes the preceding scoped milestone's lack of fresh runtime/host gates without rewriting that historical scope. The full loop facade, coherent mixed-media publication/export, separate-node graph/example, independent loop hosts, final CI and review remain pending. Stage 5 remains current.

## Stage 5 — Autonomous capture preflight decisions

Milestone: `feat(loop-engine): refuse autonomous capture starts transactionally` (the commit introducing this section), source base `813f8311`. Evidence: [`evidence/capture-decisions-stage5.json`](evidence/capture-decisions-stage5.json) and [`evidence/capture-decisions-stage5-provenance.json`](evidence/capture-decisions-stage5-provenance.json); logs and archived Miri executions are under `target/loop-layer-capture-decisions-stage5/`.

`Observation::CaptureUnavailable { target, reason }` adds a primitive-target refusal for capture starts/restarts, including autonomous trigger/composite work that has no request ID. It preserves the previous mode/content/capture revision and emits no capture-start pulse. Running-take append exhaustion remains a separate input; both classes can coexist at a boundary. Request-family refusal remains distinct and takes precedence regardless of observation order. Target-only refusal intentionally does not reject a parent composite or siblings. The checked observation bound is now two failure classes per loop plus retained request refusals; no new musical-work quota or external receipt is introduced.

Four guarded tests verify:

- Autonomous capture refusal preserves ongoing playback, suppresses the rejected start's descendant pulse, and does not appropriate a coincident unrelated request.
- **128 one-sample intervals**, all four external request slots occupied/unread, three host partitions and journal capacities **0/1/1024**. Repeated previews preserve states/flags; an independent arithmetic refusal-frame oracle and full retained journals agree across partitions.
- Restart refusal alone continues the old recording; simultaneous running-take exhaustion preserves monitored playback and the old prefix. Failed starts do not consume logical capture revisions. Stop remains usable. Both per-loop failure classes fit even with only one external request slot; request-family error precedence is independent of observation order.
- Whole composite request-family refusal versus targeted child capture refusal, plus stale/composite/duplicate observation rejection.

Fresh scoped gates: **77 loop tests per native profile and per Node/Chromium runtime**, **2051 workspace passes/four existing skips**, and **24 selected loop Miri executions** (audio6, media-decisions4, capture-decisions4, restoration4, indexed-MIDI6). All fifteen scoped gates, including warning-denied workspace build/docs, formatting and policy/boundary checks, passed. Complete new workloads run unchanged under native/Wasm/Miri.

The initial test fixture used integer `Entry.duration` values rather than `Option<u64>`, producing **E0308**; changing them to `Some(2)` corrected the fixture without reducing workload/assertions. Its failed compile log is retained. No generic runtime implementation changed here; standalone runtime release/Wasm, generic Miri, lifetime/authority prototypes and native/actual-worklet fixtures were not freshly rerun. The preceding `prepared-interval-stage5` full run remains their historical baseline. Twenty-four fresh loop Miri executions are not a fresh 138-case combined run. Complete resource-to-resolver wiring, coordinated mixed-media ownership/export, separate-node graph/example and independent loop hosts remain required; Stage 5 stays open.

## Stage 5 — Initial graph, retained owner ledger and direct audio ports

Milestone: `feat(runtime): prepare initial graphs and scoped audio port access` (the commit introducing this section), source base `48ee8de2`. Evidence: [`evidence/facade-preparation-stage5.json`](evidence/facade-preparation-stage5.json) and [`evidence/facade-preparation-stage5-provenance.json`](evidence/facade-preparation-stage5-provenance.json); logs are under `target/loop-layer-facade-preparation-stage5/`.

The facade design requires startup without a hidden warm-up interval and teardown without losing its ownership ledger. `GraphRuntime::prepare_with_initial_graph` now compiles/validates signal routes and planner bindings off-render and returns epoch one at frame zero. It invokes no planner/processor, callback, ingress command or receipt during preparation. `into_planner_and_resources` consumes a stopped runtime off-render while retaining both ledger and store for explicit cleanup. It does not settle pending requests or send MIDI to a stopped device.

`AudioRenderer::render_ports` and `AudioCapture::stage_ports` share conversion/DSP logic with the existing slice APIs through scoped enum adapters. They access only output/input windows respectively, allocate no temporary reference vector, and retain no callback borrow.

Three new runtime tests verify initial planned execution at frame zero under a one-step background validation budget, zero-call no-ops, retained-ledger pool recovery and off-render destruction; missing/wrong/out-of-range bindings and a signal cycle refused during preparation; and NoPlanner startup followed by normal live commit epochs and owning receipts. Two new loop cases compare five-channel offset output windows against both an independent ramp oracle and the slice API, preserving surrounding samples and DSP state on refusals/zero frames; and stage an exact stereo input window through prepared reservation/activation/freeze, checking original interleaved bytes and unpublished failed contributions.

| Fresh gate | Observed result |
| --- | --- |
| Runtime native debug/release | **177 passed per profile** |
| Loop native debug/release | **79 passed per profile** |
| Runtime Node/Chromium | **173 passed per runtime** |
| Loop Node/Chromium | **79 passed per runtime** |
| Prescribed workspace nextest | **2056 passed**, four unchanged opt-in canaries skipped |
| Selected Miri | **143 fresh executions across two explicit batches**: 117 runtime plus 26 loop cases |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Formatting, warning-denied build/docs, examples and policy/boundary checks | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; historical golden journal unchanged |

The initial fixture compared a `Sequence` with a submission `Ticket` (**E0308**); it was corrected to `Ticket.sequence()` without changing assertions/workload. All 21 broad gates passed. The first Miri invocation hit the **outer 1800-second tool deadline after 137 passing executions**, during the final MIDI target. No canonical combined runner success report was written, and no Miri process remained running. Completed logs and source hashes were retained; the final six MIDI cases then passed unchanged with the same seed, toolchain and per-target 900-second bound. `miri.execution_batches` records both commands/results. This is a complete fresh selected-case set, **not one uninterrupted runner pass**; the timed-out canonical `result.json` remains absent. No workload, heap guard or assertion was weakened.

All five new case workloads run unchanged under native/Wasm/Miri. The full loop planner/processor facade, coherent mixed-media capture/export and public state, separate-node graph/example, independent loop hosts and final delivery remain pending. Stage 5 remains open.

## Stage 5 — Fixed mixed-media ownership ledger

Milestone: `feat(loop-engine): retain mixed media through checked ownership slots` (the commit introducing this section), source base `1411a4fd`. Evidence: [`evidence/media-ownership-stage5.json`](evidence/media-ownership-stage5.json) and [`evidence/media-ownership-stage5-provenance.json`](evidence/media-ownership-stage5-provenance.json); logs are under `target/loop-layer-media-ownership-stage5/`.

`media::Ledger` owns each adopted audio/MIDI snapshot once while resident content, fixed playback/latest roles, pending pins and exports retain checked references. Media reservations precede promised capture activation; publication validates the entire mixed-media input before making it readable. Scope, owner generation, snapshot metadata, format/channel count, common duration, MIDI indexing and duplicate ownership are checked. Pending pins and external exports have independent fixed pools; neither consumes internal role storage. Final reference release invalidates the slot and releases both leases; generic page reclamation remains incremental. Unbinding preserves external/pending/resident owners. Explicit quiescent cleanup recovers all remaining references without pretending to stop music or emit device cleanup.

Four guarded integration cases cover saturation and role changes, pending retention through resident retirement, source removal, complete sample/packet reads, malformed/foreign/unindexed/duplicate inputs, cancelled reservations, stale tokens, owner-generation reuse and full cleanup. An independent **2,048-action** ledger model, seed **`0x87a4439cdd3517ab`**, predicts all resident/role/pin/export references, live metadata and snapshot credits after every action; it checks complete content throughout and exercises all ten action classes more than twenty times. A guarded private case verifies actual **`u32::MAX`** media/pin/export generation retirement. All five workloads are unchanged on native/Wasm/Miri.

| Fresh gate | Observed result |
| --- | --- |
| Runtime native debug/release | **177 passed per profile** |
| Loop native debug/release | **84 passed per profile** |
| Runtime Node/Chromium | **173 passed per runtime** |
| Loop Node/Chromium | **84 passed per runtime** |
| Prescribed workspace nextest | **2061 passed**, four unchanged opt-in canaries skipped |
| Selected Miri | **148 fresh passing executions**, 117 runtime plus 31 loop, in one complete runner invocation |
| Lifetime/authority prototypes | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Formatting, warning-denied build/docs, examples, policy/boundary checks | Passed |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; historical golden journal unchanged |

The initial private test included the allocator implementation inside a library enforcing `forbid(unsafe_code)`, causing nine unsafe-code lint errors. The existing support implementation was relocated **byte-for-byte unchanged** into `shoop_wasm_test_support::heap`, already a dev-only dependency, and re-exported to integration tests. The independent host includes that pure helper source directly without acquiring a production dependency on the shared harness. The private case retains its full allocator guard; production unsafe prohibitions and assertions were not relaxed. The failed compile log is retained. All 21 broad gates passed, followed by a complete fresh 148-execution Miri result; this supersedes the prior milestone's timed-out canonical report without rewriting its two-batch provenance.

This is ownership-component acceptance, not coordinated-facade acceptance. A caller-supplied export frame is not proof of a settled musical-state cut. Logical revision allocation, actual boundary preflight/re-preview and freeze/activation, coherent active-prefix acquisition, typed public outcomes/state, separate playback→FX→capture graph/example and independent loop hosts remain pending. Stage 5 remains open.

## Stage 5 complete — Coordinated runtime facade and public lifecycle

Milestone: `feat(loop-engine): coordinate runtime capture playback and coherent exports`
(the commit introducing this section), source base `915fd0b2`. Evidence:
[`facade-stage5.json`](evidence/facade-stage5.json) and
[`facade-stage5-provenance.json`](evidence/facade-stage5-provenance.json).
Logs are under `target/loop-layer-facade-stage5/`; the initial broad/Miri run and
complete hashed pre-refinement Rust sources are retained under
`target/loop-layer-facade-stage5-first/`.

`runtime::{Engine, Controller, Stopped}` connects the resolver, fixed media ledger,
separate playback/input consumers, prepared capture groups and actual signal DAG.
Preparation validates domain roles and imported content, reserves imported logical
revision watermarks and installs the initial graph without a warm-up callback.
Two capture banks permit transactional repeated Record. Boundary planning reserves
metadata/takes/intervals, latches a new refusal on every retry, freezes/activates
only after stable preview, and commits once before sample F. Finish validates
producer identities, commits staged common prefixes and then advances the resolver.

Typed operations retain reliable runtime receipts separately from pending resolver
records; media selections retain independent pins. Recovery Stop/cancel has an
independent domain slot as well as generic recovery storage. State/export replies
use a post-settlement/pre-render cut. Active-prefix snapshot failure releases
unpublished members without interrupting recording. Immutable MIDI initial state
is returned in channel-sized replies; `MidiState::from_channels` reassembles the
same exported projection without message replay. Quiescent extraction retains the
ledger/store for explicit idempotent disposal, including partially staged faults.

### Integrated checks and Stage 5 checklist mapping

All ten `shoop_loop_engine --test runtime` cases run their complete workloads under
native debug/release, Node, Chromium and Miri. The shared allocator guards cover
processing, prepared admission, result draining and cleanup; effect destruction
asserts it occurs off-render.

| Stage 5 requirement | Implemented evidence |
| --- | --- |
| Resolver-driven audio, gain/smoothing, wrap/seek/revisions | `audio`/`audio_ports` foundations plus integrated scalar stereo sample arrays, replacement/seek and `integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts` |
| Indexed MIDI, tracking/restoration, rebasing, SysEx and overflow/reset | Existing complete `midi_restore`/`midi` suites (including 257-byte SysEx and real-runtime overflow/reset/audio isolation), plus full facade original/restoration/cleanup packet comparisons and exported initial-state reconstruction |
| Typed runtime integration and single clock | Main coordinated case, requested/delivered/terminal late-result checks, validated producer roles, and `facade_role_validation_invalid_operations_and_imported_revision_exhaustion_are_checked` |
| Coherent group capture/state/export and refusal isolation | Exact immediate record→play; half-open recording prefix; repeated Record; imported revision seven followed by capture eight; checkpoint/metadata/tail-copy refusals; independently predicted page-exhaustion prefix; no partial export |
| Separate playback → FX → recording with immutable previous content | Actual host-input/playback → mixing/gain → capture/host graph; simultaneous monitored playback and new recording; independent captured stereo/MIDI bytes and retained old revisions |
| Executable full public lifecycle | `examples/coordinated_loop.rs`: stereo/MIDI record at frame one, immediate interior play at four, repeated wraps, seek, distinct second recording, retained-original replacement, stop, initial-state/audio/MIDI export and complete release/reclaim |

The main sample/packet/event oracle runs whole, single-sample and irregular host
partitions, reversed graph declaration order and journals 0/1/1024. The dense case
executes **128 one-sample boundaries** with all 32 normal runtime results unread,
checks all samples and **8,702 MIDI packets** before recovery Stop, then checks its
two cleanup packets and complete semantic journals/loss counters. No diagnostic
or external-result credit controls autonomous progression.

Failure cases check conservative checkpoint refusal without a false start pulse;
a repeated Record refused by metadata capacity while the old take continues;
optional snapshot `Budget` failure after one member was acquired, without leaking
its lease; and page exhaustion predicted from an independent page-reservation
formula across three partitions. Exhausted audio/MIDI share the exact completed
prefix/incomplete reason while live audio continues. Different failure positions
under insufficient resources are explicit, not a sufficient-resource partition
invariance claim. Cancellation covers queued and delivered work; recovery works
with normal domain/runtime results saturated. Stopped extraction recovers both
active takes and an audio-staged/MIDI-contract-fault interval. Imported resource
handles are removed separately after internal cleanup, recovering every configured
page/version/snapshot and media reference.

### Fresh verification

| Gate | Observed result |
| --- | --- |
| Runtime native debug/release | **178 passed per profile** |
| Loop native debug/release | **94 passed per profile** |
| Runtime Node/Chromium | **174 passed per runtime** |
| Loop Node/Chromium | **94 passed per runtime** |
| Prescribed workspace nextest | **2072 passed**, four unchanged opt-in canaries skipped |
| Selected Miri | **159 fresh passing executions**, 118 runtime + 41 loop, one complete invocation |
| Production lifetime/authority proofs | Positive builds/runs and deliberate **E0502/E0599** refusals |
| Public loop example | Debug/release executions and Wasm compilation; the latter is not execution |
| Broad driver | **25 gates passed**, including warning-denied workspace build/docs, both examples, policies and runner-hygiene tests |
| Existing native debug/release and actual offline/live worklet, both failure canaries | Passed; historical golden unchanged |

Provenance verifies **96 current Rust source hashes**, all 25 gate logs, archived
Miri logs and both production proofs. Tracing inventory covers 196 modules
(excluded52/direct33/indirect111); portable boundaries remain independent of
application tracing. The prior unsafe heap helper and production prohibitions
are unchanged.

### Retained failures and refinements

Initial compile/setup errors included an incorrect Diagnostics path/AudioBlock
lifetime, wrong configuration field names, insufficient graph port capacity, and
asserting scheduled admission as Applied. A four-row MIDI checkpoint fixture
correctly refused the declared 512-event interval; the positive fixture provisions
eight rows, while a dedicated negative case retains four and verifies refusal.
The replacement MIDI oracle initially assumed a restart when recording begins
while already monitoring playing content. The existing authoritative contract
requires continuity; the corrected oracle still compares every packet, including
initial-state restoration when the new take is subsequently played. Initial
tracing inventory omissions were corrected. All failed logs are retained; not
every early intermediate failed source tree was archived.

The first Miri invocation completed **155 executions** then reached the unchanged
**900-second wall watchdog** on the dense facade case. No canonical success was
written. Its complete source tree and completed/partial logs are archived. A left
running interpreter child was observed and explicitly terminated; the runner now
kills/reaps the process group on timeout and preserves captured output. Three
Python tests verify result preservation, timeout cleanup and the exit race.

Repeated pedal values were needlessly rescanning keys. The projection now updates
controller values without redundant scans when pedal up/down state is unchanged,
and release-unheld returns immediately when another sustain/hold remains down.
An independent **1,024-action** three-hold ordered-map oracle, seed
**`0xcc464669a5a5a5a5`**, checks held/latch/key state and controller values after each
action, including reset and channel modes. Its full workload runs unchanged on all
platforms. The unchanged dense musical test then passed in **890.36 seconds wall**.
The final dense playback-only fixture also reduces unused index storage from
8 versions × 8 rows to 2 × 1, sufficient for its two one/two-packet imports at
replay span 128. No frames, events, profiles, assertions, guards or stack limits
were reduced; other recording/exhaustion fixture capacities are unchanged.
The final full selected run passes the same watchdog. Miri's printed harness
elapsed values use a simulated clock; they are not host wall measurements or a
stable performance claim.

Requested timing notes, with measured command durations and scope/cache/concurrency
caveats, are in [TEST_EXECUTION_TIMES.md](TEST_EXECUTION_TIMES.md).

**Stage 5 is complete.** This does not complete integrated inter-loop/composite
lifecycle or live graph/program replacement (Stage 6), independent loop native/
AudioWorklet hosts (Stage 7), later adversarial/performance/CI work, or final
unmerged-PR/final-head-CI/review delivery. Existing generic hosts remain regressions,
not substitute loop-host acceptance.

## Stage 6 — Integrated cascade, nested clock and capture-outcome scenario

Milestone: `test(loop-engine): verify integrated cascades nested clocks and capture outcomes`
(the commit introducing this section), source base `6035ef4e`. Evidence:
[`interloop-stage6.json`](evidence/interloop-stage6.json) and
[`interloop-stage6-provenance.json`](evidence/interloop-stage6-provenance.json);
logs under `target/loop-layer-interloop-stage6/`. This milestone adds a guarded
integration/reference test and registers it with Miri; **production Rust is
unchanged**. Stage 6 remains open.

The actual facade runs four audio/MIDI primitives, each with separate playback,
mixing/gain and capture consumers, plus a regular outer and explicit inner
composite. A records `[0,4)` and starts playback at four; its pulse starts B, whose
pulse starts C at that same frame. B then wraps every sample. A countdown delivered
at zero waits for B's third pulse and starts the outer composite at six. The
outer's one-tick gap precedes each five-tick inner program; D starts at seven and
thirteen. No nominal-unit timer or extra host callback drives those activations.

D records at nine and exhausts at ten, retaining a one-frame audio/MIDI common
prefix and its initial held-note projection. A second autonomous capture at fifteen
is refused; D's existing playback continues, its capture revision is not consumed,
and no false recording-start pulse is published. The inner/outer timelines keep
progressing. All loops/composites stop at eighteen, while independent live input
passthrough remains audible.

Independent checks cover:

- Every output sample and complete absolute-frame MIDI restoration/original/
  cleanup packet, including release velocities.
- A complete frame/event table, including simultaneous cascades, countdown,
  delayed nesting, regular pass, failure events, source-stop masking and ordered
  stops; journals 0/1/1024 are checked against truncation of that same table.
- Every public field of every observer-provided primitive/composite state:
  positions/cycles, content/capture revisions/lengths, incomplete/monitoring,
  last pulse, iteration/pass and entry frame. Explicit state requests confirm
  the post-settlement cut; countdown requested/delivered/terminal frames are
  independently asserted as 0/0/6 with no transport lateness.
- Exact A/D export bytes and durations. D's requested export cuts at ten and
  fifteen both retain the original capture cut ten, not a relabelled recording.
- An independent page calculation: six imported pages plus two A pages leave
  eight used; the first D reservation needs `8 + 1 + 128 = 137`, exactly the
  store capacity. After D commits two actual pages, continuation would require
  138 and a new take 139. These predicted refusals agree with execution and the
  final ten used pages before teardown.
- Explicit export release, idempotent cleanup and removal of imported handles
  recover all **137 pages, 64 versions, 64 snapshots**, and zero media references.
  Callback/admission/polling/cleanup paths remain heap guarded; effects are
  destroyed off-render.

The complete scenario runs `[19]`, nineteen one-frame callbacks, and
`[3,2,7,1,6]`, with reversed loop/trigger/program declarations, signal graph,
bindings/effects and imports. The deliberate constrained-storage case gives the
same specified failures across these profiles; it is not a claim that every
insufficient-resource workload is universally partition invariant.

Fresh scoped results: **95 loop tests per native debug/release profile and per
Node/Chromium runtime**, **2073 workspace passes/four existing skips**, and all
**18 scoped gates**. The one new complete Miri target passes with all profiles,
assertions and guards unchanged, taking **468.43 seconds measured wall time**
under the existing 900-second watchdog. The full runner now registers 160 selected
executions, but **this is one fresh execution, not a fresh combined160 run**.
The previous full159 result remains historical. Standalone generic-runtime
release/Wasm/Miri, lifetime/authority proofs and native/actual-worklet host fixtures
were not freshly rerun for this test-only change. Provenance verifies 97 current
Rust source hashes, including all 96 preceding Stage 5 sources unchanged, eighteen
gate logs and the fresh Miri artifact.

Retained initial failures: incorrect fixture imports/endpoint integer types, then
a journal oracle that counted idempotent primitive Play/Stop as state-change
entries. The existing contract only journals actual state changes; successful
no-op commands still receive Applied outcomes. The corrected independent table
keeps complete comparisons, and all external transition completions are explicitly
checked. Failed logs remain; those early intermediate fixture trees were not
separately archived. An ignored Nix SQLite-busy warning did not fail any gate.

The full coordinated-scenario checklist item is now verified. Live generation-
checked topology/program activation, running-program replacement, broader nested
seek/stop/pending lifecycle and mixed control/replacement/disconnect stress still
remain before Stage 6 can complete. No independent loop host or final CI/review
acceptance is claimed. Additional scoped timing observations are in
[TEST_EXECUTION_TIMES.md](TEST_EXECUTION_TIMES.md). The remaining flat-metadata,
receiver staging, atomic activation and running-program constraints are recorded
in [TOPOLOGY_ACTIVATION_DESIGN.md](TOPOLOGY_ACTIVATION_DESIGN.md).

## Stage 6 — Flat compiled topology and borrowed metadata views

Milestone: `refactor(loop-engine): flatten compiled topology metadata and borrow views`
(the commit introducing this section), source base `198fe288`. Evidence:
[`topology-storage-stage6.json`](evidence/topology-storage-stage6.json) and
[`topology-storage-stage6-provenance.json`](evidence/topology-storage-stage6-provenance.json);
logs under `target/loop-layer-topology-storage-stage6/`. This implements a
representation prerequisite, not receiver staging or live activation.

Compiled topology now owns seven flat arrays rather than nested program/target/
segment/component owners. Borrowed program and target views retain logarithmic
sparse lookup; target and component iteration is exact-sized and double-ended.
Target segment ranges are relative to the containing program's narrowed segment
slice, so inserting unrelated metadata earlier in global tables does not falsely
change an existing running program. Canonical declaration-order behavior, sync
SCCs, sparse timelines, owning replacement/refusal and resolver semantics remain
unchanged.

The public view adaptation is documented in the
[compiled topology guide](../../src/rust/shoop_loop_engine/docs/topology.md).
`program()`/`target()` return borrowed view values, target lists use `get` or
iteration rather than an owned-target slice, and components expose member slices
instead of separate boxes. Copying/cloning a view does not acquire an owner.
`PreparedTopology::storage_bytes()` reports header plus owned array bytes;
`Limits::topology_storage_bytes()` reports a checked aggregate maximum at declared
limits. Aggregate metadata now must fit `i32::MAX`, in addition to existing
individual-array/resolver checks. This is an explicit preparation-capacity
constraint, not a musical-work quota. It excludes compiler scratch, allocator
overhead and future receiver/candidate storage.

Three new guarded tests independently check:

- Sparse overlap/resumption mode/ordinal/occurrence intervals, canonical order
  with a mutual-sync component, forward/backward iterator behavior, empty
  topology/program, out-of-range access and stale generation.
- Insertion of an earlier unrelated program while another runs: preserved program
  identity/state, accepted replacement and owning return, retained unread
  requests, changed-running/stale-epoch owning refusals, then valid stopped
  replacement and cursor/duration reset.
- Exact linear owned/max byte scaling for 1/2/4/8/16/32 programs; changing duration
  from one billion to `u64::MAX` does not change storage. Half-open endpoints and
  aggregate/arithmetic overflow are checked without allocation.

The production-type `topology_view_lifetime.rs` prototype builds/runs with explicit
public view types; three deliberate E0502 variants reject program, target and
component borrows used after replacement. The existing runtime decision-lifetime
and consumer capture-authority proofs also freshly pass their positive paths and
E0502/E0599 negative checks.

Fresh verification: **178 runtime / 98 loop tests per native profile**, **174/98
per Node and Chromium runtime**, **2076 workspace passes/four existing skips**,
and all **25 broad gates** including warning-denied workspace build/rustdoc,
examples, policy audits, existing native hosts and actual offline/live generic
worklet with both failure canaries. Its historical golden is unchanged. No new
unexpected compile/behavioral failure occurred; an ignored Nix SQLite-busy
warning did not fail a gate.

**45 selected loop Miri executions pass in one fresh invocation**, including all
ten facade cases, the complete interloop target and all three new tests, with
unchanged workloads/guards and the 900-second per-command watchdog. Measured
whole-invocation wall time was **3057.90 seconds (50m58s)**. The full runner now
registers 163 executions, but the preceding 118 generic-runtime executions remain
historical; this is **not** a fresh combined 163 run. Provenance verifies 98 current
Rust source hashes, 64 unchanged runtime/shared-support source hashes, all 25 gate
logs, archived Miri logs and the typed borrow/authority proofs.

The output is still allocated to compiled content size. Reusable maximum-capacity
receiver banks, bounded wire chunks, incremental validation, graph/domain/media
activation, changed-running iteration-zero behavior and broader lifecycle stress
remain Stage 6 work. The generic host regressions are not independent loop-host
acceptance. No Stage 6 completion, final CI/review approval or deadline guarantee
is claimed.

## Stage 6 — Reusable topology receiver and incremental validation

Milestone: `feat(loop-engine): stage and validate portable topology images in fixed storage`
(the commit introducing this section), source base `11cad0f4`. Evidence:
[`topology-staging-stage6.json`](evidence/topology-staging-stage6.json) and
[`topology-staging-stage6-provenance.json`](evidence/topology-staging-stage6-provenance.json);
logs under `target/loop-layer-topology-staging-stage6/`. This implements receiver
readiness and ownership, not live facade activation.

`topology::staging::Stager` prepares/pre-touches maximum-capacity metadata and
validation scratch. LTP1 uses explicit little-endian 64-byte headers/records,
checked limits/counts and canonical zero reserved bytes, not native Rust layouts
or cross-instance pointers. Ordered chunks have a finite record budget; syntax,
scope, offset and budget refusal leaves the whole chunk unconsumed. Incomplete
and semantically failed candidates are unreadable. No held call waits for a later
message. Validation advances a finite state machine under a supplied step budget;
zero budget does no work.

Readiness checks exact-generation references, program ownership/nominal length,
complete ranges, sparse segment modes/ordinals, unique component membership,
recursive programs, deduplicated sync/action relations, strong pure-sync SCCs
and canonical dependency order. Multi-node pure-sync components must follow their
single-source links through exactly all members back to the start; a cycle plus
a downstream tree is not accepted as an SCC. The implementation uses linear fixed
scratch and constant-sized work steps, but its simple duplicate-edge and component
scans have quadratic worst-case total preparation work. This is background
progress, not a musical subdivision limit or a deadline claim.

`take` transfers a ready owner; `recycle` accepts compatible fully provisioned
storage and returns rejected owners unchanged. Private vectors retain capacity,
so cancellation/reuse does not allocate, grow or free storage. Control compilation
remains content-sized. Scope allocation is preparation-only; generations never
wrap. Ready certifies timing metadata, not graph/resource/view-binding compatibility
or historical activation watermarks. The
[receiver guide](../../src/rust/shoop_loop_engine/docs/topology-staging.md) documents
wire fields, ownership, conservative budget charging on errors and integration
limitations.

Seven guarded integration tests plus a private exhaustion test cover:

- An independent exact header/body golden and full native/Wasm roundtrips under
  1/2/7/14-record chunks and 1/3/unlimited validation budgets; zero-budget behavior,
  stale tokens, unchanged out-of-range output and complete bank reuse.
- Header/syntax/reserved-byte/enum/generation/offset/budget/foreign-scope refusal,
  partial-chunk atomicity, and rejected undersized/wrong-limit owners. Two
  receivers exchange compatible buffers without callback destruction.
- 21 semantic/structural mutations, recursive/action cycles, latched unreadable
  failure and subsequent full reuse.
- **512** three-node sync/action graphs against independent transitive reachability
  and canonical SCC ordering, including expected cycle refusals. False SCC merging
  and incorrect ready-group tie order are separately rejected.
- **128** owning standalone-resolver exchanges with stale-epoch returns and
  alternating receiver recycling; actual buffers remain owned until off-render
  disposal.
- **128** active one-frame resolver intervals while another candidate is partially
  received and fails validation. Exact positions/cycles/pulses, unchanged active
  epoch and an unread result are preserved.
- **128** nested regular/explicit images against raw-entry mode/duration arithmetic,
  reversed declarations, mixed budgets and a sparse `u64::MAX` endpoint.
- Final generation/scope exhaustion without wrap or poisoning shared test state.

The new production-type staging prototype builds/runs, rejects a later-used view
across cancel/take with E0502, and rejects recycling a still-borrowed owner with
E0505. Existing topology/runtime borrowed-view and capture-authority positive and
negative proofs were also freshly rerun.

Fresh results: **178 runtime / 106 loop native tests per profile**, **174/106 per
Node/Chromium runtime**, **2084 workspace passes/four existing skips**, and all
**25 broad gates**. Existing native and actual offline/live generic worklet
regressions/canaries pass with unchanged golden. **53 selected loop Miri executions
pass in one invocation**, including all new cases and all previous selected loop
cases, with unchanged workloads/guards and the 900-second per-command watchdog.
Measured whole-invocation wall time is **3270.64 seconds (54m31s)**. The full runner
now registers 171; the preceding 118 generic-runtime executions remain historical,
not a fresh combined 171 claim. Provenance verifies 102 current source hashes,
64 unchanged runtime/shared-support sources, 25 gate logs, archived Miri and all
borrow/move/authority proofs. Tracing inventory is 199 modules: 55 excluded,
33 direct and 111 indirect.

There were no unexpected compilation or behavioral failures. Expected negative
compilation checks and malformed-input refusals remain in the evidence. An ignored
Nix SQLite-busy warning did not fail a gate. This does not complete Stage 6:
facade receipts/transport integration, atomic graph/domain/media activation,
changed-running iteration-zero behavior and broader lifecycle stress remain open.
Actual loop-host and final CI/review acceptance remain later work.

## Stage 6 — Fixed-graph facade topology activation

Implementation milestone: `feat(loop-engine): expose fixed-graph topology staging and activation`
(the commit introducing this section), base `9bab9ddf`. This is an implemented,
broad-tested integration slice, **not Stage 6 completion or a complete stage-level
Miri pass**. Evidence: [`facade-topology-stage6.json`](evidence/facade-topology-stage6.json)
and [`facade-topology-stage6-provenance.json`](evidence/facade-topology-stage6-provenance.json).
Logs: `target/loop-layer-facade-topology-stage6/`.

Generic planner operation/plan contexts now carry authoritative graph epoch and
enclosing callback start. A guarded generic test proves these values across
internal intervals, graph replacement and a zero-frame callback. NoPlanner keeps
its signal-only behavior. The production lifetime prototype is updated and all
positive/negative view, staging and capture-authority proofs pass freshly.

The opt-in facade prepares candidate banks plus a maximum-capacity initial owner.
Typed begin/append/validate/status/cancel/activate/recycle operations use reliable
receipts and owning payload returns. Work allowances are shared across a whole
callback, not replenished at musical boundaries. Ready candidates survive failed
activation; successful activation retains the displaced owner until explicit
recycling. Recovery cancels unpublished candidates independently of normal
request/result slots. State/export cuts carry graph/topology epochs.

This path preserves the signal graph, exact logical IDs/kinds, media-role bindings,
resource claims and selected content. Changed programs must already be stopped;
a Stop queued at the same frame has not yet settled when activation is delivered.
General graph/role-generation transactions and next-iteration-zero running changes
remain open. See the [public contract](../../src/rust/shoop_loop_engine/docs/topology-activation.md).

Six guarded cases establish:

- Complete independent stereo samples, absolute MIDI packets, ordered journals,
  every observer-provided public State field, epoch cuts and retained export bytes
  for a staged stopped-program change. Whole/single/irregular partitions, reversed
  declarations and journals0/1/1024 agree. Failed running/stale activation retains
  the candidate; same-frame Play/query uses the newly activated configuration.
- Two-record/three-step callback work limits remain shared across separately
  scheduled operations and one-sample internal intervals. Validation resumes with
  exact progress. Recovery cancels Ready storage with32 normal receipt slots
  stalled while128 automatic frames continue.
- Stale/foreign stage IDs, incompatible logical generations, malformed syntax and
  semantic records, and unexpected payloads preserve the working clock/configuration.
- Pre-delivery transport cancellation preserves the candidate; a later successful
  sync-source change terminates an already delivered `u64::MAX` countdown with
  MissingSync and exact requested/delivered/terminal frames.
- Idempotent guarded cleanup recovers Receiving/Validating/Ready/Failed/Taken-retired
  banks, media references and resource pools after controller disposal.
- Activation during monitored stereo/MIDI recording preserves the take, exact FX
  output, complete restoration/original packets, immutable prefix/full audio and
  MIDI bytes, all initial MIDI channels and full resource recovery across three
  partitions. Positive capacity is provisioned for replaying captured restoration
  originals, not just original note traffic.

Receiver preparation now explicitly writes through black-box references to touch
metadata and scratch, including vectors subsequently cleared for reuse. This
strengthens preparation intent without changing callback workloads or asserting
an OS paging/deadline guarantee.

Fresh broad results: **179 runtime/112 loop native per profile**, **175/112 per
Node/Chromium**, **2091 workspace passes/four existing skips**, **25 broad gates**,
104 source hashes, closed tracing inventory200 (56 excluded/33 direct/111 indirect),
and unchanged existing native/actual generic-worklet golden and failure canaries.
These generic hosts do not substitute for the future independent loop hosts.

Miri was interrupted when the goal was paused. **171 selected executions completed:
119 runtime and52 loop**, including the new context and all six facade cases.
The dense case's supervisor disappeared; orphan process group2424461 was explicitly
terminated. Completed logs, source hashes, runner and cleanup record are archived
in `miri-interrupted/partial.json`; in-memory output of the incomplete case was
not recovered. There is **no full178 pass or whole-invocation wall measurement**.
The seven uncompleted/unreported cases remain recorded in [MIRI_JOBS.md](MIRI_JOBS.md).
The user-approved cadence defers routine Miri reruns to the consolidated applicable
stage, rather than rerunning for this smaller step.

Retained setup failures include incorrect State/resource-command API use,
Option<Frame> typing, insufficient positive-fixture scheduler payload stride,
summing an already cumulative dropped-journal counter, and attempting to construct
a crate-private State. The reference now independently checks all public fields;
no API was exposed or assertion removed to bypass that privacy boundary. Failed
logs remain in provenance; their intermediate source trees were not archived.
An ignored Nix SQLite-busy warning did not fail a broad gate.

## Stage 6 — Unpublished program-cut prerequisite (stage remains open)

`Resolver::probe_program_boundary` now shares ordinary shadow initialization,
control/clock settlement and checked cursor advancement, but stops before the
selected program's child and trigger actions. The returned frame/epoch/state and
zero-entry predicate distinguish a real zero cut from a stalled cursor. Probing
publishes no receipts, musical state, events, frame or epoch; it invalidates an
uncommitted preview, cannot be committed, and cannot invalidate an already
committed interval. This is not a candidate capability or running-program
replacement. The owning activation/retirement transaction remains future work.

Evidence: [`program-probe-stage6.json`](evidence/program-probe-stage6.json) and
[`program-probe-stage6-provenance.json`](evidence/program-probe-stage6-provenance.json),
logs `target/loop-layer-program-probe-stage6/`, and the
[API guide](../../src/rust/shoop_loop_engine/docs/program-boundaries.md).

- One private guarded test inspects scratch to prove old recording/playback/stop
  and trigger actions have not been emitted; provisional request outcomes remain
  unpublished, including explicit-program end.
- Three integration tests independently check128-frame regular/explicit clocks
  across whole/single/irregular partitions, reversed declarations, journals0/1/1024
  and two fully occupied unread request slots. Full decisions, journals and loss
  counters also agree with execution without probes.
- Eleven same-frame cases cover Stop, seek, restart, idempotent Play, cancellation,
  refusal, supersession, reactive Stop and source-stop suppression. Nested-family
  and upstream capture-start refusals, stale/primitive IDs, duplicate observations,
  protected committed intervals and changed epochs are covered separately.
- A production-type positive/E0502 compile proof rejects a retained decision borrow
  across the mutable probe. No borrowed view is converted into activation authority.

Fresh results: **116 loop tests per native debug/release and Node/Chromium**, **2095
workspace passes/four existing skips**, **19 scoped gates**,106 Rust source hashes,
and closed tracing inventory201 (57 excluded/33 direct/111 indirect). Formatting,
warning-denied workspace build/rustdoc, debug/release example and Wasm example
compilation, boundary/test/Wasm/tracing policies and14 Miri-helper Python tests pass.
The unchanged standalone runtime/native/actual generic-worklet results remain
historical; those hosts were not rerun by this scoped driver. No new actual loop
AudioWorklet acceptance is claimed.

No unexpected test/compiler failure occurred in this step; the E0502 refusal is
intentional. Four new Miri executions are registered (one library and three
`program_boundaries`), bringing the current inventory to182=119runtime+63loop.
They and affected resolver regressions are deferred to the consolidated applicable
stage run. The historical interrupted171/178 remains partial, not a fresh182 pass.
Stage6 and the later acceptance stages remain incomplete.

## Stage 6 — Standalone owning single-program transaction (facade integration open)

`Resolver::stage_program` now retains a prepared single-program candidate and a
normal request-family slot. Selection uses the old unpublished stopped/zero cut;
the real preview uses the new action DAG without old-zero child actions. Commit
swaps metadata owners and publishes epoch, state, receipts and events together.
One displaced/rejected owner remains retained until `take_program_change` returns
it with the terminal completion. Raw Program tags and completion-only collection
are refused. Logical generations, sync/triggers and other programs remain fixed.

Evidence: [`program-change-stage6.json`](evidence/program-change-stage6.json) and
[`program-change-stage6-provenance.json`](evidence/program-change-stage6-provenance.json),
logs `target/loop-layer-program-change-stage6/`, and the
[owning API guide](../../src/rust/shoop_loop_engine/docs/program-changes.md).

Nine guarded cases verify:

- independent full positions/cycles/cursors and ordered journals across whole,
  single/irregular partitions, reversed declarations and journals0/1/1024;
- exact activation, preserved contiguous playback, compiled nominal length changes,
  and removal when the new DAG orders the removed target before its former parent;
- capture-family refusal/retry without revision gaps, and no old-zero phantom pulse
  or capture restart in the candidate preview; old actions execute once if refusal
  legitimately preserves the old working program;
- idle-clock waiting, cancellation, stale epochs, owning rejection, one retained
  owner slot, reliable results and invalid raw tags/shapes;
- same-frame Stop/seek/restart/refusal and cancellation after a speculative preview;
  empty/stopped changes and explicit-end installation without implicit playback;
- nested removal and direct-child override; all128 one-sample intervals with every
  request slot occupied and the successful owner/result left unread.

Fresh results: **125 loop tests per native debug/release and Node/Chromium**, **2104
workspace passes/four existing skips**, **19 scoped gates**,108 Rust source hashes,
and closed tracing inventory202 (58 excluded/33 direct/111 indirect). Warning-denied
build/rustdoc, formatting, examples, dependency/test/tracing policies and14 helper
Python tests pass. A production positive proof and E0505/E0502/E0505 refusals protect
candidate moves, decision borrows across collection and retired-owner destruction.
Unchanged standalone runtime/package-Wasm and generic-host results remain historical.

The first expanded focused run had one incorrect empty-program fixture expectation:
a supplied node length42 is replaced by compilation's derived empty nominal length0.
`focused-expanded.log` is retained; the intermediate source tree was not archived.
The expectation was corrected to the compiler contract, and nonempty nominal length
publication is separately checked. No workload, guard or musical assertion was
removed. The later focused run and all19 scoped gates pass.

Nine new Miri executions are registered, bringing the current inventory to191
(119runtime+72loop); no interpreter was run for this smaller step. The historical
interrupted171/178 remains partial. The standalone transaction is not yet wired to
facade receiver-bank ownership, runtime execution tokens, graph-epoch rechecks or
actual capture-bank preflight. General graph/role transactions, remaining mixed
stress, consolidated Stage6 Miri and later host/delivery acceptance remain open.

## Stage 6 — Fixed-graph facade owning program activation (stage remains open)

The adapter now connects a Ready receiver bank to `Resolver::stage_program`, a
normal domain slot and the runtime's reliable execution token. `ProgramActivate`
checks the authoritative graph epoch and expected topology epoch. Fixed logical
identities/sync/triggers/other programs preserve graph, role and resource bindings.
Actual capture preparation/first-interval reservation receives candidate decisions;
family refusal re-previews the old working configuration before publication.
`ProgramChanged` and coherent State/Export replies identify the settled cut.
After finish, internal collection restores a failed/cancelled candidate to Ready
or retains the displaced owner until explicit recycle, independent of unread
external results. Recovery `TopologyCancel` also recycles a cancelled unpublished
owner after that collection point. Raw Program musical tags remain invalid.

A render fault may follow configuration publication but precede interval finish.
Permanent `Resolver::quiesce` now permits final owner recovery without pretending
the interval rendered or rolling back the published configuration. It preserves
state/time/epoch and terminal outcomes, cancels nonterminal requests, invalidates
decisions and forbids execution. Stopped cleanup uses actual capture-store prefix
information and remains idempotent. This is final teardown, not pause/resume or
musical Stop; design reasoning is in `TOPOLOGY_ACTIVATION_DESIGN.md`.

Evidence: [`facade-program-stage6.json`](evidence/facade-program-stage6.json),
[`facade-program-stage6-provenance.json`](evidence/facade-program-stage6-provenance.json),
logs `target/loop-layer-facade-program-stage6/`, and the
[facade guide](../../src/rust/shoop_loop_engine/docs/topology-activation.md).

Six new guarded cases cover:

1. Full independent stereo samples, original/restoration/cleanup MIDI, ordered
   journals and every State field across whole/single/irregular callbacks,
   declaration reversal and journal capacities0/1/1024. A request delivered3
   activates6; old immutable exported media remains readable with the new cut epoch.
2. Real playback→FX→capture banks, Record6→Play8, exact prefix/full immutable audio
   and MIDI exports, versus a fully occupied media ledger refusing the whole
   configuration at6. Refusal preserves the old playback, epoch and revision,
   leaves no Program/Record event, and returns the candidate Ready.
3. Pending sequence/stage cancellation, an idle clock, intervening topology
   replacement, controller disappearance, and an FX fault after activation6 but
   before finish. All banks/resources recover; cleanup twice remains safe.
4. Graph/topology/target/stage/raw-tag delivery refusals, pre-dequeue ticket
   cancellation, stopped activation without a new pulse, receiver recycling and
   stale stage-generation rejection without losing the current Ready owner.
5. One busy program-owner slot, sixteen occupied normal domain slots, unread
   results,130 source wraps, independent recovery cancellation and a later
   successful bank retry while both displaced owners and15 pending musical
   requests remain retained. Internal clock progress never needs result reads.
6. Standalone quiescence before preview, during speculation, during applied or
   pending committed intervals, and after finish: unchanged state/time, permanent
   execution refusal and exact rejected/displaced owner return.

Fresh results: **131 loop tests per native debug/release and Node/Chromium**, **2110
workspace passes/four existing skips**, **19 scoped gates**,108 Rust source hashes
and101 unchanged preceding Rust sources. Tracing inventory remains202
(58 excluded/33 direct/111 indirect). Warning-denied workspace build/rustdoc,
examples, formatting, dependency/boundary/test/tracing policies and14 Miri-helper
Python tests pass. The production positive lifetime program runs; negative
E0505/E0502/E0505/E0502 cases protect candidate move, result collection, retired
owner destruction and quiescence across retained views.

Retained unsuccessful runs are fixture/inventory corrections, not weakened gates:
`focused-first.log` has E0689 from an untyped integer range; `focused-second.log`
uses nonexistent `Resolver::new` instead of `prepare`; `focused-third.log` refuses
fixture preparation because one media slot cannot hold both initial primitive
media identities. The refusal fixture now has exactly two occupied slots, so the
intended capture admission still fails Capacity. `first/checks.log` and
`first/miri-helper-tests.log` preserve the broad run stopped by the old191-case
inventory assertion; the exact expected count was updated to197 to include all
six added executions. Earlier intermediate source trees were not archived.

No Miri interpreter was launched for this smaller slice. Current inventory197
(119runtime+78loop) is registered, not passed; historical171/178 remains partial.
Standalone runtime/package-Wasm and generic native/worklet host results were not
freshly rerun; workspace regressions were. General graph/role-generation
transactions, broader mixed stress and consolidated Stage6 verification remain
open. The restricted controller cannot yet exercise a real intervening live graph
change; its planning-time epoch recheck needs that later integrated case.
Independent loop hosts, final-head CI and review acceptance remain pending.

## Stage 6 — Generic graph-bank and activation-permit prerequisite

The generic runtime now separates validated graph ownership from publication.
`prepare_graph_staging` allocates fixed owner slots before rendering; normal-lane
`StageGraph` validates incrementally and returns a scoped StageId in the owning
command completion. Ready candidates and Taken retirement slots retain exactly
one graph owner, returned through `ReleaseGraph` and normal completion credits.
NoPlanner and legacy commit/remove/install paths remain available.

Planner contexts lend `GraphAccess`, whose exclusive permit preflights identity,
current/validated epoch, next epoch, actual-capacity retirement budget and pending
MIDI compatibility. The caller finishes all fallible domain work before publishing
its state and consuming an infallible graph/epoch swap. Dropping the permit changes
nothing. This is graph authority, not arbitrary domain rollback, role cleanup or
processor/resource construction. Conservative aggregate preparation arithmetic and
`PreparedPlan::storage_bytes` account for vector capacities rather than row counts.

Review exposed a same-frame compatibility hazard: planning occurs after due MIDI
injections, so removing an injected target could strand its buffer until a later
graph. Preflight now refuses Busy and leaves the old graph to consume the packet.
Operation-time activation precedes MIDI delivery under existing scheduler ordering;
a subsequently missing target fails InvalidId. Recovery operation contexts cannot
activate even if a synthetic planner over-classifies that operation as recovery-safe.

Evidence: [`graph-activation-stage6.json`](evidence/graph-activation-stage6.json),
[`graph-activation-stage6-provenance.json`](evidence/graph-activation-stage6-provenance.json),
logs `target/loop-layer-graph-activation-stage6/`, and the
[archived API notes](../../src/rust/rt_runtime/docs-previous/graph-activation.md).
The new book is owned by the separate documentation worker; current changes are
specified in `src/rust/rt_runtime/docs-changes-loop-layer/`, not applied to its
chapters. That handoff currently covers this graph/recovery slice; earlier-stage
backfill and final publication remain pending.

Seven guarded integration cases verify:

1. Complete independent audio, absolute MIDI and domain/epoch cuts: operation-time
   and deferred activation7 across whole/single/irregular callbacks and reversed
   declarations. Callback-end7 remains unpublished until a nonempty callback;
   zero callbacks leave MIDI and epoch untouched. New routes/bindings and domain
   values appear together, with correct delivered/actual result frames.
2. Dropped/refused permits, wrong/foreign/stale IDs and epochs, busy owners,
   overallocated candidate and displaced metadata, recovery refusal and stage
   generation reuse preserve both owners and working output.
3. One-step validation keeps the old graph running; malformed descriptors and
   pre-dequeue cancellation return original owners without issuing new IDs.
   Deferred cancellation and release-before-activation resolve without publication.
   Continuous recovery is serviced while both staged and legacy validation retain
   progress with one validation step per callback.
4. All16 normal results remain unread while128 one-sample intervals execute;
   independent recovery cannot force a graph swap. Rendering continues after
   controller disappearance while ready/retired owners remain retained.
5. Removing/reinstalling an inactive physical node changes catalog generation and
   epoch, making a staged reference stale; its owner returns and revalidation
   rejects the old generation, without changing active output.
6. A delivered injected MIDI packet prevents planning-time removal, is consumed
   once, and never replays after that node returns. The operation-before-MIDI case
   instead rejects the missing target at delivery.
7. NoPlanner stages/releases the same owner128 times, then uses legacy commit;
   output and normal owning lifecycle remain intact.

One private edge case verifies nonwrapping scope/generation/epoch exhaustion,
aggregate-size rejection before allocation, initial activation without a displaced
owner, and inert authority. The seven integration cases run heap/destructor guards;
the private arithmetic/identity case is separate, not claimed heap-instrumented.

A guarded transport case separately checks parking/resumption with one/two
publication slots, cancelled recovery, late cancellation of an already-dequeued
normal request, FIFO order, unread owning returns and off-render disconnected
cleanup of current/parked/queued owners. Parking uses one fixed inline slot,
creates no admission credit, and never rechecks the old cancellation observation.

Fresh results: **runtime188/loop131 native per profile**, **runtime184/loop131 in
Node and Chromium**, **2119 workspace passes/four existing skips**, **25 broad
gates**,110 Rust source hashes and103 unchanged preceding sources. Tracing
inventory203 closes at59 excluded/33 direct/111 indirect. Seven positive production
prototypes and18 expected compile refusals pass, including five new graph authority
checks: E0502 graph/candidate views, E0505 permit consumption, E0515 escaping
borrowed authority, and E0624 private construction. Existing planner/resource/loop
lifetime proofs were refreshed against the changed context schema.

Generic native debug/release offline/two-thread and actual non-isolated browser
host regressions pass. Offline hashes remain samples480331717/MIDI3308430490/
graph487351092 for2601 callbacks/332928 frames/34 MIDI events. Offline/live memory
remains1,703,936 bytes initial=final. Both processor and Rust-heap canaries fail as
intended; the golden raw journal hash is unchanged. This remains generic-host
regression evidence, not actual loop-host acceptance.

`focused-expanded.log` retains one fixture-order failure: it assumed MIDI preceded
a planner operation solely by submission sequence (observed sequence3 before2).
The established scheduler orders planner operations first. The corrected case
uses deferred planning activation to test already-delivered MIDI, and separately
checks operation-time activation before injection. No assertion/workload was
removed. The intermediate failing fixture source was not archived.

Inspection also caught a missing Cargo/test-harness separator in the new exact Miri
filter before any interpreter launch. The filter now includes `--`, a helper
assertion checks all exact-filter separators, and a native mirror discovers/passes
exactly one case. The helper gate was refreshed (14 tests); its earlier passing
log is retained. Neither helper execution nor that native mirror is Miri evidence.

The expanded recovery regression initially failed at `execution` because the
second callback could not obtain a recovery result while normal validation held
the transport's current slot. `recovery-pre-fix/` retains the failed log, tracked
source patch against the recorded base, and both new Rust source files needed to
reproduce it. The correction parks the normal owner before servicing the next
callback's eligible recovery and preserves validation progress across unrelated
completions. The unchanged expanded assertion then passes for staged and legacy
validation. `first-broad/` preserves the preceding successful25-gate snapshot;
all broad gates were repeated after this behavioral correction.

The first corrected-source broad batch was interrupted when the user paused work
for documentation coordination (exit143; `interrupted-pause/`). Completed gate
names remain recorded, but in-memory command timings were lost and copied logs for
later gates may predate that batch. The next attempt correctly refused a generated
`webdriver.json` left behind by the termination (`stale-webdriver/`). Its contents
and timestamp matched the interrupted browser runner; no active runner remained.
The file was archived before retrying, not overwritten or treated as a user file.
The final25-gate run passes in582.89s; both unsuccessful batches remain distinct.

The public `gain_graph` example also runs successfully in debug/release, separately
from the25-gate sum, supporting the unchanged approved chapter7 walkthrough.
Documentation baseline hashes and specific change notes preserve the user's
concurrent rewrite without claiming a complete Stage9 handoff or finished book.

Nine new selected executions bring the inventory to206=128runtime+78loop, still
awaiting consolidated Stage6 Miri. Historical171/178 remains partial and cannot
certify changed executor/planner inputs. General loop graph/role-generation/media
transactions, broader mixed stress, independent loop hosts and final delivery
remain open. No stage completion or deadline guarantee follows from this slice.

## Stage 6 — Fixed-role loop facade live graph and paired configuration cuts

Based on `b071ffc4`, this slice connects the generic graph permit to the actual
loop facade. Evidence: [`live-graph-stage6.json`](evidence/live-graph-stage6.json),
[`live-graph-stage6-provenance.json`](evidence/live-graph-stage6-provenance.json),
with raw logs under `target/loop-layer-live-graph-stage6/`. This remains a scoped
Stage6 prerequisite, not complete general role-generation/media integration.

`Engine::prepare_graph_staging` opts into prepared owner banks before rendering.
The Controller retains a separate control-side Catalog and fixed decision bindings
for allocating graph compilation; stage/release use prepared normal-lane commands.
`GraphStatus`, `GraphActivate` and `TopologyGraphActivate` require empty payloads
and cannot use recovery admission. The paired operation first preflights the generic
permit and exact loop playback/capture bindings. Timing replacement is the final
fallible step; graph/epoch/retirement publication then cannot ordinarily fail.
Changed timing programs must already be stopped at delivery, before musical preview.
Same-frame Play uses the resulting configuration; deferred State/Export sees its cut.
Graph and timing retirement remain independently owned and explicitly released.

Six additional heap/destructor-guarded cases in `runtime_topology.rs` verify:

- Complete independent stereo samples, absolute MIDI, all public State fields and
  journals for paired publication/refusal/restart across whole/single/irregular
  partitions, declaration reversal and journal capacities0/1/1024. Running change
  refuses at5, Stop settles6, configuration/restart publishes7; export retains the
  original media and separate retirement returns the displaced FX graph/topology.
- Recording starts2, route changes4, playback starts8, Stop10: exact full capture
  `[2,4,12,16,4,8]` with right channel times10, coherent prefix `[2,4]`, complete
  serialized MIDI and unchanged take revision. Routing neither restarts the take
  nor relabels immutable exports.
- A program request delivered4 is refused at an actual graph change5, before the
  old program's zero7. Full original-program audio/MIDI proves no candidate action
  escaped. The request reports delivered4/actual5 and new graph/old topology epochs;
  its candidate returns Ready after finish.
- Disabled/late preparation; generic-valid but wrong/missing role bindings; wrong
  epochs/scopes/generations; ticket, recovery and unexpected-payload refusal;
  owning returned-descriptor equality and successful subsequent paired reuse.
- Sixteen occupied normal domain slots and unread results through130 one-sample
  intervals, independent recovery cancellation, stale graph release/restage and
  another activation while requests and both retired graph owners remain retained.
- Controller loss before/after delivery and processor failure after publication but
  before finish, preserving published epochs and recovering all resource/reference
  capacity without fake finish or rollback.

| Gate | Result |
| --- | --- |
| Loop native debug/release |137 passed each |
| Loop Node/Chromium package execution |137 passed each, no ignored tests |
| Workspace nextest0.9.116 |2125 passed, four unchanged skips |
| Scoped format/build/rustdoc/examples/policies/Wasm/dependency gates |19 passed |
| Production lifetime/authority prototypes |7 positive executions,18 expected compiler refusals |
| Miri supervisor/selection helper tests |14 passed; infrastructure, not interpreter evidence |
| Source provenance |110 recorded Rust inputs;107 unchanged from preceding graph prerequisite |

The19 gates total248.84s of warm local command wall time; separate proofs and the
native exact-filter mirror are not included. Standalone generic-runtime package
and generic-host reruns were not part of this scoped batch. Their preceding evidence
remains historical; workspace regressions are fresh. Concurrent documentation-worker
examples are separately identified, preserved and excluded from this implementation
commit/source-freshness scope.

`focused-first.log` retains E0614 from treating copied resource-reply fields as
references and E0533 from omitting `Record.monitor_previous`; the corrected fixture
uses the intended monitored recording. `focused-expanded.log` retains E0609/E0559
from incorrect test field names (`requested_frame/effective_frame`, `ticks` instead
of `requested/effective`, `skip`). Corrected public API spellings pass with the
same scenario workloads/assertions. Intermediate failed fixture trees were not
archived; these logs do not claim exact-tree reproduction. Initial13-case and later
17-case focused successes predate the final disabled-staging extension; only the
complete19-gate report certifies the final110-source snapshot.

Six selected executions raise the registered inventory to212=128runtime+84loop.
No interpreter ran for this smaller slice; historical171/178 remains partial.
General role-generation/media-claim transactions, broader mixed stress and complete
Stage6 Miri remain open, as do independent loop hosts and final delivery.
Documentation corrections are actionable notes in
`src/rust/rt_runtime/docs-changes-loop-layer/loop-facade-live-graphs.md`, not in-place
rewrites. Full prior-stage backfill and the separate worker's final pass remain due.

## Stage 6 — Media-owner generation publication prerequisite

Based on `229e1a13`. Evidence:
[`owner-activation-stage6.json`](evidence/owner-activation-stage6.json),
[`owner-activation-stage6-provenance.json`](evidence/owner-activation-stage6-provenance.json),
logs `target/loop-layer-owner-activation-stage6/`. The new
`media/activation.rs` implements bounded `OwnerChange` batches and an exclusive
`OwnerActivation` borrowing ledger, resource access and owning input descriptors.

Preflight checks every expected owner, unique slot, strictly newer generation,
old-role source, initial mixed-media shape/index/lease and duplicate input lease.
Free metadata must already cover all new initial media; possible old retirement
credits are deliberately not counted. Failed/unused permits preserve all owners,
inputs and committed generation watermarks. Consuming commit releases old internal
roles, binds new owners and transfers initial leases to ready media with resident
and playback references. It cannot ordinarily fail after other domain publication.
The permit does not stop loops, clean MIDI, change resolver state or claim groups.
Those integration obligations remain explicitly pending.

Old residency, pending pins, exports and unpublished reservations remain owned.
Their descriptions, cuts and bytes are not relabelled to the new generation.
Last references release each source once through existing bounded reclamation;
quiescent cleanup still recovers lost resident/reservation handles.

Three new guarded tests verify:

- Atomic mixed-media replacement of two owners with all external export/pin credits
  occupied, old playback/latest removal, retained old audio/MIDI/cuts, unpublished
  reservation retention, exact new initial selection, unused-permit retry and full
  resource/reference recovery.
- Later-input failure, old/new generation/slot mismatch, duplicate edits and duplicate
  cross-input leases; rejected inputs remain intact. Conservative free-metadata refusal
  is tested even when retiring the old role could subsequently free space.
- Independent512-edit reference model, seed0x38d1_e45f_0bc2_9a67: eight action classes
  exactly64 times, more than64 successful owner publications, expected references and
  free media/export/pin credits after every action, complete retained audio/MIDI checks,
  genuine u32::MAX logical-generation exhaustion, empty-batch and explicit cleanup.

The new production prototype executes positively and fails compilation as expected
for held-ledger access (E0502), resource mutation (E0499), input inspection (E0502),
owning input extraction (E0505) and forged private fields (E0451). Together with
refreshed prior prototypes this is8 positive executions and23 expected refusals.
The first proof runner omitted the prototype's second explicit crate dependency;
E0432/E0433 logs and the failed runner remain under `proof-first/`. Supplying both
actual crate dependencies fixes the runner, without changing the prototype or
relaxing its assertions. The exact native private-test mirror remains a native
selection check, not interpreter evidence.

| Gate | Result |
| --- | --- |
| Loop native debug/release |140 passed each |
| Loop Node/Chromium package execution |140 passed each; no ignored tests |
| Workspace nextest0.9.116 |2128 passed; four unchanged skips |
| Scoped format/build/rustdoc/examples/policies/dependency/portability gates |19 passed |
| Lifetime/authority prototypes |8 executed positives;23 expected refusals |
| Miri supervisor/selection helpers |14 passed; no interpreter invocation |
| Source provenance |111 recorded Rust inputs;108 unchanged preceding inputs |

The19 gates total248.43s of warm command wall time, excluding prototypes/exact native
mirror and failed proof invocation. Standalone generic-runtime/host execution was
not refreshed by this scoped batch; workspace regressions are fresh. Concurrent
untracked documentation examples remain separately identified and outside this
implementation commit/source-freshness scope. Production remains safe Rust and
heap/destructor guards remain enabled. Tracing inventory adds one portable ownership
module without importing application tracing.

Three new selected executions bring the inventory to215=128runtime+87loop, registered
for consolidated Stage6 Miri, not interpreted. Historical171/178 remains partial.
General facade generation/role/input/MIDI/capture integration, broader mixed stress,
complete Stage6 verification and later host/delivery requirements remain open.
The reviewed media-guide corrections and exact ownership/capacity/lifetime facts are
in `docs-changes-loop-layer/loop-media-owner-activation.md`; authored guides remain
unchanged for the separate documentation worker.
