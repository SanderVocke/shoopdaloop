# Stage 7 actual graph-change AudioWorklets

## Source and completed scenario

Source is the commit containing this report, based on
`ae0ba511340e188d371f58d7a06a65b43aaf17d3`. Reports retain that base and
`source_dirty: true`: testing preceded the containing commit. No production
runtime/loop Rust, manifest, lockfile or dependency changed.

This milestone completes an **actual offline/live graph-change scenario**, not
another binding-only prerequisite. Both profiles transfer prepared descriptors,
Stage/Activate/Release commands and owning results between independent ordinary
Wasm memories through MessagePort. Downstream Web Audio samples, native/reference
payload/command/result bytes, MIDI, semantic records, epochs, ownership and successful
cleanup are independently checked. Actual graph processor/heap failure canaries
also run in both profiles, separately from successful sessions.

Stage7 nevertheless remains2/8 checklist rows verified: native bounded two-thread
graph payload transport, program/resource staging, immutable exports, broader
composite/seek/replacement/capture-failure/stall scenarios, consolidated Miri and
CI artifacts remain open. Stages8–10 remain open. No new interpreter, benchmark,
production application smoke, remote CI or review pass is claimed.

## Transport, real Ready checkpoint and timing limits

`graph_host.js` extends the existing host with one96-byte payload transfer buffer,
two96-byte graph command buffers and four128-byte result buffers. Primitive pools
remain five64-byte inputs/ten96-byte results. These are finite **application-level**
credits; MessagePort itself is not claimed to be fixed-capacity. The producer awaits
each payload return. Every transfer detaches the sender, and every returned buffer
is checked before reuse/normal stop. Records contain no transferred owner/pointer.
All preparation-free Rust paths remain guarded; JS allocation/GC and scheduling
are outside that heap guarantee.

1. Prepare both instances before measuring fixed memory. The control client compiles
   the fixture bypass descriptor off-render. Send all20 `LGP1` records, then Stage,
   alongside Record0/Play4/Stop12. Real malformed payload/foreign command probes
   return4/9 and retry the original sequences without advancing the receiver watermark,
   losing the descriptor or releasing client credit.
2. `prime` requests exactly **one real audible engine sample0**. Its output1/10 and
   MIDI are included in every oracle. Only after rendering does `checkpoint` name
   engine frame1 and its browser quantum. This is not a discarded warm-up.
3. The source deliberately pauses engine advancement at1 while further Web Audio
   callbacks output silence. Consume the actual Staged/Ready result, then ingest
   Activate4 and wait for its envelope acknowledgement before `arm` resumes the engine.
   The next128-frame call covers1–128 and activates three samples inside it.
4. The separate downstream probe retains two non-silent128-frame Web Audio quanta
   in fixed arrays and checks both starts against source clock markers. The first
   contains engine sample0 plus127 checked silent padding samples; the second
   contains engine samples1–128. Their129 real engine samples match native replay.
5. After music has stopped, Release acknowledges its actual enqueue frame. Its
   owning completion must report that same frame, return next bank generation2 and
   actual retired capacities3/12/2. All command/retained counts and every transfer
   pool must recover before permanent stopped cleanup.

The wall-clock gap is an explicit **test-driver engine pause**, not continuous
wall-clock live editing/device capture or evidence that MessagePort meets future
deadlines. It consumes no hidden musical frames and adds no runtime interaction
quantum, transition cap, dropped semantics or rollback. The separate primitive
worklet scenario continues to verify contiguous128-frame processing after arm.

## Complete output/reference/ownership checks

Graph-route left samples0–12 are:

```text
1,2,3,4,1,2,3,4,1,2,3,4,0
```

Right is ten times left; samples13–128 are zero. The first four captured FX samples
remain1,2,3,4; the host route bypasses half-gain FX from4 while the capture connection
remains on FX. This scenario does not add immutable capture-export evidence.

All134 MIDI packets/2010 journal bytes match, including all16 channels' mandatory
restoration at4/8. All2440 semantic bytes match the independent primitive table:
Record/Pulse0, Play/Pulse4, Wrap/Pulse8, Stop12 suppressing the due wrap/pulse.
The timeline projection deliberately omits routing/graph metadata; actual published
graph epoch2 and requested/effective/delivered/terminal activation4 are independently
required in graph results.

The native `replay --graph-json` records all129 samples,20 payload records, three
graph commands, four graph results, six primitive results, full MIDI/semantic data
and observed recovery. Its prime1 + resume128 + release1 gives frame130/three
callbacks; release is nonmusical and its final sample is separately asserted silent.
A new guarded native test compares `[128]`,128 singles and `[0,3,0,5,120,0]` after
the common prime. Zero calls are inert; every native partition matches complete
media/journals/bytes/recovery, ignoring only opaque per-instance graph scope.

The runner checks complete actual payload/command/result bytes against fresh native
replay and independent complete graph response tables. Normalization is limited to:

- Known per-session routing nonces, which are separately validated at full64-bit width.
- Opaque local graph scope, with exact Ready-ID correlation in Activate/Release.
- The nonmusical Release completion frame, required to equal its actual ingress
  acknowledgement on the engine callback clock.

Musical frames, runtime/command sequences, admission/effective/delivered/terminal
activation values, graph/topology epochs, generations, capacities, flags and padding
remain exact. Primitive reply interleaving can depend on polling; each of three
requests still requires admission before execution and all six byte records must match.
Each actual graph reply additionally rejects a foreign-session copy and a duplicate
without consuming extra credit. No abandoned owner/result is presented as returned.

