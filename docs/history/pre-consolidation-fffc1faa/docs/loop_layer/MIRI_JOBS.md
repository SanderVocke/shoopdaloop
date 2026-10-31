> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/MIRI_JOBS.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Miri jobs and outstanding verification

This is an implementation job/evidence history, not a live-process monitor or a
requirement to run Miri for prose acceptance. All source-match statements below
refer to their recorded snapshots/checkpoints, not the current dirty checkout.
The [documentation plan](../../DOCS_REWRITE_PLAN.md) requires no new interpreter
campaign; [current public guides](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_loop_engine/README.md)
distinguish examples from this historical gate. Later implementation acceptance
obligations remain unchanged.

## Current Stage 8 model registration and verification debt

The [broader models](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/adversarial-models-stage8.md) append four exact seed0
commands to the preceding [descriptor-bank selection](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/descriptor-model-stage8.md):
**151 commands/304 executions =138runtime+102loop+64host**. Removing only those four
commands reproduces the complete prior147/300 inventory as an exact prefix;
every previous case/filter/count/seed and relative order is retained. Three new
Rust test files have full-length native/Wasm workloads with no Miri-only shortening.
Native193runtime/155loop/60fixture tests/profile,189runtime+155loop per Node/Chromium,
eight exact native dispatches,73 helpers and2148 workspace passes/four existing
skips pass. The broader model row is verified: Stage8 is6/8.

**Complete applicable Stage8/final interpretation remains unverified.** The frozen
Stage7 job passed and was collected at146/299; it cannot certify the five later
cases. The accumulated Stage8 selection has **failed on its final sync-model900s
watchdog**, with150commands/303 partial passes, not304 acceptance. Complete
workloads/guards/seeds/source isolation remain unchanged; no routine invocation
was launched per helper. Stage7 stays7/8, Stage8 stays6/8 and current interpreter/
remote rows remain open. Stage9 is verified; final-head CI/review and Stage10 remain.

### Verified terminal failure and exact censored diagnosis

[Full failed-job/diagnostic proof](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/miri-sync-model-failure-stage8.md):
`20261028T190146-abeee2e59e`, source `8633d0f0`, failed/exit1 at
2026-10-28T22:47:06.723671Z after13519.300086269039s supervisor wall. All890 source
entries/literal archive/copied supervisor/full151/304 inventory/strict flags and
150 ordered raw passing summaries match. Both recorded processes are dead; no
final report/collection exists and supported collection refuses. Last case is the
full `seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals`,
seed0, unchanged900s. No passing count is inferred for that timed-out command.

Actual failure justifies one exact diagnostic: `20261028T225455-65cf2c5a28`, same
frozen source/archive/890 entries, `--shard151/151 --measureme`, one full command/
seed0 and only declared seed/measureme flags. Identity/source/cwd/flags/ancestry-
checked SIGINT to interpreter228615 finalizes a censored profile after useful app
compiler gates. Failed/exit1/zero pass,22:54:56.450028–23:01:43.962404Z,
407.5124054910266s; recorded processes dead and collection refused. A first cwd
preflight correctly refused before signalling; it was corrected to Cargo's exact
frozen package directory, without relaxing the other checks. Original reports are
unchanged; pinned summarize11.0.1 analyzes an equal-hash copy outside them.

The1555724199byte finalized profile SHA256 is
`ece35f348124a4c477a93ddbe12c0ce529be1202b2aa166910df74c97914dff1`.
371.134845623s completed-function-event time contains overlapping `Boundary::settle`
285.561397094s/1694calls and `PreparedTopology::node`193.840898050s/155061calls.
This supports investigating full-identity-checked direct-slot lookup with unchanged
sparse/stale fallback, **not** shortening the model, raising900s or claiming native
speedup/recovered interpretation. Candidate implementation/recovery remains next.
The661316byte/192file archive retains complete failed logs/progress and diagnostic
metadata/summaries. Fresh [local hosts/benchmarks/application](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/final-hosts-benchmarks-application-stage10.md)
pass at85e36348 with509 scoped inputs unchanged; they do not replace this failure.

### Historical consolidated Stage8 launch — not passing evidence

