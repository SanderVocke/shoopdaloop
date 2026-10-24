# Independent rt_runtime host fixtures

Test-only hosts using the public `rt_runtime` API. This separate, non-publishable workspace is not a Shoop backend, device adapter, or production browser package. Native and worklet builds share the processor, wire decoder, prepared ownership pools, and guarded render bridge. The main-thread controller compiles relocatable graph descriptions; it never sends Rust pointers to another instance.

## Run

From the repository root, in its development environment:

```sh
# Self-contained native examples; no browser or Wasm tools required.
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py --native-only --profile debug
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py --native-only --profile release

# Real OfflineAudioContext AND live AudioContext, followed by native replays.
# Requires wasm32-unknown-unknown, wasm-pack 0.15.0, and matching
# Chromium/ChromeDriver 147.0.7727.117 or 147.0.7727.137 on PATH.
# CHROME_BIN may select the browser executable explicitly.
RUSTFLAGS="-D warnings" python3 scripts/run_rt_runtime_hosts.py
```

On NixOS wrap commands in `nix develop --command bash -c '...'`. The helper manifest is outside the main workspace: format it with `cargo fmt --manifest-path tests/rt_runtime_host/Cargo.toml` as well as formatting the main workspace. `--no-build` reuses fixture artifacts; it does **not** skip browser execution unless `--native-only` is also supplied. The `--profile` option selects the native build; full browser runs use release Wasm.

Results, browser/driver logs, build logs, and the actual binary command/response/sample journal go to `target/rt-runtime-host/reports`. The runner fails on assertions, processor errors, timeouts, unexpected callback counts, ownership/memory/work violations, or native/browser differences.

Native-only execution decompresses the checked-in `data/offline.rtj.gz`, verifies its SHA-256, and runs both offline and two-thread examples. That journal was recorded from a real, non-isolated AudioWorklet; replay alone is deliberately **not reported as a new browser test**. To replay a freshly recorded journal directly:

```sh
cargo run --manifest-path tests/rt_runtime_host/Cargo.toml --release --bin replay -- target/rt-runtime-host/reports/journal.bin
cargo run --manifest-path tests/rt_runtime_host/Cargo.toml --release --bin replay -- target/rt-runtime-host/reports/journal.bin --threaded
```

## Scenario and independent checks

The scenario stages and activates a graph, captures audio/MIDI, schedules exact-frame gain and MIDI changes, polls execution separately from admission, rewires a retained processor, exports immutable capture prefixes, replaces 32 KiB of audio, uploads/exports 38,400 bytes of MIDI including explicit trailing duration, and reads an old pinned revision after replacement. It checks cancelled chunks, abandoned uploads, stale graph/stream identities, malformed duplicate graph chunks, and explicit MIDI reset delivery.

Four normal transfers and a fifth independent recovery transfer are simultaneously outstanding. Duplicate-credit and oversized-envelope probes return the offending buffer without replacing an existing owner. All five returned buffers are then withheld while rendering continues for at least sixteen quanta. A packet constructed before that stall is sent afterward; both its lateness and actual execution frame must be reported.

OfflineAudioContext is suspended at quantum boundaries to make ingress deterministic. Its **actual output buffer**, not an imitation of `process`, is checked sample-by-sample against the graph/gain/resource timeline. Every callback records admitted packets, all response bytes, and a hash of that callback's actual Web Audio samples. Native hosts compare each response byte and each callback sample hash, plus aggregate MIDI/graph/sample hashes and work counters. The native controller independently recompiles the graph variants and compares the descriptor-stream hash. These are exact comparisons: this fixture's constants/routing use exactly representable `f32` values.

Live AudioContext runs the same operations without a deterministic external clock. Its acknowledged frames determine MIDI/capture expectations; the analyser must receive the expected steady output after replacement. Live totals/hashes are not falsely equated with offline totals when message delivery adds idle callbacks.

The two-thread native example uses a bounded public SPSC transport. Atomic offline-clock coordination/yielding occurs **outside** the guarded callback; it is not a device driver or a claim about operating-system scheduling. The callback itself never waits for the producer.

## Bounds, ownership, and shutdown

Configuration and protocol details are in `docs/rt_runtime/HOST_FIXTURE.md`. Each render-side ingress, callback, and completion-recycling scope is guarded against allocation, reallocation, and deallocation. Positive canaries run when each bridge is prepared, including optimized Wasm. All plan/payload/response owners are prepared in advance and recycled. A terminal consistency fault parks its owning completion until non-callback destruction rather than dropping it on the render path.

The worklet uses ordinary independent Wasm memory, five fixed 512-byte transferable buffers, and a fixed five-entry ingress queue. An instance-global marker proves that changing main-thread instance state does not change worklet instance state. The wrapper checks memory size after every callback. A dedicated localhost server sends neither COOP nor COEP; the page asserts `crossOriginIsolated === false`.

Before stopping, the scenario removes the processor, releases its playback lease and every export, removes all resources, and drains bounded retirement. It verifies all 4096 pages and all 16 version/snapshot slots are free, with no outstanding transfers/cursors/staging plans. Six plans are free and the seventh is the active empty graph. Only then does the worklet acknowledge stop, free Rust owners from a message handler outside `process`, and finish/close its context. The offline output tail must remain silent after disposal.

JavaScript message delivery, structured cloning, object creation, and GC are **not** hard-real-time. Transfer bounds assume a producer obeying credits; a hostile caller can flood a browser's own MessagePort queue outside this protocol. The UTF-8 fallback is needed by Chromium 147's AudioWorklet global for binding error messages; reports record its use. Successful rendering uses numeric bindings, not string decoding.

## Journal format and provenance

`RTJ1`, little-endian `u32` callback count (1–4096), then for each callback:

1. `u32` ingress mask, followed by increasing-slot packets: `u32` length and that many bytes.
2. `u32` return mask, followed by increasing-slot replies in the same length/bytes format.
3. `u32` FNV-style hash of the 128 actual sample bit patterns.

Only five mask bits are valid; packet lengths are 48–512 bytes. The native reader rejects truncated/trailing data and limits journal size to 64 MiB. Generation of the golden artifact is reproducible with `gzip -n -c target/rt-runtime-host/reports/journal.bin`; update it only after an actual successful browser run and review the corresponding source/evidence changes. Its uncompressed SHA-256 is `fb8ecf1fae2cb7ac7246bbb836d2ed7670f9d1341b8c005ab3d2565f07ee6237`. Checked-in stage-6 results are in `docs/rt_runtime/evidence/hosts-stage6.json`.
