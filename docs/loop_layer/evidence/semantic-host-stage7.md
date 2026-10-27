# Stage 7 semantic host journal milestone

## Source and scope

Source is the commit containing this report, based on
`25b82940496cd4b4df4cf5489366921ac221d976`. Raw host reports record that base and
`source_dirty: true`, not a claim that these changes were already committed.
No production runtime/loop Rust implementation or dependency changed.

This completes the real-context/reference-comparison checklist row, alongside the
already completed fixture/runner foundation. **Stage7 remains incomplete**:
graph/program/resource payloads, owning returns, exports, broader musical and
adversarial scenarios, dynamic live ingress and CI artifacts remain open. No new
Miri, benchmark, hosted application smoke, remote-platform CI or review pass is
claimed. Applicable interpreter cases belong to consolidated Stage7 verification.

## Implemented observation contract

The test-only observer prepares and touches a fixed `LSJ1` buffer: a40-byte header
and capacity for128 records of160 bytes. It records a whole boundary marker,
all decision snapshots, then all ordered semantic events. Meaningful observation
points include events, playback discontinuities and changes in resolver journal
loss. Ordinary callback cuts are omitted.

Snapshots preserve the public timeline-state fields, discontinuity and origin
presence; opaque RequestId values and the private revision watermark are excluded.
Routing/graph epochs, gain and resource metadata are not this projection. The
codec supports every public event variant and composite-state fields, but codec
coverage alone does not imply actual composite/replacement/capture-failure host
execution.

If the whole boundary batch cannot fit, none of it is written. Saturating counters
retain observer-dropped boundaries/records separately from the high-water mark of
resolver-reported lost events. Loss never changes music or receipts. A counter
reset cannot erase a previous indication of incomplete evidence.

Observation occurs after settlement and before rendering: committed records can
remain after a subsequent processor error. A complete journal is not, by itself,
proof that the following audio interval succeeded. Successful host reports require
successful rendering, complete media/receipts, zero diagnostic loss and full stopped
recovery independently.

Native drivers extract bytes from the stopped observer off-render. The browser
copies them into separately prepared local storage after stopping callbacks, then
transfers them as plain bytes. No Rust pointer or owner crosses the instance boundary.
This is stopped diagnostic extraction, not live telemetry or immutable media export.

## Independent oracle and exact results

The independent byte table predicts these boundaries:

| Frame | Final mode | Position/cycles | Events |
| --- | --- | --- | --- |
| 0 | Recording | 0 / 0 | Record, Pulse |
| 4 | Playing | 0 / 0 | Play, Pulse |
| 8 | Playing | 0 / 1 | Wrap, Pulse |
| 12 | Stopped | 0 / 1 | Stop |

It also checks content/capture lengths and revisions, last pulse, flags and all
reserved bytes. Stop suppresses the otherwise-due wrap/pulse at12. The complete
stream has15 records: four boundaries, four decisions and seven events, totalling
2440 bytes. Native direct/wire/two-thread replay across all four callback partitions
and actual offline/live worklets in both profiles compare every byte to this table.

All four final worklet streams have SHA-256:
`1c37c5d4f3607432c147852004a985b6ad2f6f3d1cf369287680ed9e9d5899dd`.
The existing complete13-frame stereo,134-packet MIDI and three requested/delivered/
terminal receipt oracles still pass. Actual downstream Web Audio samples and the
full512-frame OfflineAudioContext result are checked separately.

The same-position Seek case at frame5/position1 has a genuine playback
restart/discontinuity without changing timeline position or emitting a Transition
or Pulse. Native and Node/Chromium binding tests require its eventless boundary
and decision, producing17 records while preserving the original seven events.
This is not yet a new actual worklet seek scenario.

## Fresh verification

| Gate | Result |
| --- | --- |
| Fixture native debug/release | 24 passed per profile; no skips; zero doctests |
| Direct/wire/two-thread replay | Six mode/profile combinations; four partitions each |
| Node public-binding Wasm checks | Nine named checks per profile |
| Chromium public-binding Wasm checks | Nine named checks per profile |
| Actual worklets | Offline/live pass in both profiles, with complete semantic byte equality |
| Actual processor/heap failure canaries | Both kinds exit1 with specific Rust-origin witnesses in each profile |
| Python decoder/failure-gate tests | Five pass |
| Root workspace nextest | 2141 passed/four existing ignored failure canaries;33.363s summary |
| Warning-denied builds/rustdoc, formatting, policies | Pass |