Job **`20261028T190146-abeee2e59e`** was launched from committed candidate
**`8633d0f0754a2f4445c3d0febb1f1ae2d7e9572e`**, started2026-10-28T19:01:47.423617Z:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-8-consolidated --commit 8633d0f0754a2f4445c3d0febb1f1ae2d7e9572e
```

The19:06:19Z launch proof verifies all890 source-manifest entries and committed
literal archive re-export, copied supervisor, complete151/304 JSON-normalized
inventory, strict Rust/rustdoc/source environment and isolated build. All214 scoped
native-model implementation/test/dependency/configuration inputs match the frozen
source. Recorded supervisor117885 and runner117886 identities are live at that
checkpoint;`.pi/` is excluded. The result is **running**, not passed or collected.
No completion count is inferred from launch. Old Stage7 recovery is already
collected and its recorded identities are dead; this is not a duplicate active job.

- Root: `target/loop-layer-miri-jobs/20261028T190146-abeee2e59e/`.
- Job SHA256: `a139dfecc8735b5fdeab5f752d22364b06acb599fbc67a25576b2dd792075100`.
- Source archive SHA256: `8c009c30678bbb9d11653b4f0699be7c4e2f9b6f3c4c4bc887f9f4b9f0aa525f`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Proof/command: `target/loop-layer-consolidated-stage8/launch-verification.json`
  and `commands/verify-launch.py` beneath the same directory.

Accumulated descriptor/timeline/receipt/capture additions after the frozen Stage7
snapshot require this applicable stage selection, not a run per helper. Continue
final examples and LL-A acceptance work while it runs. Do not mutate or duplicate
the job. An eventual pass can serve final interpreter acceptance only after full
terminal collection and exact relevant final-source/configuration/inventory
comparison; it cannot certify final-head CI or review.

### Stage10 native/Wasm checkpoint while this job runs

Supported status at2026-10-28T20:31:19.715900Z still reports **running**, frozen
`8633d0f0`, full151/304 selection; no completion count is inferred. Final native/
complete19-package Wasm execution at `24b458ad` verifies all509 scoped code/test/
dependency/configuration/runner inputs equal that frozen source before/after.
See [native/Wasm evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/final-native-wasm-stage10.md). This comparison
supports possible reuse only after genuine complete terminal collection and exact
final inventory/toolchain/flag/source checks. It is not interpreter acceptance.

The final native preflight caught an older PATH mistake: the pinned nextest binary
is directly under `$HOME/.cache/rt-runtime-nextest-0.9.116`, not its nonexistent
`bin/`. Prior raw0.9.143 invocations remain0.9.143 despite erroneous historical
metadata; the fresh complete workspace/discovery uses checksum/version-verified
0.9.116. Cargo-test/Miri use separate tools; recorded nightly interpreter evidence
is unchanged. Failed preflights are retained. No safety flag,900s watchdog,
workload/guard/seed or interpreter job is changed for this evidence-only milestone.
Stage9 is verified; current interpreter/remote acceptance and Stage10 remain open.

## Current Stage 7 verification debt

### Consolidated recovery passed and collected

[Verified complete collection](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/miri-consolidated-stage7.md):
`20261028T153110-784d2380c0`, immutable `3bf1dbca`, **146 commands/299 executions**,
ended2026-10-28T18:25:48.965005Z after10478.51554903097s supervisor wall time.
All878 entries, literal committed source re-export, copied supervisor, complete
inventory/strict flags,147 report/raw-log hashes, dead recorded process identities
and supported collection match. Earlier failed jobs remain failed. This closes the
frozen interpreter recovery, not the later151/304 selection, remote artifacts or
final-head CI/review. Stage7 therefore remains7/8. The following entries preserve
the earlier checkpoints rather than overriding this terminal result.

### Earlier checkpoint: exact bulk recovery collected; full selection pending

Job **`20261028T145739-89dfd53bdb`**, frozen
**`3d10c7a06f1e4ad01185d047e4a1c44abf525629`**, ended **passed/exit0** at
2026-10-28T15:10:30.291589Z and supported collection succeeded. Its unchanged full
host10/63 test completed one command/one execution, seed0; all seven internal
replays/partitions, assertions, guards and900s watchdog remain. Sole interpreter
flag: `-Zmiri-seed=0`. Supervisor monotonic wall time is770.7792120630038s;
command wall time is768.9439976660069s. Miri's simulated harness time is not wall time.

Post-terminal verification matches all876 immutable entries, source archive/copied
supervisor, exact inventory and report/log hashes; both recorded process identities
are dead. See [recovery/tracing evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/miri-bulk-recovery-tracing-stage8.md)
for the raw collected archive. Report SHA256:
`23cf65e9d6bba1f59db33fe949b7a8c8a1d41fd7daa70877928ed33c39d8d73a`;
raw log SHA256: `49fe557d99a10d0ef95536ac1dfac2443534ba7186cd541b77faa0c8af2e5509`;
collected SHA256: `8fe42d40d987d3209074a74554bc81d7fa404ade6fa63006b47604e91e44200b`.

This resolves the exact failure only. Earlier failed/interrupted jobs remain failed;
there is no paired speedup or CI-margin claim. **Complete frozen Stage7 coverage,
146commands/299executions (136runtime+100loop+63host), remains required**, including
the bulk-buffer model, plus remote source-matched artifacts. Stage7 stays7/8.
At that collected-recovery checkpoint Stage8 was4/8 because fresh tracing/boundary/
test-policy verification closed its inventory row; callback audit and broader
models were then open. Later audit/model status is recorded above.

The original consolidated failure and subsequent relevant runtime dependency
changes require complete Stage7 regression coverage, not another routine exact
retry. With implementation ready and the failed command recovered, launch one
required consolidated recovery from a committed frozen snapshot, then continue
Stage8 work while it runs. Prose/inventory verification alone is not its rationale.

### Historical launch checkpoint — not then passing evidence

Required consolidated recovery **`20261028T153110-784d2380c0`** launched from
**`3bf1dbca7c93f486be50cd7359555172dad723f3`** at
2026-10-28T15:31:10.449476Z, supervisorPID32802:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-consolidated-recovery --commit 3bf1dbca
```

It selects the complete current **146 commands/299 executions =136runtime+
100loop+63host**. All878 immutable entries and the copied supervisor match, `.pi/`
is excluded, complete inventory equality is verified, and warning-denied Rust/
rustdoc/source-commit environment and isolated build match. No profiling or safety
override is enabled; seed flags follow the unchanged declared inventory. Both
recorded supervisor/runner identities are live at this launch checkpoint.

The original consolidated failure, relevant runtime dependency changes and now
collected exact recovery justify this complete stage gate; prose/trace-inventory
verification alone would not. The checkpoint is **running**, not passed. Retain
900s per-command watchdog, full workloads/guards and terminal collection. Continue
Stage8 implementation/audit rather than poll or launch a routine duplicate.

- Root: `target/loop-layer-miri-jobs/20261028T153110-784d2380c0/`.
- Job SHA256: `2ef0873da407abdeb054ecdd5e8c595d3f6fa36ab1fb10af3cd7400ea83bfc2b`.
- Source archive SHA256: `20d8a871153e2f37c871360738b5125344d00fc7402fe3ffa49415c7b369780c`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Launch proof: `target/loop-layer-consolidated-recovery-stage7/launch-verification.json`.

The initial proof helper compared decoded JSON lists with Python tuples and refused
before writing a proof. Full JSON-normalized equality then passed, retaining every
field; no job, source, selection or runner changed. This helper correction is not
an interpreter failure or a passing result. The preceding recovery archive predates
this new launch and cannot certify it. Stage7 stays7/8; remote artifacts and later
final-head CI/review remain mandatory even if this consolidated job eventually passes.

The following entries preserve historical failures and launch checkpoints.

### Original consolidated job failed; full recovery required

