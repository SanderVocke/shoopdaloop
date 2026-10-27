# Repository runtime contracts, audit and evidence follow-ups

Reviewed all seven destinations in docs/rt_runtime named by Stage9, plus the current
crate README and independent generic-host README. Hashes and evidence-integrity
checks are in repository-baseline.json. These are publication instructions, not a
new callback safety audit or an authored replacement guide. Preserve concurrent
edits, historical reports and DONE_INITIAL_CRATE.md. Re-read destination drift first.

## CONTRACTS.md — distinguish current contracts from initial history

The current draft already includes graph activation permits and startup/extraction;
preserve those concurrent additions. Correct the opening loop-facade-pending claim
and the final general loop graph/role-transactions-outstanding claim with separately
attributed prepared-template facade evidence. Keep the generic crate independent.

The initial table deliberately retains split-credit/min(..., subranges-1) history,
but mixes it with newer transaction wording. Clearly label the historical scheduling
and quota rows or split historical/current tables. Current normal credits are
min(scheduled_events, budgets.events), with one separate configured-planner recovery
receipt; subranges is ignored. Internal positive intervals are not external events.
Runtime delivery success, domain outcome and reliable owner return are different.
Planner actions precede processor targets; per-target action/key/sequence ordering
must not be abbreviated to a misleading global FIFO across recovery/normal lanes.

Use the current four work classes plus diagnostics from chapter17 notes: required
RenderIo/index/group work is counted and storage-bounded, not throttled by transfer
credits. Legacy APIs and optional exports retain quotas. Identity/frame/revision
exhaustion is checked; work counters saturate. A planner's finite structural work
can exceed a deadline, and generic safe Rust does not establish arbitrary planner
termination. Capture prefix bounds and no whole-content seek/freeze scan remain.

Link current published chapter15 only when it exists; transitional archived guide
links must not imply the book is finished. Production graph/view prototypes and
historical simplified interval prototypes have different evidentiary scope. Keep
A1–A8 historical; LL-A1–LL-A12 remain the current complete loop acceptance criteria.

## RESOURCES.md — actual access, capture and extraction paths

- Replace loop-adaptation-pending with the facade attribution, not new loop semantics
  in the generic store. The introductory14-cut ownership proof was historical;
  current tests/capture_group_disconnect.rs declares17 lifecycle cuts after prepared-
  first-interval support. Cite prepared-interval/facade-preparation Stage5 evidence.
- Distinguish raw ResourceId/SnapshotId same-store caller discipline from scoped
  ReadIndex/cursors/MIDI events. Copying any handle does not acquire a lease. State
  the fixed versions×(1+C) projection storage and C×K−1 packet limit explicitly.
- Explain PreparedReserved and reserve_prepared_capture_interval before activation,
  exact stamped lifecycle authority, one staging contribution per member and common
  prefix commit. Unexpected staging/producer faults are not ordinary late exhaustion.
  Single-resource append alone is not multi-member transaction authority.
- Qualify callback-boundary snapshot/switch wording: legacy commands run at ingress;
  planner lifecycle and coherent domain cuts may occur at exact internal boundaries.
  Required freeze consumes reserved final leases; optional active-prefix snapshots
  can refuse for background Budget without stopping capture. Three explicit staging/
  commit/read media copies must not be described as zero-copy.
- into_resources discards the planner; domains with cleanup ledgers require
  into_planner_and_resources. Explain this choice before the teardown recipe. Plain
  object Drop cannot release resource IDs hidden in a discarded domain ledger.

Keep the page/version/lease ownership and incremental reclaim description: a prefix
lease may conservatively retain later pages of its version; immutable snapshots do
not imply selective page reclamation or infinite recording. The document's initial
Stage5 evidence list remains historical, with new behavior linked separately.

## GUIDE.md — user-facing compatibility and task links

Retain gain_graph, capacity formulas and prepared-ingress advice. Add planned_gain
as the generic interval example and clarify that its current32sample/five-interval
assertions are unguarded and one-consumer. New guarded/multi-consumer and pending-
receipt tutorials remain a documentation-worker task; a successful cargo run is not
proof of an example it does not contain. Loop coordinated_loop belongs to the other
crate and is not evidence of generic dependency contamination.

Replace full-loop-integration-pending with a link to the separate facade and its
remaining independent-host obligations. Add initial graph epoch1/frame0 preparation,
legacy versus staged graph publication, recovery preemption of held validation and
permanent domain quiescence after partial publication. Existing budgeted stereo
2048byte examples are legacy I/O sizing, not required-render throughput limits.

The support matrix records the initial runtime's verified CI/platform runs. Label
its source/run provenance; do not present those green cells as current loop-branch
Windows/macOS or final-head CI. Keep Node/Chromium package tests distinct from real
worklets, generic worklets distinct from the pending loop host, and device adapters
excluded. Historical platform/browser versions are not a claim of current toolchain
availability on every machine.

## AUDIT.md — stale bounds and required new audit work

The current title says Final callback audit but its run_scheduled and configuration
paragraphs describe the initial split-bound runtime. Preserve that historical audit
as scoped evidence; add a separately identified current audit when Stage8 source
review is actually performed. This documentation pass does not certify all new
callback paths merely by inspecting the old table.

