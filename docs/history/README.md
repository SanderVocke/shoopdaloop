# Historical planning and design documents

Current work and design live in [PLAN.md](../../PLAN.md),
[ARCHITECTURE.md](../../ARCHITECTURE.md) and [VALIDATION.md](../../VALIDATION.md).
Everything indexed below is historical reference, **not a parallel work queue**.

## Readable pre-consolidation history

The [pre-consolidation snapshot](pre-consolidation-fffc1faa/README.md) contains
readable versions of all **34 original planning, design and progress documents**
from `fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a`.
Each page is labelled historical. Its original text is retained, with relative
link destinations adapted: links within the snapshot stay local; source, guide
and evidence links outside it point to that immutable Git revision. Old “current”,
“waiting”, “not implemented”, pause and delivery statements describe that snapshot,
not today's source or an instruction to restart its work.

The obsolete root and subordinate redirect pages have been removed. The one
exception is `docs/loop_layer/MIRI_JOBS.md`: a small compatibility pointer preserves
links in unchanged raw evidence reports. It is not another ledger. Use the three
canonical documents for current status and the paths below for detailed history.

### Root plans, proposals and decisions

| Historical document | Current interpretation / home |
| --- | --- |
| [ARCHITECTURE.md](pre-consolidation-fffc1faa/ARCHITECTURE.md) | Original partial stack overview; unified architecture is at the repository root |
| [PROPOSAL_RT_ENGINE.md](pre-consolidation-fffc1faa/PROPOSAL_RT_ENGINE.md) | Runtime rationale and optional migration direction; initial runtime is delivered |
| [DESIGN_RT_ENGINE.md](pre-consolidation-fffc1faa/DESIGN_RT_ENGINE.md) | Runtime design/evolution history; current contracts are in architecture and the public book |
| [DONE_INITIAL_CRATE.md](pre-consolidation-fffc1faa/DONE_INITIAL_CRATE.md) | Completed initial-runtime criteria, stages and delivery history |
| [INTER_LOOP_PROCESSING.md](pre-consolidation-fffc1faa/INTER_LOOP_PROCESSING.md) | Accepted sample-exact timing/termination decision, preserved in current architecture |
| [PROPOSAL_LOOP_LAYER.md](pre-consolidation-fffc1faa/PROPOSAL_LOOP_LAYER.md) | Loop rationale/scope; implementation and incomplete final acceptance are distinguished in the current plan |
| [PLAN_LOOP_LAYER.md](pre-consolidation-fffc1faa/PLAN_LOOP_LAYER.md) | Original LL-A1–LL-A12, stage checklist, evidence and execution history; unfinished checks remain in the current plan |
| [CRITIQUE_LOOP_LAYER_ARCH.md](pre-consolidation-fffc1faa/CRITIQUE_LOOP_LAYER_ARCH.md) | Composability assessment and proposed subsystem/optional-facade direction; not an adopted refactor |
| [PROPOSAL_TRANSPORT.md](pre-consolidation-fffc1faa/PROPOSAL_TRANSPORT.md) | Proposed modular transport/protocol; not an implemented service deployment |
| [METAPLAN_SHOOP_INTEGRATION.md](pre-consolidation-fffc1faa/METAPLAN_SHOOP_INTEGRATION.md) | Strategic integration sequence; native first slice now delivered, later work remains proposed |
| [PLAN_MILESTONE_1.md](pre-consolidation-fffc1faa/PLAN_MILESTONE_1.md) | Original M1-A1–M1-A10 and stages; narrower consolidation delivered, full original acceptance unclaimed |
| [DOCS_REWRITE_PLAN.md](pre-consolidation-fffc1faa/DOCS_REWRITE_PLAN.md) | Completed book/reference/guide authoring and scope-change record, not a backlog |
| [DOCS_MERGE_UPDATES_PLAN.md](pre-consolidation-fffc1faa/DOCS_MERGE_UPDATES_PLAN.md) | Completed documentation correctness merge/history |
| [bus-fx-switch-fix-plan.md](pre-consolidation-fffc1faa/bus-fx-switch-fix-plan.md) | Delivered native regression fix, PR #873 |
| [PI_GOAL_X.md](pre-consolidation-fffc1faa/PI_GOAL_X.md) | Completed plan/executor/auditor review; replacement tooling remains proposed |