Job **`20261028T083225-374aa055bf`** launched at2026-10-28T08:32:25.969265Z
from immutable commit **`8242de59fbb20ff17eb9724fe154c36aedb257e6`**:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7 --commit 8242de59
```

Expected scope:144 commands/291 executions (128 runtime+100 loop+63 host), pinned
nightly-2026-05-04, unchanged900s per-command watchdog, full host workloads and
guards. Job root: `target/loop-layer-miri-jobs/20261028T083225-374aa055bf/`.
Post-launch verification found the supervisor running, matched all856 source
entries and the copied supervisor, confirmed both workspace manifests/lockfiles,
native support/oracles and dev allocator inputs, and found no `.pi/` export.
The exact package-to-manifest mapping and warning-denied flags are recorded.
At the CI-scheduling inspection checkpoint,9 commands/9 executions had completed;
`09-lib-seed0.log` was running the complete dense acknowledged-restart/stall case.
This partial progress does not replace terminal collection or cover all291.

The job subsequently **failed** at2026-10-28T08:50:28.447343Z after
1082.4781163089792s wall time. The dense restart/stall command exceeded the unchanged
900s watchdog; its partial log ends inside the test, without a passing summary.
Nine preceding host commands/executions passed; the other135 commands are not
completed coverage. The supervisor/runner identities are no longer live, source/
supervisor hashes still match, and `collect` explicitly refuses the failed job.
Raw state, progress, completed/partial logs and collection refusal are preserved
with [CI-host evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/ci-hosts-stage7.md). No watchdog, workload or guard
was weakened. The focused recovery launch below follows this failed checkpoint;
complete Stage7 recovery remains open.

- `job.json` SHA256: `bb53dd4414568c9ff7677ef5390843552853cff5e0c9c247280a7ffd324f4053`.
- Source archive SHA256: `0cc88b39499bc8db876858a4817faa6308dbc1dda952c5a76a4f6409cc724eed`.
- Copied supervisor SHA256: `5e9ac63c03b06687db2adcfac79d9c476c736f07cca4d06de3bb13164d23b8e2`.

Launch/input verification and nine partial passes are **not a passing complete
interpreter result**. Preserve this failed job, investigate the expensive command,
and justify the smallest adequate affected recovery selection. Continue useful
implementation work without routinely repeating the full suite. Stage7 remains7/8;
interpreter recovery, remote CI artifacts and later delivery obligations stay open.

### Dense recovery candidate (native verified; interpreter pending)

The supplementary dense event checks now use an off-render BTreeSet membership
index after the unchanged complete semantic-byte comparison. All assertions,
seven replay runs/partitions, frames/packets/receipts, seed0, guards and900s watchdog
remain unchanged. This removes repeated linear query work, not musical execution;
no interpreter speedup or recovery is claimed yet. Native57 Python/59 fixture
checks per profile, both complete native report comparisons and2141 workspace
passes/four skips validate the candidate. See
[evidence/dense-miri-recovery-stage7.md](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/dense-miri-recovery-stage7.md).

The supervisor now forwards/verifies a declared selection. Host batch10/63 names
exactly the failed test, so one committed focused job is justified by the actual
900s timeout. Its eventual pass would prove only that case; all remaining stage/
final coverage is still required.

Focused job **`20261028T094652-f2718d3398`** launched from committed candidate
**`dbe6d6236d7e7a8266d9dc75af5346490cfdf7fe`** at
2026-10-28T09:46:52.535072Z, supervisorPID3709585:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-recovery-dense --commit dbe6d623 \
  --package loop_engine_host_fixture --shard 10/63
```

Input verification matched all862 frozen source entries and the copied supervisor,
excluded `.pi/`, confirmed the explicit source-commit environment and exactly one
seed0 execution of the failed full test. The package-wide63-command inventory
fingerprint is `6a20e86fab488dc30b6e0d6f4da0a07bf48a6ff12d97f7c7b80e2a8b24ed3b8d`.
At the09:53Z natural checkpoint the supervisor was still running. It subsequently
**failed** at2026-10-28T10:01:54.404514Z after901.8694647719967s: the same full test
again exceeded the unchanged900s watchdog, with **zero completed executions**.
The partial log has no passing summary. All862 source entries and the copied
supervisor still match; known supervisor/runner identities are no longer live;
collection refuses `Cannot collect passing evidence: failed`. The checker index
was not sufficient to meet the watchdog; censored timeouts do not measure a speedup
or slowdown. Both failed jobs and their different scopes remain preserved.
Stage7's complete291-execution selection and final source/CI/review obligations
remain open.

- Root: `target/loop-layer-miri-jobs/20261028T094652-f2718d3398/`.
- `job.json` SHA256: `e2fe8e309d4be009016606acab2b983b3d0b831e0598e4f76d676378f4371e50`.
- Source archive SHA256: `cd36c73d87b7fd1640b3f2a9527b10c9da4e0ccc398469293e934e28bd8b3343`.
- Copied supervisor SHA256: `c2cd7c0db36774db2a086e0721745ccca750b786b02870756abdbfb304fcd20d`.
- Local launch/input report: `target/loop-layer-miri-recovery-stage7/launch-verification.json`.

### Exact-test diagnostic profiling (interrupted; not a pass)

The second concrete timeout justifies profiling the same one-command/full-test
selection before further optimization. `--measureme` adds only Miri's function
profiler, requires one selected command, records exact flags and profile mode, and
keeps seed0/full workloads/guards/900s watchdog. Collection checks profile artifacts;
CI complete-set verification refuses diagnostic-profile reports. Thirty-two Miri
helper/synthetic tests (sixty with loop parser/orchestration tests) pass. Native
Rust/Wasm/dependencies are unchanged from `dbe6d623`; no fresh interpreter pass or
native performance result is inferred. See
[evidence/dense-miri-profile-stage7.md](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/dense-miri-profile-stage7.md).

The analysis tool is `summarize v11.0.1` from measureme commit
`9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`, installed in a writable private cache.
Initial attempts hit read-only Cargo storage and obsolete third-party `unsafe`
blocks under current rustc's denied warnings. The analyzer alone was built with
`RUSTFLAGS=-A unused-unsafe`; project/Miri warning-denied builds, safety checks and
guards are not changed. Its unsupported `--version` probe is retained; Cargo's
installed-package listing records the version/revision.

Diagnostic job **`20261028T102117-ca19c81152`** launched from committed tooling
snapshot **`3574d8fc3abe3abcc7ef933d3200f3876357eca3`** at
2026-10-28T10:21:18.263541Z, supervisorPID3725186:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-dense-diagnosis --commit 3574d8fc \
  --package loop_engine_host_fixture --shard 10/63 --measureme
