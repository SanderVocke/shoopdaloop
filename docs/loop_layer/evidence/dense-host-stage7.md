# Stage 7: dense musical progression across actual hosts

## Scope and source

This milestone adds an opt-in prepared five-loop scenario to the independent
`tests/loop_engine_host` fixture. It uses the actual loop facade and unchanged
LLC1/LLR1 primitive protocol, not a replacement resolver or the production backend.
The native direct/capacity-one actor hosts, public Node/Chromium bindings and actual
non-isolated offline/live AudioWorklets all run the scenario in both profiles.

Recorded source base: `be83807f985f42e72425d4b2af154679d63f3788`, with the dense
milestone implementation dirty (`source_dirty: true`). These are local milestone
results, not final-head CI or a new interpreter/performance campaign. No production
library Rust, manifests, dependencies or lockfiles change in this milestone.

Raw logs and final reports: [`dense-host-stage7-logs.tar.gz`](dense-host-stage7-logs.tar.gz),
1,943,565 bytes, SHA-256
`4d834cd933ced8fcb743fe4f6e127dea181d4c103b3bf59cd42c8c087c64118d`.
The archive root is `loop-layer-dense-stage7/`; `final-reports/{debug,release}`
contains the final browser/native/Node reports and four failure-child reports per
profile. Earlier failed attempts and intermediate logs remain separate and intact.
The compressed parser fixture `scripts/tests/data/loop_dense_native.json.gz`
(34,660 bytes) is only mutation-test input, not a substitute for execution.

## Musical and ownership contract exercised

- Four requests are admitted before rendering: Record0, Play1, same-position
  Seek64, and Seek130. A fifth normal request refuses Busy10 transactionally.
- A captures one stereo/MIDI frame. Playback begins immediately at1, passes through
  half-gain FX, and continues through128. The seek at64 is a real discontinuity
  that suppresses that frame's natural wrap/pulse while preserving exact media.
- A's Record-start pulse starts timing-only B→C at frame0. B's one-frame clock
  drives a regular→explicit composite pair. The regular cursor makes32 complete
  passes; the explicit cursor reaches its end at4, while C stays playing under
  the combined trigger/program semantics. This is not an audible composite-stop
  claim, countdown coverage, or a nominal fallback-clock scenario.
- No results are polled through128+1 frames. All four normal command credits remain
  held, including the future Seek130; successful stop refuses15 without consuming
  the engine. Independent recovery Stop129 is then admitted before a two-frame
  callback. It suppresses A's due wrap and emits MIDI cleanup. Seek130 returns
  domain InvalidState261, not a renderer fault. B/C and composite clocks continue.
- All five command credits remain held until ten admission/execution records are
  polled. Native and binding duplicate receipts refuse State11 without releasing
  another credit. Every request/result buffer returns before successful stop.
- Cleanup/reclamation recovers256 resource pages,16 versions,16 snapshots and zero
  media references. Native actor waits/joins remain outside guarded callbacks.

Independent native tables check every stereo sample, MIDI byte and semantic record:

| Observation | Complete expectation |
| --- | --- |
| Stereo | 131 samples: `[1,10]` at0, `[0.5,5]` at1–128, silence at129–130 |
| MIDI | 8,449 packets /126,735 bytes, including complete neutral restoration, original note, wrap/seek cleanup and final Stop cleanup |
| Semantics | 1,968 LSJ1 records /314,920 bytes:131 boundaries,655 complete decisions,1,182 ordered events |
| Requests/results | Five64-byte requests, ten96-byte results; exact phases, sequence/slot, requested/effective/delivered/observed clocks, statuses and domain failure261 |
| Ownership | Four report counts zero;256/16/16/0 stopped recovery |

`browser/native_dense/expected.rs` builds the closed-form semantic table without
calling the resolver or its encoder. It predicts every boundary, decision field,
ordered event and zero padding. The MIDI oracle separately enumerates packets and
absolute timestamps. Whole, single-sample and irregular partitions with zero calls
produce identical complete reports in both direct and capacity-one actor modes.
The direct-facade test additionally leaves the actual control returns unread,
rather than collecting them into bridge slots during callbacks.

## Actual Web Audio evidence

The actual driver acknowledges128/1/2-frame engine calls, pausing engine advancement
between them. These are test-driver choices, not an interaction quantum, per-hop
musical delay or continuous wall-clock scheduling/deadline proof. Both independent
Wasm instances use ordinary memories in a non-isolated page; the main marker is
`0x51a17`, and the worklet marker remains zero.

A separate mono marker output marks the first sample of each rendered quantum.
The downstream probe uses that sideband to capture the actual stereo buffers even
for the silent recovery quantum. The marker does not alter or calculate stereo
samples. Every one of384 probe samples per channel is checked, including zero
unused tails of the one-/two-frame calls. Offline additionally checks all1024
samples per channel from `OfflineAudioContext` itself. Live output is muted only
after the probe.

