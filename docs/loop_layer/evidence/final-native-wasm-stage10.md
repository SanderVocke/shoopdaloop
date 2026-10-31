# Stage 10 native and complete repository-Wasm checkpoint

## Result and source boundary

The native/policy and complete repository Node/Chromium portions of final
validation pass at **`24b458adda606f2d0bdef3624934a99efb55af75`**. All **509 scoped
code/test/dependency/configuration/runner inputs** match frozen interpreter
candidate `8633d0f0754a2f4445c3d0febb1f1ae2d7e9572e` before/after execution. These
are implementation execution inputs, not a documentation baseline. No production,
API, dependency, test workload or safety flag changed for this checkpoint.

**Stage7 remains7/8, Stage8 remains6/8; Stage9 is verified and Stage10 is open.**
Independent generic/loop host replays and actual worklet/failure controls, release
measurement campaigns, application builds/packaged smokes, current interpreter
collection, checked remote artifacts, pushed PR/final-head CI and review still
remain. This checkpoint does not certify those gates or complete the plan.

All Cargo/Python payloads run through `nix develop`, with Rust/rustdoc warnings
denied. The archive preserves actual commands, flags, tool versions, discovered
identities, raw outputs, source comparison, intentional skips and failed preflights.

## Native suites and policies

Twenty-one commands pass, including all three formatting scopes, workspace build,
both library profiles, both standalone fixture test builds/profiles, rustdoc/
doctests, pinned discovery/full workspace nextest, applicable helper tests and
boundary/dependency/test-attribute/tracing policies.

| Executed scope | Debug | Release |
| --- | ---: | ---: |
| `rt_runtime` tests |193|193|
| `shoop_loop_engine` tests |155|155|
| Independent loop-fixture tests |60|60|
| Total library/loop-fixture passes |408|408|

These816 native test summaries have zero failures, ignored or filtered cases.
The generic fixture's Cargo test harness discovers **zero tests** in both profiles:
those commands verify compilation, not its required direct/two-thread replay.
Both library doctest harnesses also discover zero tests; Stage9 separately ran all
46 public example invocations with actual result assertions.

The prescribed complete workspace command passes **2148 tests/four skipped**:

```sh
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
```

The backend allowance is limited to that prescribed command, not real-host gates.
The four skips are explicitly ignored intentional failure controls, independently
confirmed in full workspace discovery, not missing-backend skips:

- `shoop_common`, `nextest_capture_smoke::intentional_failure_publishes_trace`.
- `shoop_wasm_test_support::tests::native_result_failure_canary`.
- `shoop_wasm_test_support::tests::shared_failure_canary_is_ignored_by_default`.
- `shoop_wasm_test_support::tests::shared_result_failure_canary_is_ignored_by_default`.

The CI profile retains four test threads and zero retries. **91 helper tests**
pass across the six runtime/loop Miri/host/benchmark modules, two boundary modules
and Wasm report parser. Historical documentation-administration tests are not
reactivated under the accepted documentation workflow. Closed tracing207 and
both four-target dependency graph checks pass; graph checks are not remote OS
builds. The worklet-client dependency boundary also passes.

### Verified nextest pin and correction of earlier metadata

The final preflight refused **0.9.143** before running native tests. The formerly
recorded PATH ended in a nonexistent `bin/` directory, so it selected Nix's newer
binary. The earlier adversarial-model raw log actually prints0.9.143. Historical
metadata that calls such an invocation0.9.116 is incorrect: retain those raw results
under their actual version, never retroactively relabel them as pinned execution.
Cargo-test and Miri results use separate tools; this does not turn their genuine
passes into nextest runs or invalidate their recorded interpreter toolchain.

The corrected invocation prepends **`$HOME/.cache/rt-runtime-nextest-0.9.116`**
without `/bin`. `cargo nextest --version` now reports **0.9.116**, commit
`2416f2fb1afcb91eb3d701f6dbbde2d82e7c24ad`, x86_64 Linux musl. The runner asserts
that exact version before every final native campaign. Its cached release archive
matches the existing checksum and its executable matches the archive member:

| Pinned tool input | SHA256 |
| --- | --- |
| Release archive | `a50c45875b436c3cdcd6725c6e5b46db1fe588180d6ec85786767d3b53efc179` |
| `cargo-nextest` executable | `fa5177e947cd7949cbe0626623e155fb289f78d19a9c0ad7c034f5ca8196c5fe` |

Fresh0.9.116 discovery confirms193runtime/155loop identities; all348 exactly match
the older0.9.143 discovery used by the Stage9 case-name lookup. The fresh pinned
workspace run above supplies current prescribed-version evidence. No test/assertion
or timeout was weakened. The initial checksum helper also used a nonexistent
`.tar.gz.sha256` name; only its lookup was corrected to the existing `.sha256` file.
Failed preflight/source records and the correction remain in the archive.