```

Launch verification matched all864 frozen source entries and the copied supervisor,
excluded `.pi/`, confirmed warning-denied Rust/rustdoc flags and the explicit source
commit, and matched the exact one-test inventory/seed0 plus these sole interpreter
flags: `-Zmiri-seed=0 -Zmiri-measureme=<job>/reports/measureme`.
The launch-verification checkpoint was **running**, current `00-lib-seed0.log`.
This is a justified diagnostic invocation, not a second routine full campaign or
a passing interpreter result. Preserve any partial profiles on timeout; analyze
a copy outside the reports directory. Continue useful work and check at natural
checkpoints rather than waiting/polling. Complete Stage7/final coverage stays open.

- Root: `target/loop-layer-miri-jobs/20261028T102117-ca19c81152/`.
- `job.json` SHA256: `ba3619d4c0c6420d10e1d267e96bada5ad512a79296c7be40b1a6cdbd2edaecd`.
- Source archive SHA256: `d26da23dd767ba4e3f5dc74424b5bdcd246c049c6552c7a9d413a8a43e8057c6`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Local launch/input report: `target/loop-layer-miri-profile-stage7/launch-verification.json`.

The diagnostic was deliberately interrupted with SIGINT to the identity-verified
interpreter child so Miri could finalize its profile before the unchanged hard
watchdog. An earlier identity/path assertion refused to signal; the corrected
check matched the actual isolated build path before signalling. The supervisor
recorded **failed**, exit1, at2026-10-28T10:34:59.550132Z after821.2866128640017s,
with **zero completed executions**. All864 source entries/supervisor still match;
collection refuses `Cannot collect passing evidence: failed`. This interruption
is diagnostic data, never a shortened passing workload or a timeout-recovery pass.

The live profile initially lacked its string index and could not be decoded.
After finalization, a copy outside reports decoded successfully with the pinned
analyzer:816.845415594s of recorded completed function events and14,530 labels.
MIDI restore consumed231.277s inclusive, cleanup72.915s across its two instantiations;
there were1,576,960 note lookups. Journal/MIDI-buffer preparation consumed98.057s/
120.302s inclusive; ordered MIDI insertion consumed108.454s inclusive, including
66.720s in partition-point lookup. These nested times must not be summed as
independent costs or mistaken for completed execution/native callback timings.
The3,232,339,355-byte raw profile remains in the job; SHA256
`c1e1a5fdf46b5a8232e8d5ba2aa63cc96cf1214cbcb64e82a8d21e8caa316268`.
See [measured-cost candidate evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/midi-sparse-stage8.md) for analysis,
raw failed logs and subsequent native/Wasm verification, not an interpreter pass.

### Measured-cost candidate and current registered selection

Fixed channel bit indexes now expose ascending non-default-note/known-control
iteration; cleanup/restoration retains every required packet and its order.
Bulk pre-touch still writes all prepared bytes/slots, and ordered insertion has an
append fast path with unchanged stable middle insertion/refusal. Two new guarded
models compare indexes with dense getters and insertion with a scalar stable sort.
Existing full musical workloads, seven replay runs, guards/seeds and900s watchdog
are unchanged. Native190runtime/153loop/59fixture tests per profile,186runtime/
153loop tests in each Node/Chromium, all loop bindings/actual contexts/six failures
per profile, complete native report equality and2143 workspace passes/four skips
validate this candidate. No interpreter speedup/recovery is claimed yet.

At that candidate checkpoint the selection was **145 commands/298 executions =135runtime+100loop+63host**.
All prior filters/seeds and their relative order remain. `midi_state` grows from
six to seven tests; the full `rt_runtime` `midi` target adds six executions (five
existing tests plus the new insertion model). No prior coverage is removed.
Host batch10/63 and its fingerprint remain unchanged. The repeated concrete dense
timeout plus measured-cost implementation changes justify one unprofiled retry
of that same complete exact test/seed0 as the smallest adequate recovery check.
A successful retry would cover only that case; consolidated current/final coverage,
remote CI artifacts, remaining Stage8 work and final review remain mandatory.

Focused unprofiled job **`20261028T113106-86abdbc4e8`** launched from committed
candidate **`802833f492df0b7e8f5a8f1c955c16ea102c2cdd`** at
2026-10-28T11:31:07.191913Z, supervisorPID3899660:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-recovery-measured --commit 802833f4 \
  --package loop_engine_host_fixture --shard 10/63
```

Launch verification matched866 frozen source entries/the copied supervisor,
excluded `.pi/`, confirmed strict Rust/rustdoc/source-commit environment and the
unchanged exact full-test selection/seed0. Its sole interpreter flag is
`-Zmiri-seed=0`; there is no profiler or safety-flag override. The launch-verification
checkpoint was **running**, not passing. No second routine job should run while
this one is active. Continue useful development; inspect at natural checkpoints
and require terminal collection before any passing claim.

- Root: `target/loop-layer-miri-jobs/20261028T113106-86abdbc4e8/`.
- `job.json` SHA256: `aa5e8a8811a161c2fc4a3217bcc01da0b682dc12df562e85f36bd71e63662f69`.
- Source archive SHA256: `74ba9648fccc81ff345fb9a4b00c1e9b6b39c2a4864ea65882d32f763c5d2d4b`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Local proof: `target/loop-layer-midi-sparse-stage8/recovery-launch-verification.json`.
  Its first reporting attempt used the wrong archive-key name after source/selection
  checks; the corrected report records the actual key. No passing result was written.
  The preceding candidate archive predates launch and is not interpreter evidence.

The unprofiled measured-cost recovery subsequently **failed** at
2026-10-28T11:46:09.194427Z after902.0025350259966s. The same complete exact seed0
command exceeded the unchanged900s watchdog, with **zero completed executions**
and no passing raw summary. Terminal verification matched all866 source entries,
the copied supervisor and original job hash; both recorded supervisor/runner
start-tick identities are dead. Collection refuses
`Cannot collect passing evidence: failed`. This candidate did not resolve the
watchdog failure; censored timeouts establish neither speedup nor slowdown.
There is no active job from this attempt. No routine duplicate campaign is
launched for the native-only benchmark addition below. Further recovery needs a
concrete residual-cost diagnosis or relevant change and the smallest adequate
selection; complete145-command/298-execution coverage and final-head CI remain
mandatory. The failure proof/raw logs accompany
[benchmark-stage8 evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/benchmark-stage8.md); benchmark success is
not interpreter recovery. Stage7 remains7/8.