The five new native tests cover:

- All14 event encoding variants with full-width fields and canonical padding.
- Full-width decision scalars, optional pulse/composite fields and absent-value zeroing.
- Sticky/saturating loss, including a resolver counter reset.
- A72-run matrix: observer capacities0,1,4,8,14,128 × resolver journal capacities
  0,1,1024 × four host partitions. Media, receipts and complete pool recovery stay
  unchanged; complete/partial record prefixes and loss counts are checked.
- The eventless same-position seek decision.

The baseline/native binding tests now compare semantic bytes as part of their full
reports. Shared Wasm binding checks also verify semantic loss while repeated
playback exhausts diagnostic storage without changing any audio sample or command
receipt. They are binding-level Wasm checks, not24 Rust test functions interpreted
as package-Wasm tests.

Python validates canonical headers, exact lengths/counts, strict boundary ordering,
child frames, decision identities/optional fields, event tags/arguments and zero
padding. All520 truncation lengths of a synthetic full-width journal, oversize
records, corruption and explicit incomplete-report handling are exercised. The
runner rejects incomplete data before comparing it byte-for-byte with native output.
`LLH2` adds `semanticBytes` and a validated summary; historical `LLH1` artifacts
remain unchanged.

### Final browser observations

| Profile | Worklet initial = final bytes | Offline callbacks/frames | Live callbacks/frames |
| --- | ---: | ---: | ---: |
| Debug | 1,835,008 | 2 / 256 | 4 / 512 |
| Release | 1,769,472 | 2 / 256 | 4 / 512 |

Both contexts preserve ordinary independent memories, returned transfer buffers,
zero successful-render heap counts and complete256page/16version/16snapshot recovery
with zero media references. Offline first engine output is at browser frame128;
these final live runs start at browser frame0. Live startup/callback counts remain
schedule-dependent and are checked through the observed clock mapping, not assumed.
Unchanged memory size is an observation, not an optimization or deadline claim.

Final Wasm SHA-256:

- Debug: `0dbbefc9627fa7ef3def375ff92aab4890ee05a76283e7013b20c6390437865e`
- Release: `143c4ed7a2060bcb532df9f0c23bcf28ed5bb13025ab307ce1ae5b8635a77292`

## Commands and tools

Commands ran inside `nix develop --command ...`, with `RUSTFLAGS="-D warnings"`
and warning-denied rustdoc. Final logs record rustc1.97.1, cargo1.97.1, prescribed
nextest0.9.116, wasm-pack0.15.0, Node22.22.2 and Chromium/ChromeDriver147.0.7727.137.
Host-only wasm-bindgen remains0.2.127 and rtrb remains0.4.0. The provisioned browser,
Node and nextest directories were on PATH. Stack limits, guards, workloads and
watchdogs were not relaxed.

```sh
cargo fmt --all
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
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

Both final host commands build their profile's locked artifacts before executing
Node, Chromium, actual worklets and native reference/replay. Only the intentional
failure children reuse their parent's just-built artifacts. No native replay is
labelled a fresh worklet execution.

## Retained failure and artifacts

The initial native build failed with two `E0716: temporary value dropped while
borrowed` errors in the independent oracle's conditional temporary event arrays.
A local fixed array now outlives the bounded slice iteration. This was a test-helper
lifetime error; no expected values or runtime assertions were weakened. A later
`git diff --check` found trailing whitespace in the native JSON print statement;
it was removed without changing the format or behavior.

[`semantic-host-stage7-logs.tar.gz`](semantic-host-stage7-logs.tar.gz) retains the
failed build, intermediate checks, final native/workspace/doc logs and complete
final browser/Node/native/canary reports with WebDriver/console output. SHA-256:
`26c6f96c77f50d0d52efde17a12245b40952a139af54b99e2b71aa508d2f7576`.
Local originals: `target/loop-layer-semantic-host-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
