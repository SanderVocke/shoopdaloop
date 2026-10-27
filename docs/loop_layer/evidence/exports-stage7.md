# Stage 7 immutable exports and native host-row completion

## Source and scope

The implementation is the containing commit, based on
`ebe14e4e915911fc277a07f1f379be54211eb743`. Fresh structured reports retain that
base with `source_dirty: true`: testing preceded the containing commit. Production
runtime/loop Rust, manifests, lockfiles and dependencies did not change.

The fixture adds `BrowserExportClient`, opt-in `BrowserBridge.new_exports`,
canonical96-byte `LLE1` requests/1024-byte `LLF1` results, one independent command
credit, two retained receiver-local export banks and a reusable64-byte owning
resource-read buffer. Scalar admission and terminal execution remain distinct;
read returns its actual owning buffer. Export IDs/cursors stay receiver-local;
no Rust owner/pointer/layout crosses memories. Normal stop refuses15 while an
export remains retained. Successful releases advance both banks to generation2.

Native direct and capacity-one actor replay, Node/Chromium public bindings and
actual offline/live separate-memory worklets execute one complete script:

1. Record0/Play4/Stop12; coherent Export Latest at8 retains source cuts4 and
   capture stamps0–4, logical content revision1 and stereo1,2,3,4/right×10.
2. Keep that owner while Record128/Play132/Stop140 creates revision2 with distinct
   stereo10,20,30,40/right×10. Export at136 retains source cuts132/stamps128–132.
3. Read both retained immutable exports through actual resource cursors. Require
   exact32-byte stereo data, original rebased MIDI note-on0/note-off2 for revision1,
   empty MIDI for revision2, and complete656-byte initial projections for **all16
   channels of each snapshot**, including every note/controller, program/bank,
   pressure/pitch, reset and parameter-history field.
4. Explicitly release both, require no commands/owners/missing transfer buffers,
   and permanently stop with256 pages/16 versions/16 snapshots/0 media references.

All44 accepted requests/84 export results/twelve primitive results,256 audible
samples,3930 MIDI journal bytes and4840 semantic bytes compare completely. Native
checks use independent sample/data/projection expectations and the existing manual
MIDI/semantic table extended for the second recording. Raw resource append revisions
remain truthful diagnostics: callback partitions can alter append history, so only
the partition-matrix test excludes those raw descriptor fields. Public content
revisions/media/timeline never normalize. Fixed128-frame native direct/threaded,
bindings and worklet comparisons retain every descriptor/result byte, normalizing
only the known actual-worklet session nonce.

Actual worklets advance44 explicitly acknowledged128-frame calls/5632 engine
frames. The driver deliberately pauses the engine between calls; Web Audio emits
silence without hidden engine advancement. Actual downstream probes match both
audible source clocks and all256 samples/channel; OfflineAudioContext additionally
checks **all11520 frames/channel**, including idle/query/stopped silence. These
pauses do not prove continuous live editing or MessagePort delivery deadlines.
One96-byte export input/two1024-byte export responses transfer/detach/return
separately from primitive buffers and Rust command/owner credits. A malformed
reserved byte and foreign-session request return4/9 and retry the original sequence;
foreign/duplicate copies of every actual export reply cannot consume extra credit.

The recorded storage tuple `[16384,64,96,1024,2,1]` gives actual prepared resource
backing bytes, reusable read bytes, request/response sizes, retained banks and
command credits. It is a component footprint, **not total native heap/RSS**.
Guarded native/render paths remain allocation/reallocation/deallocation-free;
ordinary worklet memories remain fixed. JS allocation/GC/scheduling are outside
that Rust guarantee. Actor coordination/waits/joins remain outside callbacks.

## Verification

| Gate | Fresh result |
| --- | --- |
| Standalone native debug/release | 47 passed/profile, zero ignored, zero doctests |
| New native cases | Two full host/lifecycle cases and two codec/client cases |
| README exact export filter | One execution/pass per profile |
| README direct/two-thread export JSON commands | Both modes/profile executed |
| Node/Chromium bindings | Fifteen shared groups/runtime/profile; complete export report equality |
| Actual successful sessions | Primitive, graph and export offline/live: six/profile |
| Actual intentional failure gates | Four kinds/profile, exact nonzero exits/diagnostics/Rust witnesses |
| Python decoder/report/failure tests | Thirteen pass |
| Workspace nextest | 2141 passed/four existing skips,34.203s summary |
| Warning-denied workspace build/docs | Pass, including fixture rustdoc |
| Format/test-usage/dependency boundaries/tracing | Pass; production tracing inventory unchanged |