### Residual-cost diagnosis after the measured-cost timeout (failed)

The concrete unprofiled timeout above justified profiling the same whole exact
host10/63 command again, now against the measured-cost implementation. Diagnostic
job **`20261028T131907-e502c4cd23`** used committed
**`0b853b4a68cdd73092ad62a6ebf22a2e2a931f84`**, seed0 and `--measureme`.
Relevant interpreted library/fixture/support sources, manifests/locks,
configuration and runner scripts match the failed `802833f4` input. Its872 frozen
entries and copied supervisor were verified; `.pi/` is excluded. The full seven
replays, assertions, guards and900s watchdog were unchanged.

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-residual-diagnosis --commit 0b853b4a \
  --package loop_engine_host_fixture --shard 10/63 --measureme
```

At a natural development checkpoint, identity/ancestry/command/seed/profiler-path
checks preceded SIGINT to interpreterPID3939463, finalizing measureme's string
index before the hard watchdog. This intentionally interrupted diagnostic
**failed/exit1**,2026-10-28T13:19:07.543223Z–13:30:18.412422Z,
**670.8692323739524s**, **zero completed test executions**. Frozen source/supervisor
still match; recorded supervisor3939332 and runner3939334 identities are dead.
Passing collection refuses `Cannot collect passing evidence: failed`.
There is no active job from this attempt.

The2,389,684,069-byte finalized raw profile has SHA256
`d4a413d92e027d3d7b9172d98f4d2fb24721086ca5465a7fb38867901f5bfafc`.
Analysis used a checked copy outside job reports and the existing pinned analyzer;
the raw file remains local. Its666.539662436s/14,839 labels cover completed function
events, not a completed test. Inclusive costs include routing104.405s,
merge-window95.478s, packet parsing78.209s, MIDI-buffer preparation74.402s and
restoration66.904s. Nested values overlap; the profile also exposes large
state-valued seek/result paths. Different interrupted prefixes are not paired
speedup measurements or native callback timings. No further routine unprofiled
retry is launched: implementation/model work must address concrete residual costs.
Complete145-command/298-execution coverage and remote/final delivery remain due.
See [residual diagnosis and CI evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/residual-ci-stage8.md).

### Bulk-MIDI candidate: native/portable verified, interpreter recovery pending

The residual profile above motivates safe bulk Event preparation and validated
whole-source MIDI append. All configured slots/bytes are still pre-touched;
capacity/window/recovery checks and stable equal-time ordering are preserved.
Partial or interleaved windows retain the existing insertion path. No workload,
seven replay runs, assertion, guard, seed or900s watchdog is reduced.

The independent guarded model executes1280 buffer-reuse rounds and checks11
preparation capacities, complete scalar packet order/content, variable packet
sizes/SysEx, byte-layout permutations, rebasing, empty windows, arithmetic refusal
and32-packet reset behavior. Its allocation/reallocation/deallocation witness
proves active instrumentation. Native191runtime/153loop/59fixture tests/profile,
187runtime+153loop per Node/Chromium, all loop binding/actual-host/failure gates,
complete native before/after equality, fresh generic-host controls,72 helper tests
and2144 workspace passes/four skips validate semantics, not interpreter speedup.

The **current selection is146 commands/299 executions =136runtime+100loop+63host**.
One exact runtime-library model command is added; all prior cases/counts/seeds and
relative order are explicitly compared and retained. Host10/63 is unchanged.
The registered model name is
`buffer::tests::bulk_preparation_and_window_merges_match_scalar_packets_through_refusal_reset_and_reuse`.
It uses `-- --exact`, minimum1 and seed0; no new interpreter invocation is inferred
from native dispatch or registration.

The actual prior timeout plus these measured-cost implementation/model changes
justify one unprofiled retry of the same complete exact failed host command as the
smallest adequate recovery selection, after committing/freezing the candidate.
A pass would cover only that test, never consolidated current/final coverage.
See [bulk-MIDI evidence](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/bulk-midi-stage8.md). Stage7 remains7/8 and
Stage8 remains3/8; broader models/audit, remote artifacts and delivery remain due.

Focused unprofiled recovery **`20261028T145739-89dfd53bdb`** launched from
committed **`3d10c7a06f1e4ad01185d047e4a1c44abf525629`** at
2026-10-28T14:57:39.512395Z, supervisorPID32129:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-7-recovery-bulk --commit 3d10c7a0 \
  --package loop_engine_host_fixture --shard 10/63
```

Launch verification matches876 frozen source entries/the copied supervisor,
excludes `.pi/`, verifies the unchanged exact host10/63 inventory and strict
Rust/rustdoc/source-commit environment. Its sole interpreter flag is
`-Zmiri-seed=0`, with no profiler or inherited safety override. The checkpoint is
**running**, not passed. The full seven replays/assertions/guards and900s watchdog
remain. No second routine job should run while this selection is active.

- Root: `target/loop-layer-miri-jobs/20261028T145739-89dfd53bdb/`.
- Job metadata SHA256: `21667f95e3a4d6c67def55c378fb81358221bf19afaeb3b07280fb8f1c515bdc`.
- Source archive SHA256: `4a7598ac45912ff2d332bd67fdb445afb0abfa969847f5d5aab98f6095795217`.
- Copied supervisor SHA256: `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d`.
- Local launch proof: `target/loop-layer-bulk-midi-stage8/recovery-launch-verification.json`.

The preceding native/portable evidence archive predates this launch and cannot
certify its result. Continue implementation rather than polling tightly; terminal
collection/source verification is required before any passing claim. Even a
successful scoped recovery leaves complete146-command/299-execution current/final
coverage, broader audit/models and remote/final delivery due.

### Historical Stage7 registered selection

Stage7 host implementation is ready for its consolidated interpreter gate; no
complete passing Stage7 interpreter result exists yet. The pinned runner now registers all59 native
standalone-host library tests at seed0, plus primitive-wire and graph-actor tests
at seeds1/2:63 host executions. This includes the35 cases detailed below and all24
remaining ingress/return, observer, codec, allocator and foundation cases. Exact
filters each require one passing test, use the standalone manifest/lockfile and
retain full workloads/guards. Native discovery and63 exact native dispatches pass;
these are not interpreter results. Nineteen synthetic/tooling Python tests pass.

