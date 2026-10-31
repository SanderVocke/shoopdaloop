# Stage 7 primitive loop AudioWorklet milestone

## Source and evidence boundary

Source is the commit containing this report, based on
`888b08d9f233191f9c09c1064b94b3caccf09fe6`. The reports truthfully record that base
head and `source_dirty: true`: the implementation was tested before this milestone
commit. Production runtime/loop Rust implementations did not change. The fixture
adds host-only pinned wasm-bindgen0.2.127; rtrb remains0.4.0.

This is **partial Stage7**, not LL-A11 completion or final delivery. It supplies
actual independent loop worklets for the small primitive scenario, not merely
Wasm compilation or the existing generic NoPlanner host. Broader payload staging,
exports, semantic boundary journals, composites and adversarial/live-ingress
scenarios remain open. No Miri invocation/pass, benchmark, hosted application
smoke, Windows/macOS execution, remote CI or review is claimed here.

## Implemented host paths

`BrowserClient` and `BrowserBridge` reuse the native primitive codec/controller
and actual loop facade. The main and AudioWorklet instantiate the same module in
separate ordinary memories. Both buffers must be `instanceof ArrayBuffer`, even
when the global SharedArrayBuffer constructor is unavailable. The main-instance
marker is absent in the worklet; the page is non-isolated and the HTTP server has
no COOP/COEP headers.

Five prepared request buffers and ten prepared response buffers cross MessagePort
by transfer, with sender detachment and return checks. Rust wire credits remain
separate from transport-buffer acknowledgements. Guarded ingress, prepared
submission, rendering, MIDI observation and completion recycling retain their
fixed storage. JavaScript allocation/GC and browser scheduling are not included
in the Rust no-heap claim.

The engine stays at frame zero through silent browser bootstrap callbacks. All
three commands are staged before an acknowledged arm. The first engine callback
records its actual browser frame; a separate downstream AudioWorklet probe must
observe the same start. Thus live timing does not assume MessagePort delivery at
browser frame zero. It also does **not yet test dynamically arriving commands**
against an already-running engine clock.

The probe captures256 actual stereo Web Audio samples. The offline path additionally
checks all512 samples per channel returned by OfflineAudioContext, including the
silent bootstrap/tail. Full samples,134 complete MIDI packets and three exact
requested/delivered/terminal receipt triples match the native/scalar reference.
The six raw admission/execution records and all three requests remain in the
versioned `LLH1` JSON report. The MIDI observation blob is2010 bytes; its exact
record format and ownership boundary are documented in the fixture's WIRE.md.

Successful stop first disables further engine callbacks, drains all results and
returns all transfer buffers, then explicitly cleans up and destroys the stopped
owner. All256 pages/16 versions/16 snapshots return, with zero media references.
The live context closes; the offline context renders the remaining silent tail.
Intentional failure sessions are separately labelled and are not substituted for
this successful shutdown proof.

## Fresh results

| Gate | Observed result |
| --- | --- |
| Fixture native tests, debug/release | 19 passed per profile; no skips; zero doctests |
| Direct/wire/two-thread native replay, both profiles | Six driver/profile combinations; all four partitions agree |
| Public-binding Wasm checks in Node | Seven named checks per profile |
| Same public-binding checks in Chromium | Seven named checks per profile |
| Actual non-isolated worklets | Offline and live pass in both debug/release |
| Actual processor and heap failure gates | Each child exits1 in each profile, with matching Rust-origin witnesses |
| Failure-gate Python tests | Two tests pass; missing/misleading witnesses and ineffective heap counts refused |
| Workspace nextest | 2141 passed/four existing ignored failure canaries; summary32.860s |
| Formatting, warning-denied builds/rustdoc and policies | Pass |

The binding checks are JavaScript-driven tests of the real exported Wasm API,
not a claim that the19 native Rust test functions ran as package-Wasm tests.
They cover constructor refusal, full-width session/frame encoding, bounded normal
and independent recovery credits, malformed/foreign/duplicate receipt rejection,
zero callbacks, full stereo output, fixed memory, stopped recovery and specific
processor/heap faults. Ten repeated playback/stop rounds deliberately fill the
16,384-byte diagnostic MIDI journal while checking all audio samples and retaining
all request receipts; diagnostic exhaustion does not change musical progression.

Three new native binding tests also check complete MIDI bytes against the existing
independent oracle, callback fault witnesses and journal-exhaustion isolation.
The existing three expected-panic allocation/reallocation/deallocation canaries
remain effective in both native profiles.