| Profile/context | Ordinary worklet bytes, unchanged | Actual quantum origins |
| --- | ---: | --- |
| debug offline | 5,963,776 | 128,384,640 |
| debug live | 5,963,776 | 1536,4096,4608 |
| release offline | 5,898,240 | 128,384,640 |
| release live | 5,898,240 | 1024,1536,2048 |

The main memory is the same size as the worklet memory for each recorded dense
profile and is independently checked unchanged. All callbacks have zero Rust
allocation/reallocation/deallocation counts; journals report no overflow/loss.
Actual reports retain both stopped-credit probes (frame129/four outstanding,
frame131/five outstanding, stop refusal15), exact engine/browser clocks and all
five input/ten response buffers returned before successful cleanup.

Dense prepared capacities are16,384 MIDI events/65,536 payload bytes per runtime
port and host output,262,144 MIDI journal bytes and4,096 semantic records (655,400
bytes). These are finite component capacities, not total native RSS or a musical
work quota. Standard hosts retain their smaller capacities. Observer sizing now
checks u32 record-count representation and checked byte arithmetic off-render.

Final Wasm SHA-256:

- debug: `076c3f67bf4c9d6846585d888edb4b8db68b9f4a097654810cd0e516aec2ad74`
- release: `9a715720d7352d09b5c8d6548272bf20bfb55816e31854b7222d445b317cfd35`

## Tests, commands and retained failures

Two new native tests:

- `scenario::dense::tests::dense_capture_cascades_and_clocked_composites_keep_music_with_unread_results`
- `browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled`

Both profiles pass53 native fixture tests. Seventeen shared binding groups pass in
both Node and Chromium per profile. Each profile passes ten successful actual
contexts (primitive/graph/export/staging/dense, offline/live) and four existing
intentional failure gates. Seventeen Python tests pass, including two new dense
mutation cases rejecting missing/altered media, semantic/receipt data, contexts,
clock alignment/order, stalled credits, silent output/tails, memory and recovery.

Workspace regression passes2,141 tests with four existing skips in33.282s warm
nextest time. This is neither cold-cache nor callback timing/deadline evidence.
Warning-denied workspace/fixture builds, runtime/loop/fixture rustdoc, loop doctests,
formatting, test-attribute policy, both dependency boundaries and tracing coverage
pass. Both documented dense replay modes and the exact native actor filter pass
in debug/release. Main logs are `final-native-workspace.log`, `release-hosts.log`
and `final-debug-hosts.log`; the latter host run refreshes the earlier debug report.

All commands ran inside `nix develop --command ...`, with
`RUSTFLAGS="-D warnings"` and `RUSTDOCFLAGS="-D warnings"`:

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo build --locked --workspace
cargo build --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
cargo test --locked -p shoop_loop_engine --doc
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --dense-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --dense-two-thread-json
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_dense::tests::dense_wire_and_capacity_one_actors_preserve_music_with_all_results_stalled -- --exact
```

The final three documented commands also ran with `--release` before `--bin`/the
filter. Toolchain: rustc1.97.1 (`8bab26f4f`), cargo1.97.1 (`c980f4866`), repository
nextest0.9.116, wasm-pack0.15.0, wasm-bindgen0.2.127, Node22.22.2 and matching
Chromium/ChromeDriver147.0.7727.137. Browser binaries came from the existing pinned
Nix store paths; no browser-absence substitution was used.

The four failure-child gates each exit1 with the specific diagnostic and Rust
witness: primitive/graph processor errors code10/origin1/heap0; primitive Rust heap
code`0xffff0001`/origin2/counts4/20/4; graph Rust heap same code/origin/counts1/5/1.
These are existing processor/heap canaries, **not planner-specific canaries**.
Successful sessions independently prove complete disposal/recovery.

Retained failed attempts:

- Initial dense preparation hit the old128-record observer assertion. The new
  scenario prepares4096 records using checked sizing; no musical workload was cut.
- The native duplicate-response assertion initially expected Sequence8 (and then
  distinguished admission/execution incorrectly). The primitive client contract
  returns State11 for both already-admitted and already-finished duplicates. The
  corrected expectation follows that existing contract; runtime code did not change.
- The first closed-form oracle failed compilation because `129..=>` needed a space
  before `=>`. The corrected oracle passes every byte across all native partitions.

## Remaining acceptance work

Stage7 remains **5/8**, not complete. This milestone supplies dense wraps, same-frame
cascades, clocked composite cursors, actual seek and unread-result recovery. Broader
countdown/composite interactions, prepared playable-media replacement, capture
exhaustion/failure, dynamic live ingress and in-flight transferred-buffer stalls
still need actual-host coverage. The marker/engine-pause protocol does not establish
continuous live-edit deadlines. Rust guards do not cover JavaScript allocation/GC.

The two new native cases bring graph/export/staging/dense debt to29 cases awaiting registration
and interpretation in the consolidated Stage7 Miri selection, plus applicable
ingress/observer debt. No new Miri invocation is claimed. Planner failure canaries,
CI artifacts, Stage8 performance/models/audit, Stage9 final contracts/examples and
Stage10 source-matched final verification, independent audit, unmerged PR, required
final-head CI and review remain open.