The full selection is144 commands/291 executions:128 runtime+100 loop+63 host.
Host cases run first; all prior81 commands/228 executions retain their filters,
counts, seeds and relative order. The new `Controller::resource` path and host
ownership/planner/observer support are included with their runtime/loop regressions.
The immutable committed Stage7 job above attempted this selection with the
unchanged900s per-command watchdog but failed after nine executions. Until verified
recovery/complete terminal collection, interpreter/CI
artifacts and the last Stage7 row remain open. See
[evidence/miri-registration-stage7.md](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/evidence/miri-registration-stage7.md).
Native, public-Wasm binding and actual worklet passes do not extend the collected
Stage6 inventory.
The new standalone `loop_engine_host_fixture` graph receiver contributes seven
native cases under `graph_transfer::tests`, including unchanged128-round lifecycle
workloads, actual route activation, stalled owning returns and disconnect cleanup.
The native graph command/return milestone adds another eight cases: six under
`graph_wire::tests` and two under `graph_bridge::tests`, including128 wire candidate
rounds, mixed four-normal/two-graph-credit saturation, a local owning Full refusal
and intact unexpected-completion quarantine. Public graph command bindings add two
native `browser::tests` cases: `graph_binding_buffers_cover_route_cut_and_reliable_owning_release`
(three partitions/full media/receipts/semantic/recovery) and
`graph_binding_fault_disposal_abandons_credits_without_fabricating_returns` (both
actual FX fault modes). Actual graph-worklet integration adds
`browser::native_graph::tests::acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference`,
covering whole/single/irregular/zero partitions and complete native payload/command/
result/media/semantic/resource evidence for the audible checkpoint script.
The native actor extension adds
`browser::native_graph::tests::bounded_two_thread_graph_transport_matches_complete_native_reference`:
capacity-one copied-byte transport, renderer-local owners, malformed retries,
whole/single/irregular/zero partitions and complete direct-report equality.
Immutable export integration adds four more native cases: two `browser::native_export::tests`
(complete direct/threaded two-revision snapshots/partitions and cancellation/EOF/stalls/
128 export reuses) and two `export_wire::tests` (canonical/full-width codecs,
truncations/projection validation and transactional credits/nonwrapping generations).
Resource/program staging adds four more native cases: two
`browser::native_staging::tests` (complete direct/threaded resource revisions,
raw-MIDI source refusal, nested play/iteration-seek/stop and full independent
semantic/media tables; owning chunk cancellation and128 program reuses in both
actor modes), plus two `staging_wire::tests` (full-width codecs/all truncations/
padding and transactional/nonwrapping client identities). The new
`Controller::resource` forwarding path and prepared topology/source settings are
also relevant source inputs for the consolidated selection.
Dense host coverage adds two cases: `scenario::dense::tests::dense_capture_cascades_and_clocked_composites_keep_music_with_unread_results`
(direct facade, unchanged131-frame full media/cleanup workload) and
`browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled`
(whole/single/irregular/zero partitions in direct and capacity-one actor modes,
complete independent media/semantic/receipt oracle). Larger fixed MIDI/observer
storage is prepared off-render; there is no reduced interpreter workload.
Transferred-result buffer exhaustion adds
`browser::native_dense::tests::acknowledged_restart_keeps_media_and_owning_results_while_transport_is_unpolled`:
the complete136-frame restart script, seven requests/fourteen replies and independent
media/semantic tables across whole/single/irregular/zero partitions and both actor
modes. Actual MessagePort buffer holding is JavaScript/browser evidence, not a
native interpreter feature. The affected dense native support/oracle must be part
of the consolidated source snapshot.
Prepared media adds three applicable native cases:
`scenario::media_case::tests::replacement_countdown_and_capture_exhaustion_keep_exact_prefix_and_live_audio`,
`browser::native_media::tests::replacement_countdown_prefix_exports_and_bounded_actors_match_complete_oracles`,
and `staging_wire::tests::countdown_results_keep_delivery_distinct_from_terminal_frame_and_preserve_credit_on_corruption`.
They retain complete indexed replacement/MIDI restoration, delayed delivery/terminal
frames, truthful common-prefix exhaustion/export/initial state and direct/capacity-one
partition workloads. The existing canonical staging test now covers all27 command
kinds. Include the new constructor/catalog/cleanup paths and source/oracle support
in the consolidated snapshot; browser output is not interpreter evidence.
Planner lifecycle canaries add two applicable cases:
`planner_canary::tests::resolver_planner_control_is_partition_invariant_with_complete_boundary_tables`
and `planner_canary::tests::planner_errors_and_heap_faults_have_exact_lifecycle_origins_and_are_not_successful_cleanup`.
The first retains full128-frame whole/single/irregular/zero workloads and independent
512-record semantic tables; the second retains actual planner error and deliberate
allocator instrumentation, permanent faults and off-render abandonment. Include the
new module's real Resolver adapter/decision consumer, destructor guards and support
in the snapshot. The existing dev-only allocator remains enabled; no workload or
900s watchdog reduction and no immediate per-commit interpreter run is introduced.
All19 graph-focused, four export-focused, four staging-focused, three dense/stall,
three media and two planner cases (**35**) are now **registered, without a collected
passing interpreter result**.
Node/Chromium now execute shared
public payload and command bindings, including128 real candidate return cycles;
they do not execute these native test functions or extend Miri coverage. Keep the
existing900s per-command watchdog, guards and workloads in the registered
standalone manifest/lockfile and exact-filter selection. Final delivery
still requires the complete applicable source-matched selection; no routine
per-commit Miri invocation was launched for this payload foundation.

## Collected Stage 6 pass

Job `20261027T025325-e065d48941` completed successfully and was collected with the
supported `collect` command. **81 commands/228 executions passed:128 runtime+100
loop**, pinned nightly-2026-05-04 / miri0.1.0(ad3a598ca4), unchanged900s watchdog.
Implementation snapshot: `faec78df0a990d28b4fefb2f75e43ea0ace7f1ea`. Ended
2026-10-27T04:55:23.943226Z, total7318.292420581012s. Full snapshot/archive/supervisor,
ordered inventory/seeds/toolchain and82 artifact hashes were verified. Current
123 scoped Rust/10 configuration-runner inputs and supervisor/helper hashes match.