## Complete repository Node and Chromium suites

Both commands execute all **19 Cargo-opted-in packages**, independently matched
against package metadata; they are not just the two portable libraries:

```sh
python3 scripts/run_wasm_tests.py --runtime node --profile dev
python3 scripts/run_wasm_tests.py --runtime chrome --profile dev
```

| Scope | Node | Chromium |
| --- | ---: | ---: |
| Complete repository passed |1837|1838|
| `rt_runtime` passed |189|189|
| `shoop_loop_engine` passed |155|155|
| Intentionally ignored support canaries |2|2|

The full commands subsume the portable package selections through their actual
per-package commands/reports. Every package has exact listed/executed/result
counts, unique test identities, no malformed/missing results, zero failures and
returncode0. Filters/features are empty. The only two ignored cases in each runtime
are the explicitly opt-in `shared_failure_canary_is_ignored_by_default` and
`shared_result_failure_canary_is_ignored_by_default`, with their actual reasons.
Chromium additionally discovers/passes
`shoopdaloop::tests::browser_window_perfetto_capture_is_nonempty`; Node has no
extra-only case. No unexpected skip is hidden by aggregate totals.

Tool versions are Rust1.97.1/Cargo1.97.1, Python3.14.7, Node22.22.2,
Chromium/ChromeDriver147.0.7727.137, wasm-pack0.15.0, wasm-bindgen0.2.127 and
wasm-bindgen-test0.3.77. Explicit tool paths select the pinned Node/browser instead
of Nix's newer default Node. Per-package600s and global3600s budgets remain
unchanged. Driver monotonic wall times are699.4814354679547s Node and
701.7669378319988s Chromium; these are execution durations, not performance/deadline
measurements. Native and package-Wasm checks ran while the isolated Miri lane ran.

The production worklet asset is intentionally **reused**, not relabeled as a fresh
build. Both reports retain its actual manifest source `dfd610d9`, matching input
hash `f84144c75f79007b0a3f8e0819436e16c0795ad22f5622e7adb148aab17cecd8`, and
119154623-byte Wasm SHA256
`f14cf70c8a17e49140fb2b2cfa4588107b116efcb9f18d4c54106234ec475bc6`.
The fresh package executions are distinct from future explicit application builds,
packaged smokes and independent non-isolated loop AudioWorklet controls.

One one-off preflight initially treated Cargo's nullable package metadata as a
dictionary. It failed before invoking a Wasm suite; the helper now uses the same
`metadata or {}` handling as the existing orchestrator. That failed setup remains
recorded; no project behavior or test workload changed. Previous verified report
directories were preserved separately before the fresh full runs; only new reports
are packaged here.

## Interpreter, base and outstanding delivery

Supported status at **2026-10-28T20:31:19.715900Z** still reports
`20261028T190146-abeee2e59e` **running**, frozen at `8633d0f0`, full151-command/
304-execution selection. No completed count or terminal pass is inferred. The
source comparisons above support possible final reuse only after complete terminal
verification/collection and exact relevant source/configuration/inventory/toolchain
comparison. They do not replace it. No extra Miri job was launched for these
native/browser/example/prose-only gates;900s workloads/guards/seeds remain unchanged.

The parent PR recheck finds [#878](https://github.com/SanderVocke/shoopdaloop/pull/878)
**OPEN**, base `master`, remote head `cc6ccb4ac35114d61ef7e1c03ecc7f2fe81bb867`;
no loop-branch PR exists at this checkpoint. This live remote result supersedes
assumptions about later local parent heads. Delivery should target the unmerged
`feat/generic-rt-runtime` parent unless it changes before push. Existing workflow
PR branch filters target master; checked final-branch workflow dispatch/CI evidence
will be needed if those filters do not run for the parent-targeted PR. Do not push
the parent, merge either PR or invent final-head green/reviewer approval.

## Artifact

`final-native-wasm-stage10-logs.tar.gz`: **486851bytes/149files**, SHA256
`a04bb7df6098f1275f9bbe604b351d3b55a8d7b029b2605200d7d981751a3dae`.
Every archived member was byte-verified. It includes all native/raw Wasm logs and
structured results/JUnit, full package/native discovery,509-input comparison,
version/checksum/skip/identity proofs, failed preflights and original commands,
parent PR metadata and both pending interpreter checkpoints. Native log hashes and
both complete Wasm summary hashes were rechecked before packaging. No source,
binary or documentation freshness claim extends beyond the stated scopes.