### Loop contracts and verification history

| Historical document | Current home |
| --- | --- |
| [SEMANTICS.md](pre-consolidation-fffc1faa/docs/loop_layer/SEMANTICS.md) | [Loop semantics](../../ARCHITECTURE.md#loop-semantics) |
| [RUNTIME_INTEGRATION.md](pre-consolidation-fffc1faa/docs/loop_layer/RUNTIME_INTEGRATION.md) | [Runtime/planner contracts](../../ARCHITECTURE.md#accepted-architectural-constraints) |
| [FACADE_DESIGN.md](pre-consolidation-fffc1faa/docs/loop_layer/FACADE_DESIGN.md) | [Media/capture](../../ARCHITECTURE.md#resources-capture-and-media) and [publication](../../ARCHITECTURE.md#prepared-publication-and-teardown) |
| [TOPOLOGY_ACTIVATION_DESIGN.md](pre-consolidation-fffc1faa/docs/loop_layer/TOPOLOGY_ACTIVATION_DESIGN.md) | [Prepared publication](../../ARCHITECTURE.md#prepared-publication-and-teardown) |
| [MIDI_INDEX_DESIGN.md](pre-consolidation-fffc1faa/docs/loop_layer/MIDI_INDEX_DESIGN.md) | [Indexed resources](../../ARCHITECTURE.md#resource-ownership-and-indexed-access) |
| [CAPTURE_TRANSACTIONS_DESIGN.md](pre-consolidation-fffc1faa/docs/loop_layer/CAPTURE_TRANSACTIONS_DESIGN.md) | [Coordinated capture](../../ARCHITECTURE.md#coordinated-capture) |
| [VALIDATION.md](pre-consolidation-fffc1faa/docs/loop_layer/VALIDATION.md) | [Loop evidence](../../VALIDATION.md#loop-evidence); complete chronology and exact-case map stay historical |
| [MIRI_JOBS.md](pre-consolidation-fffc1faa/docs/loop_layer/MIRI_JOBS.md) | [Miri evidence/debt](../../VALIDATION.md#miri-evidence) |
| [STAGE6_READINESS.md](pre-consolidation-fffc1faa/docs/loop_layer/STAGE6_READINESS.md) | Closed source-scoped Stage 6 checkpoint |
| [TEST_EXECUTION_TIMES.md](pre-consolidation-fffc1faa/docs/loop_layer/TEST_EXECUTION_TIMES.md) | Historical scheduling observations, not acceptance thresholds |

### Runtime and integration history

| Historical document | Current home |
| --- | --- |
| [CONTRACTS.md](pre-consolidation-fffc1faa/docs/rt_runtime/CONTRACTS.md) | [Architecture](../../ARCHITECTURE.md) and public runtime book/reference |
| [RESOURCES.md](pre-consolidation-fffc1faa/docs/rt_runtime/RESOURCES.md) | [Resource/capture contracts](../../ARCHITECTURE.md#resources-capture-and-media) |
| [AUDIT.md](pre-consolidation-fffc1faa/docs/rt_runtime/AUDIT.md) | Source-scoped initial audit, indexed by current validation |
| [VALIDATION.md](pre-consolidation-fffc1faa/docs/rt_runtime/VALIDATION.md) | Initial-runtime chronological evidence, not later merged-head certification |
| [PERFORMANCE.md](pre-consolidation-fffc1faa/docs/rt_runtime/PERFORMANCE.md) | Initial measurements; current observations in [validation](../../VALIDATION.md#performance-observations) |
| [GUIDE.md](pre-consolidation-fffc1faa/docs/rt_runtime/GUIDE.md) | [Runtime book/reference](../../src/rust/rt_runtime/docs/README.md) |
| [HOST_FIXTURE.md](pre-consolidation-fffc1faa/docs/rt_runtime/HOST_FIXTURE.md) | [Runtime fixture README](../../tests/rt_runtime_host/README.md) |
| [PARITY.md](pre-consolidation-fffc1faa/docs/integration/PARITY.md) | [Compatibility/ownership map](../../ARCHITECTURE.md#compatibility-and-integration-direction) |
| [MILESTONE_1.md](pre-consolidation-fffc1faa/docs/integration/MILESTONE_1.md) | [Native slice scope](../../PLAN.md#native-composition--milestone-1) and [evidence](../../VALIDATION.md#integration-evidence) |

## Retired experiments

| Experiment | Last version | Superseded by |
| --- | --- | --- |
| Milestone 1 composition spike | [`experiments/milestone_1_composition`](https://github.com/SanderVocke/shoopdaloop/tree/1868baf881395c4682ed0519a2970518b8cc3185/experiments/milestone_1_composition) | `shoop_example_dsp` and `shoop_engine_service`, rebuilt on the composable loop library ([refactor plan](../../PLAN_REFACTOR.md)) |

The spike showed that independent DSP could run in the loop engine's graph by
carrying its own control bridge beside the runtime. Once processors gained their
own scheduled operation family, that bridge became unnecessary.

## Exact originals

[pre-consolidation-fffc1faa.tar.gz](pre-consolidation-fffc1faa.tar.gz) retains all
34 original files **byte-for-byte**, at their original repository paths, including
all criteria, completed/unfinished checkboxes, failures, scope changes and timing
records. This existing bundle is unchanged by the readable-history cleanup.
The readable views above adapt navigation only; they are not claimed byte-identical.

For the complete original source/link context, use the
[immutable Git snapshot](https://github.com/SanderVocke/shoopdaloop/tree/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a).
Offline, extract the bundle into a separate empty directory, **not over this checkout**:

```sh
mkdir -p /tmp/shoop-document-history
# From the repository root:
tar -xzf docs/history/pre-consolidation-fffc1faa.tar.gz -C /tmp/shoop-document-history
git show fffc1faa:PLAN_LOOP_LAYER.md
```

## Retired execution context

The user explicitly decided not to resume the old loop goal and to continue
elsewhere. The former executor's objectives and lifecycle are history, not an
active dependency, a completion claim or an instruction to resurrect that goal.
The unfinished verification/acceptance requirements remain visible in the current
plan and validation so a separately authorized continuation can assess them.

Native records have not been manually moved, edited, completed or cleared:

| Historical goal | Stored state at consolidation | Interpretation |
| --- | --- | --- |
| `mvqjv1pa-kwtmu3` | Paused by user | Old loop executor context; user will not resume it as-is |
| `mvo6lnim-1xzkd7` | Paused | Earlier loop predecessor |
| `mvps85i2-uuvn9n` | Paused | Old documentation objective with superseded workflow contracts |
| `mvrpaoj0-u4f0gj` | Blocked | Original M1 dependency-wait record predates the delivered narrow consolidation |

These are historical descriptions, not a second task tree. Native lifecycle
changes belong to normal user/native controls, not raw JSON/event/checkpoint edits.
Old plan paths are intentionally removed; no root file is retained just to keep
an abandoned executor's references alive.

## Material deliberately left in place

Raw JSON/CSV/log/archive evidence and loop milestone reports remain at their
existing `docs/*/evidence/` paths, unchanged and tied to their recorded sources.
Their source paths, hashes and failures are not relabelled as current validation.

The runtime book, loop guides, fixture/crate READMEs, draft `docs-new/` book,
`.agents/` policy, tracing inventory and Miri operating procedures remain separate
references. Existing `docs-previous/` and `.pi/docs-rewrite-evidence/` are already
historical; no authoring, baseline, handoff or documentation-audit workflow is
restarted. The `.pi/` records are left to native lifecycle management.