Committed evidence: `evidence/miri-stage6.json`, `miri-stage6-collected.json`,
`miri-stage6-provenance.json`, and `miri-stage6-logs.tar.gz` containing the raw
81 logs and result report. Report SHA256:
`d4c8fa7b5e0caf610783a56d178ae4343288b413f63e2841184847ae15df3ae0`.
The dense128-boundary case passed in877.7475190730183s; no watchdog/workload/guard
change or second routine invocation was needed. Stage6 is now complete. Per the
then-current user instruction, the implementation goal paused before Stage7;
this does not pause the documentation goal or waive later implementation gates.
The earlier interrupted171/178 invocation is still partial and is not relabelled.

## Historical Stage6 launch and while-running checkpoints

Launched `20261027T025325-e065d48941` at2026-10-27T02:53:25Z from immutable commit
`faec78df0a990d28b4fefb2f75e43ea0ace7f1ea` with the full selection:
**81 commands /228 executions (128 runtime+100 loop)**, pinned
`nightly-2026-05-04`, unchanged900-second per-command watchdog. Launch command:

```sh
nix develop --command python3 scripts/run_rt_runtime_miri_jobs.py start \
  --stage stage-6 --commit faec78df
```

Job directory: `target/loop-layer-miri-jobs/20261027T025325-e065d48941/`.
Its immutable `job.json` SHA256 is
`baf376abb633bc363f7e94712cbef3af9108d3675ad1026b8061db28dd06d807`.
The manifest includes source/archive/supervisor hashes, exact filters/seeds/counts,
command/environment and isolated paths; live process identity is separately recorded.
The first natural documentation-review checkpoint verified the supervisor running;
progress subsequently reported8 completed commands/49 executions and `midi_state`
in progress. At the Part I handoff commit checkpoint it had22 completed commands/
111 executions, with `capture_group_model` in progress and the supervisor still
running. **This is partial progress, not a collected pass or Stage6 completion.**
Do not start another routine job. Continue documentation work, inspect at natural
checkpoints, and collect/verify the terminal result before updating acceptance.
The working-tree documentation/example review does not mutate this snapshot.
The subsequent resource/MIDI/capture handoff checkpoint verified the supervisor
still running:37 commands/162 executions completed, with the configuration
input-projection preserve/reset case in progress. Selected native documentation
checks ran in a different Cargo output directory; no additional Miri was launched.
This later progress is also partial and does not replace terminal collection.
The capacity/ingress/shutdown review checkpoint verified53 commands/178 of228
executions completed, with the paired cancellation/released-generation case running.
Do not confuse this new-job partial count with the historical interrupted178-selection.
Full native runtime tests and bounded documentation smokes/canaries ran in separate
output while the immutable interpreter job continued; no second job was started.
The control/planning review checkpoint verified67 completed commands/207 of228
executions, with the loop MIDI target (`67-midi-seed0.log`) in progress. Four native
example executions and the drafting checker ran separately. All123 scoped Rust and
10 configuration/runner input hashes remain unchanged; this review adds no new
interpreter invocation or terminal-pass claim. Stage6 remains incomplete.
The loop-guide/facade-contract review checkpoint verified70 completed commands/
215 of228 executions; `70-runtime-seed0.log` was running the stopped-input/half-open-
prefix export case. Fresh native example/rustdoc checks used separate output, and
all scoped Rust/configuration/archive hashes remained unchanged. Still no terminal
collection or new interpreter invocation; the existing job alone continues.
The repository/architecture review checkpoint verified75 completed commands/220 of228
executions, with the dense128-boundary runtime case in `75-runtime-seed0.log` running
under the unchanged900-second watchdog. Both-crate rustdoc and a drafting book check
ran separately; source/configuration hashes remain unchanged. This is still partial
progress and does not authorize Stage6 completion or another routine Miri invocation.


Latest nested-lifecycle/readiness milestone: **228 selected =128 runtime+100 loop**.
The added guarded interloop case passes native/release/Node/Chromium; all153 loop
cases and2141 workspace cases/four skips pass. All28 topology and both interloop
cases now use individual exact filters. Native discovery matches every name and
all30 filters execute exactly one native test each. Workloads, seed0, guards and
the900-second per-command watchdog are unchanged. Historical interloop duration
468.43s and grouped suite growth motivate finer supervision; no new timeout or
interpreter pass was claimed at readiness. The subsequently collected job above now
verifies the full inventory. [Stage6 completion audit](STAGE6_READINESS.md) maps
requirements and records terminal collection/source checks. Later acceptance remains
open in that implementation plan; its historical pause is not an instruction
for the current documentation authoring workflow.

Previous paired program/graph milestone: **227 selected =128 runtime+99 loop**.
Four new guarded facade cases are registered, not interpreted. The existing private
probe case also checks selection-query phases. Native/release/Node/Chromium pass152
loop tests; fourteen supervisor/runner helper tests pass. Full Stage6 still needs
its consolidated supervised immutable-snapshot interpreter run.

Latest configuration milestone: **223 selected =128 runtime+95 loop**, including
seven new facade configuration cases and one private role/source exhaustion case.
This is registration only, not interpreter evidence. Native debug/release and
Node/Chromium passed148 loop cases; fourteen runner-helper tests passed. Preserve
the historical interrupted171/178 result as partial. Consolidated Stage6 execution
still awaits a final immutable committed snapshot and supervised background run.