Offline also compares **every sample of both complete768-frame output channels**,
including startup, deliberate pause and stopped silence. Live mute is downstream
of the probe. Main marker`0x51a17`/worklet marker0, non-shared ordinary memories,
non-isolated page/no COOP/COEP, prepared/final memory, zero heap/overflow, all input
pools5/1/2, response pools10/4, five zero command/retained counters and actual
256 pages/16 versions/16 snapshots/zero references are required.

## Failure canaries and report tests

Each profile now requires four intentional **actual worklet** failures:

| Kind | Rust result/origin | Final heap witness | Exit |
| --- | --- | --- | --- |
| primitive processor | 10 / 1 | `[0,0,0]` | 1 |
| primitive Rust heap | `0xffff0001` / 2 | `[4,20,4]` | 1 |
| graph processor | 10 / 1 | `[0,0,0]` | 1 |
| graph Rust heap | `0xffff0001` / 2 | `[1,5,1]` | 1 |

The graph faults occur in the real one-sample prime with a graph command outstanding;
the primitive callback visits four DSP intervals, explaining its larger heap counts.
Matching text/nonzero exit alone is insufficient: exact diagnostics and independent
Rust result/origin/heap witnesses are required. Failed sessions are not successful
cleanup evidence; no graph return is invented on their behalf. Planner-error canaries
and broader failure campaigns remain later Stage7 work.

Three new Python cases require both actual contexts, all downstream samples/clock
segments, real transactional refusals, correct sessions/Ready identity, primitive
phase order, fixed memory/heap, buffers, terminal clocks and resource recovery.
Existing failure-gate tests now cover all four kinds. The committed
`scripts/tests/data/loop_graph_native.json` is native report input for synthetic
parser tests, not a fresh worklet pass or an additional musical oracle.

`browser::native_graph::tests::acknowledged_graph_worklet_script_has_complete_partition_invariant_native_reference`
is the one new native case. All18 graph-focused native cases remain unregistered/
uninterpreted Stage7 debt in [MIRI_JOBS](../MIRI_JOBS.md); no routine per-commit
interpreter invocation was launched. Keep the900s watchdog, workloads and guards
for the required consolidated applicable snapshot/run.

## Final gates and observed clocks

| Gate | Result |
| --- | --- |
| Fixture native debug/release | 42 passed per profile; zero skips/zero doctests |
| New native reference/partition exact filter | One test executes/passes per profile |
| README `--graph-json` commands | Run/passed in both profiles |
| Node/Chromium public binding inventory | Fourteen groups per runtime/profile |
| Actual successful sessions | Primitive offline/live + graph offline/live per profile |
| Actual failure gates | Four kinds above per profile, all exact witnesses/exit1 |
| Native primitive direct/wire/two-thread | All six mode/profile combinations/four partitions |
| Python report/failure/decoder tests | Ten pass |
| Workspace nextest | 2141 passed/four existing skips;33.355s summary |
| Warning-denied workspace build/rustdoc, formatting and applicable policies | Pass |

Code-complete full native/workspace gates precede the prose-only guide updates;
new runnable reference commands, exact filters, warning-denied build/rustdoc/policies
and fresh browser builds/sessions were checked afterwards. All captured build/test
commands passed. No assertion, workload, guard, stack limit or watchdog was weakened.
These warm observations are not timing benchmarks, deadline or CI-margin claims.

| Profile/scenario | Prepared = final main/worklet bytes | Engine frames/callbacks | Browser starts | Release enqueue/terminal |
| --- | ---: | ---: | --- | ---: |
| Debug graph offline | 1,835,008 | 257 / 3 | 128,256 | 129 |
| Debug graph live | 1,835,008 | 1025 / 9 | 1024,1536 | 513 |
| Release graph offline | 1,769,472 | 257 / 3 | 128,256 | 129 |
| Release graph live | 1,769,472 | 1025 / 9 | 0,1024 | 513 |

Graph source frames always satisfy `1 + (callbacks - 1) * 128`; live origins,
wall-clock gaps and Release times are schedule-dependent, not fixed deadline
assumptions. Final primitive sessions have offline256/2/start128 and live512/4/start0
in both profiles. Normal heap/overflow counts are zero; all successful recoveries
are256/16/16/0. Canonical semantic SHA-256 remains
`1c37c5d4f3607432c147852004a985b6ad2f6f3d1cf369287680ed9e9d5899dd`.

Final Wasm SHA-256:

- Debug `2c54aebfe964d2d640d867a6bf1820d53bb6b9798b6767c43ee3a1e66e454a81`
- Release `92735bce306b8532b0d2f3ad73908996ffb90b2b0fda11821f0195336f059387`

## Reproduce and artifacts

All Cargo/Python gates ran through `nix develop --command ...`, warning-denied Rust/
rustdoc and prescribed nextest0.9.116. Tools remain rustc/cargo1.97.1, wasm-pack0.15.0,
host-only wasm-bindgen0.2.127, Node22.22.2 and Chromium/ChromeDriver147.0.7727.137.
No manifests/lockfiles changed.

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay -- --graph-json
# Repeat with --release before --bin; run the README exact partition filter per profile.
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

[`graph-worklet-stage7-logs.tar.gz`](graph-worklet-stage7-logs.tar.gz) retains initial/
final native/workspace/docs logs, native reference JSON and complete final native,
Node/Chromium/primitive/graph/canary reports. SHA-256:
`b9588b41c2114cc299f80dacf084a689a188bd69470b2df94ecfb25665d3e5dd`.
Local originals: `target/loop-layer-graph-worklet-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