Concrete current corrections and audit rows to verify:

| Path | Current argument and required inspection |
| --- | --- |
| executor::run_scheduled | At most host frames positive intervals, not effective_events+1. Due external work is finite; Planner::plan/finish and its application termination contract are additional. Validate positive end and recovery terminal at delivery frame before consumers. |
| Planner decision/observation views | Fixed storage, validated binding and per-consumer observation capacities; no retained views across mutation. No second mutable timing authority. |
| Transport held normal | One inline parked owner, callback-snapshotted recovery, preserved validator progress and original cancellation observation; no extra credit, dequeue or producer-chasing loop. |
| Graph stages/activation | Fixed owners/actual vector capacities, checked scope/generation/epochs, finite prepared-size pending-MIDI preflight; unused permit inert and final owner swap infallible. No callback plan destructor. |
| Required media/index access | Balanced page index maintained during append, scoped cursors, fixed checkpoint replay span; independent saturating counters, no whole-history scan or reset of background allowance. |
| Capture groups | Claims, prepared versions/index/final leases, first-interval and continuing reservations, fixed staging, common prefix and truthful interface faults; cancellation and stopped recovery of partial stages. |
| Loop planner/resolver | Separate domain audit: finite SCC/action settlement, latched-failure retries, authoritative discontinuities, fixed pending slots and journals, coherent queries and owning configuration publication. Not a generic runtime dependency. |
| Loop consumers/roles | Prepared boxed MIDI projections, port-window conversion, logical progress after sink failure, explicit input reset policy, checked-generation observations and no hidden final-owner Drop. |

Delete no historical observation to hide a changed contract. The old aggregate
worst-case diagnostic-counter rejection is no longer current: checked allocation
sizes remain, while counters saturate. Heap guards prove only observed heap behavior;
inspect locks/waits/I/O/retries/destructors independently. Existing prototype successes,
Miri selections and source review notes do not substitute for the required Stage8
full callback-reachable audit, models, canaries and measured workload campaign.

## HOST_FIXTURE.md and tests/rt_runtime_host/README.md

Preserve the independent generic fixture, NoPlanner/RTR1/RTJ1 protocols, bounds,
stop/dispose and actual-Web-Audio oracle. Do not add loop semantics to its codec or
replace the recorded journal with a Rust-generated surrogate. Native-only replay is
not fresh browser execution; --no-build still runs the browser unless native-only.
Keep exactly representable samples and acknowledged live timing rules explicit.

The fixture status has an old stages7–8-pending phrase although initial-runtime CI
was later verified; reconcile it with dated evidence, not the loop plan's stage
numbers. Fix the trivial preserved.s typo in the fixture README during publication.
The original1,507,328-byte memory measurement is historical; subsequent generic-host
regression recorded1,703,936bytes after index/storage additions. Record each with
its source/run; neither is the future loop host's memory requirement. The uncompressed
golden journal hash remains fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237.

The loop fixture README/wire/journal guide cannot yet be completed: its Stage7
implementation is pending. Carry forward explicit bounds, instance routing, typed
receipt/admission/terminal distinctions, local generation mappings, ordinary separate
Wasm memories, guarded ingress/render/recycling, memory-growth checks and real failure
canaries. A LTP1 topology body is not that complete host envelope or proof of ownership.

## PERFORMANCE.md and VALIDATION.md — preserve measured provenance

Keep all seven-workload420,000-callback results, raw source/config/toolchain/hardware,
preliminary90,159ns outlier, OS-counter limitations, zero-observed-miss qualification
and expected failures unchanged. They measure the initial native core, not new
planner/group/loop work. Existing bounded_runtime capture uses legacy quota-based
I/O. Later1024-callback documentation smokes are not a substitute full campaign.
Future benchmark_loop_engine must report its own workloads, scaling, correctness,
storage/copies/counters/tails/canaries and environment, without a speedup inference
from unchanged warm reruns or wall-clock pass thresholds on shared CI.

VALIDATION.md is a chronological initial-runtime record. Preserve all dated commands,
counts, unsuccessful attempts, old design filenames, interpreter selections, PR878
and source-head CI/review findings with their original scope. Add a current pointer
to docs/loop_layer/VALIDATION.md/MIRI_JOBS.md instead of rewriting historical93/89/38
counts to current numbers. Historical no-reviewer status is not a new review check.
Recheck parent/base and final PR head/attempt before delivery; the old green run
cannot certify this branch. Initial-runtime Stage6/7/8 labels are not loop-plan
stage completion. Never turn the historical interrupted171/178 interpreter result
or current228-case running selection into a collected pass.

## Current crate README/navigation

The README already reflects generic planning, required resources and graph permits;
preserve those concurrent edits. Replace only the stale loop general-transactions-
pending qualifier with the prepared-template scope. The book has populated draft
chapters1–4/7, not solely an empty ordered structure, but14chapters remain pending at
this review. Preserve transitional archive links until the publication worker has
completed the actual book and its checks. Do not recreate deleted flat guides or the
absent former src/rust/rt_runtime/CONTRACTS.md. Keep workspace-managed packaging and
conditional callback safety explicit. Crate rustdoc's stale split-credit/all-I/O-quota
summary remains a separately recorded source-prose correction, not fixed by a clean
rustdoc build.