Miri runs asynchronously under the [plan's execution contract](../../PLAN_LOOP_LAYER.md#background-miri-execution-user-approved-scheduling-change).
Pending verification does not prevent implementation commits or further development;
it does prevent claiming an applicable stage fully verified or making final
delivery without the required passing evidence.

Default cadence is **one consolidated run per applicable numbered-stage
milestone**, not per smaller step or commit. Accumulate new/affected cases and
necessary regressions until that stage's implementation is ready. Use focused
extra runs only for actual Miri failures, concrete safety regressions or relevant
post-snapshot changes requiring refreshed evidence, with a recorded reason.
Do not repeatedly launch full/package suites during ordinary iteration. Final
coverage remains required; a complete stage run can also satisfy it only with a
verified match of the full required inventory and relevant inputs.

Use [`scripts/run_rt_runtime_miri_jobs.py`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/scripts/run_rt_runtime_miri_jobs.py)
for launch/status/collection; [MIRI_BACKGROUND.md](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/loop_layer/MIRI_BACKGROUND.md) documents the
commands, committed-snapshot requirement, Linux-only wrapper and failure handling.
Before consolidation, the smaller program-probe, owning-transaction,
facade-adapter and generic graph-permit steps had no separate background run.
At the media-owner prerequisite the inventory was215=128runtime+87loop; the later
configuration, paired-program/graph and nested cases brought it to228. These were
registration-only checkpoints then, subsequently covered by the collected Stage 6
run above, not retroactive passes of the individual earlier invocations.

## Job record requirements

Each new job records:

- Job ID and state: queued, running, passed, failed, orphaned, interrupted, cancelled or
  superseded. A missing supervisor/result is not success.
- Immutable source snapshot/commit, relevant dependency and runner/configuration
  hashes, pinned toolchain, and reproduction command.
- Package/target/filter/seed inventory and expected execution counts.
- Supervisor/process identity, isolated build/report paths, start/end time,
  monotonic wall duration, exit status and per-case logs.
- Verified artifact hashes, source applicability, any superseding job, and the
  milestone/final acceptance obligations it closes or leaves open.

Inspect jobs at natural development checkpoints, not in a blocking tool call or
a tight polling loop. Only promote a result to passing evidence after validating
the terminal result, complete expected inventory and source/artifact identities.
Changes made while a job runs belong to a later snapshot. Accumulate any needed
reruns for the next applicable stage/final verification barrier rather than
launching a new job after every edit.

## Historical outstanding work at the scheduling change

| Job / evidence | State | Verified scope | Outstanding obligation |
| --- | --- | --- | --- |
| `facade-topology-stage6`, interrupted foreground invocation | Interrupted; orphan process group terminated | 171 completed selected executions: 119 runtime and 52 loop, including all six new `runtime_topology` cases | Seven selected loop executions were not completed/reported; no full 178-execution pass or whole-invocation wall duration exists. No immediate repeat/resume is scheduled solely for this smaller step; include required coverage in the consolidated Stage 6 run |
| Background supervisor/snapshot support | Implemented; 14 Python tests pass | Immutable committed exports, detached Linux supervisor, streamed output, exact inventory/hash collection and cancellation/descendant cleanup tested with synthetic fixtures | First actual interpreter use remains the consolidated applicable stage run; no real Miri pass is claimed by these infrastructure tests |
| Unpublished program-boundary probe prerequisite | Deferred to consolidated Stage6 run | Four new executions registered: one private library case and three `program_boundaries` cases; native/release/Node/Chromium/workspace gates pass | Include these and affected resolver regressions in the stage snapshot. Current selected inventory is182=119runtime+63loop, not a completed run |
| Standalone owning single-program transaction | Deferred to consolidated Stage6 run | Nine `program_changes` executions registered; native/release/Node/Chromium/workspace gates pass | Include the new owner/activation cases and affected resolver regressions in the stage snapshot. Current inventory is191=119runtime+72loop; no fresh interpreter pass is claimed |
| Fixed-graph facade owning program activation and permanent quiescence | Deferred to consolidated Stage6 run | Five new `runtime_topology` cases and one new `program_changes` case registered; native/release/Node/Chromium/workspace gates pass | Include all new and affected resolver/facade lifecycle cases. Current inventory is197=119runtime+78loop; no interpreter run for this slice |
| Media-ledger owner-generation publication | Deferred to consolidated Stage6 run | Three guarded media cases including512-edit independent reference model and logical generation exhaustion; loop140 native/Node/Chromium, workspace2128/four skips,19 scoped gates and8 positive/23 negative proofs pass | Include new and affected media/facade regression cases. Inventory215=128runtime+87loop; no interpreter invocation. Facade role-generation integration remains pending |
| Fixed-role loop facade live graph and paired topology activation | Deferred to consolidated Stage6 run | Six additional guarded runtime_topology cases; loop137 native/Node/Chromium, workspace2125/four existing skips,19 scoped gates and7 positive/18 negative proofs pass | Include new cases and affected facade regressions. Inventory212=128runtime+84loop; no new interpreter invocation. General role/media transactions and Stage6 completion remain open |
| Generic staged-graph activation capability | Deferred to consolidated Stage6 run | Seven guarded `graph_activation` cases, one guarded held-request recovery transport case and one exact private scope/generation/epoch case registered; both packages' native/Node/Chromium, workspace, generic hosts/canaries and lifetime proofs pass | Include new cases and affected executor/planner/graph/transport regressions. Inventory at that step was206=128runtime+78loop. Exact-filter Cargo/harness argument separation is checked by helper tests and a native mirror, not a Miri invocation |
| Final implementation Miri selection | Pending | Historical/milestone runs retain their original scope | Run and collect the complete required final selection against the final implementation snapshot; reconcile later changes and final-head CI |

The interrupted invocation used the working-tree candidate based on `9bab9ddf`,
subsequently committed as `d89f96a7`; it was not a committed final implementation
when tested. Its 104 Rust source hashes are recorded in
`evidence/facade-topology-stage6.json` and were rechecked during interruption
cleanup. The broad 25-gate pass is separate from Miri completion.

Local interruption archive:
`target/loop-layer-facade-topology-stage6/miri-interrupted/partial.json`, with
completed case logs, driver output, runner copy, source hashes and process-cleanup
record. The interrupted dense case's in-memory output was not recovered. The
remaining registered executions are:

1. `runtime`: `scenarios::dense_one_sample_media_keeps_all_128_boundaries_with_unread_external_results`.
2. `runtime`: `scenarios::facade_role_validation_invalid_operations_and_imported_revision_exhaustion_are_checked`.
3. `runtime`: `scenarios::integrated_restart_smoothing_and_gain_follow_decisions_not_host_cuts`.
4. `topology_views`: all three cases.
5. `interloop`: its single case.

Under the reduced cadence, defer routine completion/repetition of this smaller-step
selection to the consolidated Stage 6 run, expanding it for subsequent Stage 6
changes. The list above records missing historical evidence, not a requirement to
launch an immediate additional job. If a focused continuation is justified,
freeze a reproducible source snapshot first and confirm it matches the recorded inputs. A resumed batch must be reported separately from
the interrupted batch, not as a fresh uninterrupted 178-execution invocation.
If relevant inputs have changed, queue the affected cases against the new snapshot
instead. Keep all original completed and interrupted evidence.
