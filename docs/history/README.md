# Historical planning and design documents

The maintained sources of truth are [PLAN.md](../../PLAN.md),
[ARCHITECTURE.md](../../ARCHITECTURE.md) and [VALIDATION.md](../../VALIDATION.md).
This directory is a read-only archive/navigation aid, not another work queue.

## Pre-consolidation snapshot

[pre-consolidation-fffc1faa.tar.gz](pre-consolidation-fffc1faa.tar.gz) preserves
**34 complete original documents byte-for-byte**, at their original repository
paths, from source `fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a` (2026-10-29
consolidation). Every archive member was compared with both the original working
file and that committed version before replacement. No evidence result, acceptance
criterion, completed checklist, failure, timing or historical scope is rewritten
inside the snapshot. It contains narrative documents, not source code or a new
verification campaign.

For readable originals with working source-revision links, browse the
[immutable Git snapshot](https://github.com/SanderVocke/shoopdaloop/tree/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a).
Offline, use `git show fffc1faa:PATH` or extract the bundle into a **separate empty
directory**, never over the current checkout:

```sh
mkdir -p /tmp/shoop-document-history
# From the repository root:
tar -xzf docs/history/pre-consolidation-fffc1faa.tar.gz -C /tmp/shoop-document-history
git show fffc1faa:PLAN_LOOP_LAYER.md
```

The extracted documents retain their original relative-link base; use the full
Git snapshot for historical source/evidence targets not included in the bundle.
Old paths in the current checkout are compatibility redirects with legacy anchors,
not maintained copies. The old ARCHITECTURE.md is in the bundle; its current path
contains the unified architecture instead of a redirect.

| Original document(s) in the bundle | Current home |
| --- | --- |
| `ARCHITECTURE.md`, `PROPOSAL_RT_ENGINE.md`, `DESIGN_RT_ENGINE.md`, `INTER_LOOP_PROCESSING.md`, `PROPOSAL_LOOP_LAYER.md` | Architecture: implemented stack, constraints and loop semantics |
| `CRITIQUE_LOOP_LAYER_ARCH.md`, `PROPOSAL_TRANSPORT.md` | Architecture: explicitly proposed composability/protocol directions |
| `PLAN_LOOP_LAYER.md` | Plan: unchanged LL-A1–LL-A12, stage state, all nine original unchecked rows and final gates |
| `PLAN_MILESTONE_1.md`, `METAPLAN_SHOOP_INTEGRATION.md` | Plan: delivered narrow composition scope, retained original M1 criteria and proposed next work |
| `DONE_INITIAL_CRATE.md`, `bus-fx-switch-fix-plan.md` | Plan/validation: historical delivered work; original full acceptance records stay archived |
| `DOCS_REWRITE_PLAN.md`, `DOCS_MERGE_UPDATES_PLAN.md` | Plan/validation: completed documentation work, not an authoring backlog |
| `PI_GOAL_X.md` | Architecture: plan/executor/auditor division; full installed-version review stays historical |
| `docs/loop_layer/SEMANTICS.md`, `RUNTIME_INTEGRATION.md`, `FACADE_DESIGN.md`, `TOPOLOGY_ACTIVATION_DESIGN.md`, `MIDI_INDEX_DESIGN.md`, `CAPTURE_TRANSACTIONS_DESIGN.md` | Architecture: consolidated contracts; current public guides remain separate |
| `docs/loop_layer/VALIDATION.md`, `MIRI_JOBS.md`, `STAGE6_READINESS.md`, `TEST_EXECUTION_TIMES.md` | Validation: current verification/debt with original chronological records retained here |
| `docs/rt_runtime/CONTRACTS.md`, `RESOURCES.md` | Architecture and existing runtime book/reference |
| `docs/rt_runtime/AUDIT.md`, `VALIDATION.md`, `PERFORMANCE.md` | Validation and unchanged raw runtime evidence |
| `docs/rt_runtime/GUIDE.md`, `HOST_FIXTURE.md` | Existing runtime book/fixture README; historical prose retained here |
| `docs/integration/PARITY.md`, `MILESTONE_1.md` | Architecture compatibility map; plan/validation integration scope and evidence |

## Material deliberately left in place

Raw JSON/CSV/log/archive evidence and the 31 loop milestone reports remain at their
existing `docs/*/evidence/` paths, unchanged. Their source/run identity is not
relabelled as validation of this migration or today's merged source.

The runtime book, current loop guides, fixture/crate READMEs, draft `docs-new/`
book, `.agents/` policy, tracing inventory and Miri operating procedures remain
separate references. Existing `docs-previous/implementation-review/`, completed
documentation reference and `.pi/docs-rewrite-evidence/` remain historical; no
baseline or handoff workflow is restarted. Native `.pi/` objectives, events,
checkpoints and lifecycle state are not edited by document consolidation.

Superseded words such as “current”, “waiting”, “not implemented” and old commit/
pause directions apply to their original snapshots. Consult the canonical plan
and validation before interpreting them as present work or acceptance. In
particular, Milestone 1's narrow consolidation is delivered without claiming its
old full final acceptance, and the expanded loop Miri campaign remains failed.