### Actual final browser observations

| Profile/context | Worklet initial = final bytes | Engine callbacks/frames | Browser first engine frame |
| --- | ---: | ---: | ---: |
| Debug offline | 1,835,008 | 2 / 256 | 128 |
| Debug live | 1,835,008 | 4 / 512 | 0 |
| Release offline | 1,769,472 | 2 / 256 | 128 |
| Release live | 1,769,472 | 4 / 512 | 0 |

The main-instance prepared memory also remains fixed at the corresponding size;
it has already executed the binding checks, so this is not a minimal-memory
measurement. Live startup/callback counts vary with scheduling; an earlier passing
run started at browser frame512. The contract compares the observed clock mapping
and samples, not a fixed live startup frame or callback count. No timing,
optimization, cold-cache, xrun or deadline claim follows from these observations.
Decoder fallback was active in the worklets. Normal render heap counts and MIDI
journal overflow were zero in every successful final session.

The processor canary returns runtime error10 with processor witness1 and heap
counts `[0,0,0]`. The heap canary returns `0xffff0001` with processor witness2 and
observed allocation/reallocation/deallocation counts `[4,20,4]`. It executes in
the real FX processor over the callback's intervals, not a JavaScript substitute.
The gate requires the exact diagnostic, nonzero child exit and structured matching
witnesses; unrelated exceptions, missing bindings, timeouts or text alone cannot
satisfy it.

Final generated Wasm SHA-256:

- Debug: `ca77563aeed3febf39b29759b86ea0ee775830d39f40deb1e0a2dbb91ea1b68e`
- Release: `0b41f5019f38c9e175a4dc7fb3ee56654bbaac6db19470ee1cfb06e912db4fe6`

## Commands and tools

Commands ran inside `nix develop --command ...`. Rust builds/tests/host runs used
`RUSTFLAGS="-D warnings"`; rustdoc used `RUSTDOCFLAGS="-D warnings"`.

```sh
cargo fmt --all
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

The host runner builds locked native artifacts and uses wasm-pack with matching
`--dev`/`--release`, `--target web`, `--no-opt` and Cargo `--locked`. It executes
`replay --json` for complete native comparisons and all three ordinary replay modes.
Artifacts are profile-separated; `--no-build` explicitly means reuse, not a fresh
build. The final runs above did not use that option.

Tools: rustc1.97.1 (`8bab26f4f`,2026-07-14), cargo1.97.1 (`c980f4866`,2026-06-30),
wasm-pack0.15.0, wasm-bindgen0.2.127, Node22.22.2, Chromium/ChromeDriver
147.0.7727.137 and prescribed nextest0.9.116. Browser tools and Node were selected
from their provisioned Nix store paths; the nextest cache was on PATH. No watchdog,
stack setting, callback guard or musical workload was relaxed.

## Retained failed attempts

1. The initial runner directly invoked `wasm-bindgen`, which was not on PATH:
   `FileNotFoundError: [Errno 2] No such file or directory: 'wasm-bindgen'`.
   Native and Wasm compilation had succeeded, but this was not a browser pass.
   The runner now uses the repository's existing wasm-pack provisioning route.
2. The first browser attempt failed with `unsolicited message armed`: the host
   registered command names rather than acknowledgement names. Waiters now match
   `armed`, `polled` and `stopped`. No timing/assertion was weakened to obtain green.

The raw archive retains both attempts and intermediate successful runs separately
from `final-reports/{debug,release}`. `final-docs-and-hosts.log` records the final
source checks after the explicit ordinary-ArrayBuffer assertions and documentation
updates; `final-hosts.log` also records tool versions. Final complete browser,
Node, native and canary reports are included, along with WebDriver/console logs.

Archive: [`browser-host-stage7-logs.tar.gz`](browser-host-stage7-logs.tar.gz), SHA-256
`7b5d15a089828c697927853b04d398d2af1f91ca71dce23d92a3fa123ee07766`.
Local originals are under `target/loop-layer-browser-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.

## Remaining stage work

The fixture/runner foundation checklist row is now complete. The broader combined
rows remain open for graph/program/resource payloads and reliable owning returns,
semantic boundary journals, immutable exports, richer loops/composites and
one-sample cascades, seek/replacement/capture-failure cases, running live ingress
and transfer/result stalls, expanded failure coverage and CI artifacts.
Applicable native fixture cases still need the consolidated Stage7 interpreter
selection; no per-commit routine Miri run was launched. Stages8–10, final source-
matched gates, PR, required final-head green CI and review remain mandatory.
