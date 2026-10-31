# Stage 8 final sync-model timeout and censored diagnosis

## Frozen campaign failed, not collected

Job **`20261028T190146-abeee2e59e`**, immutable source
**`8633d0f0754a2f4445c3d0febb1f1ae2d7e9572e`**, ended **failed/exit1** at
2026-10-28T22:47:06.723671Z. Supervisor monotonic wall time is
**13519.300086269039s**, starting19:01:47.423617Z. Full intended selection remains
**151 commands/304 executions =138runtime+102loop+64host**.

The first150 commands have303 matching passing executions in the verified ordered
progress/raw logs. The final exact seed0 command exceeded the unchanged900s watchdog:

```sh
rustup run nightly-2026-05-04 cargo miri test --locked -p shoop_loop_engine \
  --test adversarial_timeline \
  seeded_sync_components_deliver_each_occurrence_once_with_saturated_results_and_optional_journals \
  -- --exact
```

Its raw log has no terminal passing summary. There is no final `result.json` or
collection; supported `collect` refuses `Cannot collect passing evidence: failed`.
**303 partial passes do not certify304 executions or the final implementation.**
No allocator/assertion/UB finding is reported by this timeout, but failure remains
failure. The earlier collected146/299 Stage7 recovery certifies only its older
frozen source and cannot fill the missing new case.

Verification rechecks all890 immutable entries, `.pi` exclusion, literal committed
source re-export, copied supervisor, complete normalized151/304 inventory, strict
Rust/rustdoc/source/isolated-build environment, every completed filter/count/seed/
flag and raw passing summary. Both recorded process identities are dead/current
identity null. No job input was modified. Key identities:

| Artifact | SHA256 |
| --- | --- |
| Job metadata | `a139dfecc8735b5fdeab5f752d22364b06acb599fbc67a25576b2dd792075100` |
| Literal committed archive | `8c009c30678bbb9d11653b4f0699be7c4e2f9b6f3c4c4bc887f9f4b9f0aa525f` |
| Copied supervisor | `75dfb8fdc9b45957a2bfd01e679216e0f5ce28be2dffafdf8aba5b938a36fa1d` |

The descriptor case passes37.30232309299754s; added typed model9.739069725037552s,
capture790.2986791380099s and program875.0295954379835s. These real command wall
times do not establish CI margin or justify watchdog changes. The final sync model
keeps all4seeds×8rings×9partition/declaration/journal profiles,64frames,2–17members,
periods1–7, retained normal results, maximum countdown, independent occurrence/
multiplicity/full-order/state assertions and positive heap controls. No Miri-only
shortening, musical quota or safety-flag override is introduced.

## One exact diagnostic justified by the actual failure

Job **`20261028T225455-65cf2c5a28`** uses the same committed source/archive/all890
entries and copied supervisor. `--shard151/151 --measureme` selects exactly the
unchanged full failed command, one seed0, with strict flags and900s watchdog.
The22:57:38Z immutable launch proof verifies the exact selection and live recorded
supervisor222584/runner222585 identities. Launch is not a pass.

After useful application compiler checks, an identity/ancestry/source/cwd/argument/
flag-checked SIGINT to actual Miri interpreter228615 finalizes a **censored diagnostic**.
An initial preflight refused before signalling because it expected repository-root
cwd; Cargo correctly uses the exact frozen package directory. Only that expectation
was corrected, retaining all identity/source/flag/ancestry checks. The signal goes
to the verified interpreter, not an assumed/reused PID or unrelated supervisor.

Diagnostic terminal status is **failed/exit1/zero passed**,22:54:56.450028 to
23:01:43.962404Z, **407.5124054910266s** supervisor wall. Both recorded processes are
dead/current null, source/supervisor remain intact, raw log has no passing summary,
and supported collection again refuses. Only flags are `-Zmiri-seed=0` and the
declared absolute `-Zmiri-measureme=.../reports/measureme` path. Interrupted diagnosis
is not recovered interpretation, a benchmark, or evidence of a complete model run.

Finalized profile: **1555724199bytes**, SHA256
`ece35f348124a4c477a93ddbe12c0ce529be1202b2aa166910df74c97914dff1`, retained below
the diagnostic job's `reports/measureme/`. Pinned measureme summarize11.0.1,
Git`9aeaf4d0cfaafb13379f0d8f3c3b5e8c13071afc`, analyzes an equal-hash copy **outside**
verified reports. JSON output is written beside that copy; original profile hash
is unchanged. The large raw profile is retained locally, not embedded in this archive.

## Censored cost evidence and next investigation

The profile has5879labels and **371.134845623s completed-function-event time**:

| Path | Calls | Inclusive seconds | Self seconds |
| --- | ---: | ---: | ---: |
| `Boundary::settle` |1694|285.561397094|10.034016114|
| `PreparedTopology::node` |155061|193.840898050|5.501469258|
| Node binary search |155061|149.495105237|19.918540753|
| `select_unpredictable<usize>` |606633|68.980597859|20.535956544|

Times overlap and are inclusive/censored, not additive, native timing, paired
speedup or a completed model. This identifies repeated actual callback topology
lookup as a concrete cost candidate rather than evidence that assertions should
be cut. A narrow possible optimization is a full-identity-checked direct-slot
lookup when canonical node position matches, retaining the existing sparse-ID/
stale-generation binary-search fallback. That candidate is **not implemented or
verified by this report**; it needs safety/regression/source-matched execution and
applicable interpreter recovery. No storage/API/semantic relaxation is proposed.

## Artifact and remaining acceptance

`miri-sync-model-failure-stage8-logs.tar.gz`: **661316bytes/192files**, SHA256
`887cd7359bea6473e49e2a150e21abbd5c8b1ffcd0df48069d85459bc408baa9`.
Every member is byte-verified. It preserves complete original151 raw logs/progress,
job/source-manifest/supervisor/process/terminal metadata, refusal proofs, diagnostic
launch/signal/failed evidence and copied-profile text/JSON/cost summaries and helper
sources. No raw report is mutated or failed result promoted to pass.

[Fresh local host/benchmark/application gates](final-hosts-benchmarks-application-stage10.md)
pass at85e36348 with509 scoped inputs unchanged from this interpreter candidate.
They do not replace the failed mandatory interpreter selection, checked remote
artifacts, final-head CI/review or independent auditor. Stage7 stays7/8, Stage8
stays6/8 and Stage10 remains open. A measured fix must preserve the complete model,
guards/seeds/900s and obtain verified affected recovery/consolidated acceptance;
no routine new campaign is justified merely for this prose/evidence commit.