The full native export case compares `[128]`,128 singles and irregular zero calls
through direct and bounded-threaded hosts. The other new host case verifies a
cancelled owning read, stalled return/Busy without credit loss, subsequent actual
read/EOF, cancelled Release preserving the owner/stop refusal, and128 real
Export/Release reuses ending at generation130 with full recovery. Codec/client
cases cover all request/result truncations, canonical padding, full-width values,
projection validation, foreign/stale replies and nonwrapping sequence/generation
exhaustion. Two new Python mutation cases reject missing/corrupted content, clocks,
transactional refusals, memory/owners and actual output. Their3966-byte gzip native
input is **parser fixture data**, not a fresh execution or worklet substitute.

Both profiles' actual export contexts finish5632 frames/44 callbacks:

| Profile/context | Audible browser starts | Worklet bytes, initial=final |
| --- | --- | ---: |
| Debug offline | 128,384 | 1,835,008 |
| Debug live | 1536,2048 | 1,835,008 |
| Release offline | 128,384 | 1,769,472 |
| Release live | 1024,1536 | 1,769,472 |

All guarded heap counts are zero; all successful sessions fully recover. Live
origins/gaps are scheduler-dependent, not musical time or latency measurements.
Fresh Wasm SHA-256:

- Debug `71e3d43c31c2fff152ccb6761d9e5c26f604f7e7b47f34798fa2a71909161a50`
- Release `d2a44a2c00cc8a397e3be807c062e31c6da2341adc7ed48f1815633f65b3b47f`

Fresh intentional failures all exit1, both profiles:

| Kind | Rust result | Heap alloc/realloc/dealloc | Processor origin |
| --- | ---: | --- | ---: |
| processor | 10 | 0/0/0 | 1 |
| rust-heap | 4294901761 | 4/20/4 | 2 |
| graph-processor | 10 | 0/0/0 | 1 |
| graph-rust-heap | 4294901761 | 1/5/1 | 2 |

These are the actual primitive/graph guard canaries, not newly invented export-
specific failure kinds. Failed sessions do not substitute for successful export
release/recovery. Warm test times are not performance/deadline/CI-margin claims.

Initial implementation attempts remain in the archive: a double-reference type
error and conflicting borrow were fixed; the first partition assertion also
compared append-history resource revisions, which legitimately vary with callback
partitioning. That diagnostic distinction was corrected explicitly, without
normalizing musical content revisions or weakening any guard/workload/watchdog.
Final full gates passed after all behavioral changes. Subsequent changes were
prose; new documented commands/exact filters and docs/tests were run afterwards.

## Commands, artifacts and remaining work

Commands ran through `nix develop --command ...`, with `RUSTFLAGS="-D warnings"`,
`RUSTDOCFLAGS="-D warnings"` and prescribed nextest0.9.116. Pins remain rustc/cargo
1.97.1, wasm-pack0.15.0, host-only wasm-bindgen0.2.127, Node22.22.2 and
Chromium/ChromeDriver147.0.7727.137.

```sh
cargo fmt --all
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml \
  browser::native_export::tests::immutable_exports_hold_both_revisions_across_native_direct_and_bounded_threads -- --exact
# Repeat the exact filter with --release before its name.
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --export-json
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --export-two-thread-json
# Repeat both modes with --release before --bin.
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
cargo build --locked --workspace
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
```

[`exports-stage7-logs.tar.gz`](exports-stage7-logs.tar.gz) retains initial/refined/
final native/workspace/docs logs, documented-command JSON, and complete fresh
native/Node/Chromium/worklet reports including all intentional failure sessions.
SHA-256 `3a263abe3997dc00f7b2c2e9ab0c6ec7be97efcfe3ecb6ccc3f29214b7410cfd`.
Local originals: `target/loop-layer-export-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.

Stage7 is now **4/8**: native offline/two-thread/export evidence joins fixture,
real-context/reference comparison and fixture documentation. Program/resource
upload and program staging, broader musical/failure/stall scenarios, planner-error
canaries, consolidated Stage7 Miri, CI artifacts and Stages8–10 remain incomplete.
No new Miri/benchmark/remote-CI/review/production-smoke pass is inferred. Four new
export-focused native cases join19 graph-focused unregistered/uninterpreted cases;
other applicable standalone ingress/observer debt remains. Preserve the consolidated
stage cadence, complete workloads/guards and900-second watchdog.
