# Plan: Responsive Browser Session and Waveform Transfers

Prepared according to `.agents/prompts/write_plan.md`, with `.agents/rules/style.md`, `.agents/info/build.md`, `.agents/info/test.md`, and `src/rust/shoopdaloop/README.md` consulted.

## Goals

1. Remove sample-count-dependent work from each session-replacement poll.
2. Replace JSON float serialization with a compact binary session-transfer representation.
3. Replace JSON integer-array bulk chunks with base64 and increase the raw chunk size to 32 KiB.
4. Reduce waveform loading from thousands of frame-gated round trips by using 4,096-sample chunks and up to eight requests in flight.
5. Fix trace-ring wraparound so performance validation captures continuous AudioWorklet evidence.

## Scope

- Browser `RemoteWorkletBackend` session capture/replacement.
- Browser loop-content bulk chunk encoding where it shares the same wire representation.
- AudioWorklet session codec and transfer assembly.
- Waveform request scheduling and assembly.
- AudioWorklet and Worker trace-ring transfer.
- Protocol, native, Wasm, JS contract, and browser verification tests.

The `.shoop` archive format is not changed.

## Immutable acceptance criteria

1. Session replacement remains atomic and preserves all tracks, loops, audio samples, MIDI, processor state, ports, and mappings.
2. An active replacement neither owns a cloned `BackendSessionData` nor compares or serializes its samples again on polling calls.
3. Session capture and replacement use one shared binary codec and round-trip representative backend data exactly.
4. Bulk byte fields use base64 JSON strings, not JSON number arrays.
5. The raw bulk chunk limit is 32 KiB, and the largest valid encoded command remains below `COMMAND_MAX_BYTES`.
6. Protocol compatibility is explicit through a protocol-version increment.
7. Existing generation, ordering, backpressure, cancellation, restart, size-limit, and error-reporting behavior remains enforced.
8. Waveforms remain sample-exact and preserve channel order, offsets, preplay, revision handling, and final completion semantics.
9. Waveform chunks contain at most 4,096 samples, with at most eight waveform requests in flight and existing global command-capacity bounds respected.
10. Trace draining handles a complete record group crossing the SAB boundary without stalling or dropping it.
11. Native backend behavior and persisted session/media formats remain unchanged.
12. All required formatting, warning, native, Wasm, protocol, and browser validation gates pass.

Goals and acceptance criteria may not change without explicit user approval.

## Design rules and constraints

- Keep ordinary control commands and events JSON encoded; only bulk payload contents receive specialized encoding.
- Use the existing ordered MessagePort command/response transport and bounded pending-command queue.
- Use one deterministic, transient binary serde format on both client and Worklet. It is protocol data, not a persisted format.
- Centralize binary codec and base64 wire behavior rather than duplicating configuration.
- Do not add work, allocation, or synchronization to the audio render path. Bulk decoding and replacement remain control-path operations.
- Do not weaken transfer limits when accounting changes from JSON size to binary size.
- Preserve complete-group trace publication and atomic write-index publication.
- Prefer deterministic structural/message-count tests over timing assertions.
- Avoid unrelated refactors and formatting changes.

## Implementation stages

### Stage 1 — Bulk codec and wire representation

- [x] Add a workspace-pinned binary serde dependency suitable for native and Wasm, with allocation support only as needed.
- [x] Add shared binary encode/decode helpers in `shoop_audio_protocol` so client and Worklet cannot select different configurations.
- [x] Add centralized base64 serde handling for bulk `Vec<u8>` fields in:
  - `WriteSessionReplace`
  - `SessionCaptureChunk`
  - `WriteLoopContentReplace`
- [x] Change `SESSION_TRANSFER_CHUNK_BYTES` from 2 KiB to 32 KiB.
- [x] Increment `PROTOCOL_VERSION` and update JS contract expectations.
- [x] Preserve decoded Rust command/event shapes so transfer assembly code still works with byte vectors.
- [x] Add protocol tests proving:
  - binary representative payload round-trip;
  - base64 wire round-trip;
  - serialized bulk bytes are a JSON string;
  - a maximal chunk envelope fits below `COMMAND_MAX_BYTES`;
  - malformed base64 is rejected.

**Verification**

- `cargo nextest run -p shoop_audio_protocol --profile ci`
- Native and Wasm checks for `shoop_audio_protocol`.
- Inspect representative serialized envelopes for size and shape.

**Milestone:** Commit the codec, protocol version, and bulk wire format together.

---

### Stage 2 — Efficient session capture and replacement

Depends on Stage 1.

- [x] Replace `serde_json::{to_vec,from_slice}` for `BackendSessionData` with the shared binary codec in:
  - `RemoteWorkletBackend::replace_session_async`
  - `RemoteWorkletBackend::capture_session_async`
  - AudioWorklet session capture
  - AudioWorklet session replacement commit
- [x] Remove `session: BackendSessionData` from `SessionReplaceAssembly`.
- [x] Remove the full `replace.session != session` comparison.
- [x] Document the asynchronous backend contract: once started, subsequent calls poll the single active operation; the current argument is consumed only when starting and when applying successful completion.
- [x] Retain only generation, encoded bytes, offsets, commit/completion state, and progress in the assembly.
- [x] Preserve transfer cancellation on driver generation change, detach, explicit abort, command error, and stale generation.
- [x] Ensure progress remains bounded and meaningful under the larger chunks.
- [x] Update session and loop-content transfer tests for binary payloads and base64 envelopes.
- [x] Add a multi-track fixture containing finite nontrivial audio, MIDI, timing, and processor state; verify capture → transfer → replacement equality.
- [x] Add a polling regression test proving an active replacement emits no second begin command and performs no second encoding.
- [x] Verify large-session command count is derived from 32 KiB chunks rather than sample count or 2 KiB chunks.

**Verification**

- `cargo nextest run -p shoop_worklet_client -p shoop_audio_worklet --profile ci`
- `python3 scripts/run_wasm_tests.py --runtime node --profile dev --package shoop_worklet_client`
- `python3 scripts/run_wasm_tests.py --runtime node --profile dev --package shoop_audio_worklet`
- Warning-denying native and Wasm builds for the affected packages.

**Milestone:** Commit binary session transfer and removal of repeated sample traversal.

---

### Stage 3 — Bounded waveform pipelining

Depends on the protocol groundwork but can otherwise be developed independently of Stage 2.

- [x] Change `WAVEFORM_CHUNK_SAMPLES` to 4,096.
- [x] Add a waveform in-flight limit of eight.
- [x] Refactor `WaveformAssembly` to track separately:
  - next expected response;
  - next request offset/channel;
  - known channel count and current-channel sample count;
  - number of requests in flight;
  - completion state.
- [x] Initially request one chunk to learn channel metadata, then fill the bounded pipeline.
- [x] Never exceed either the waveform limit or the transport’s global pending-command threshold.
- [x] Continue relying on transport sequence ordering while validating returned channel, offset, revision, and totals.
- [x] Advance across channel boundaries without requesting beyond a channel’s declared total.
- [x] Preserve invalidation on loop mutation, replacement, grab, clear, driver restart, and revision change.
- [x] Keep audio unavailable to the app until the exact existing all-channel completion condition is met.
- [x] Extend tests to cover:
  - initial metadata request;
  - eight-request fill and refill;
  - short final chunks;
  - channel transitions;
  - multi-channel exact assembly;
  - timing metadata;
  - stale revisions;
  - empty channels;
  - cancellation and transport-capacity pressure.

**Verification**

- Targeted `shoop_worklet_client` and `shoop_audio_worklet` native tests.
- Node and Chromium Wasm package tests.
- A deterministic assertion that representative 242,526-frame, four-channel content requires approximately 240 requests rather than 1,896, while producing identical samples.

**Milestone:** Commit larger waveform chunks and bounded pipelining.

---

### Stage 4 — Trace-ring wraparound correctness

Independent of Stages 1–3 except for the protocol-version update in JS contracts.

- [x] Add a wrap-aware trace-drain operation to `raw_wasm_host.js` that:
  - drains complete groups into the existing contiguous Wasm transfer buffer;
  - copies the result into SAB tail and head segments;
  - performs no per-record allocation;
  - returns the complete record count.
- [x] Use the shared operation from both `audio_worklet.js` and `audio_worker.js`.
- [x] Publish the SAB write index only after both copies complete.
- [x] Retain the existing contiguous `traceDrainInto` API where needed by contracts.
- [x] Extend `raw_wasm_host_contract.mjs` with the exact failure boundary:
  - write position `capacity - 1`;
  - next group larger than one record;
  - successful split copy and continued draining after wrap.
- [x] Add a bounded repeated-wrap test proving the producer does not become permanently stuck and reports no source drops when total consumer capacity is sufficient.

**Verification**

- Build the production Worklet artifact.
- Run `raw_wasm_host_contract.mjs`.
- Run Node and Chromium tracing tests.
- Capture a short detailed trace and verify engine records continue across multiple 8,192-record boundaries with zero source drops.

**Milestone:** Commit the trace wraparound fix and regression coverage.

---

### Stage 5 — End-to-end validation

Depends on all previous stages.

- [x] Load and round-trip a representative mixed audio/MIDI session through the browser backend.
- [x] Verify the replacement becomes visible only after successful commit and old state survives injected failure/cancellation.
- [x] Select both one-cycle and two-cycle multi-channel loops and verify exact waveform completion without long frame-gated waits.
- [x] Capture a coarse browser Perfetto trace and verify:
  - session-load polling no longer produces the repeated 17–22 ms frontend-update plateau;
  - no second session serialization occurs;
  - normal UI updates remain responsive during transfer.
- [x] Capture a short detailed trace to verify the wrap fix; do not use a long detail capture as a performance baseline because retention limits and tracing overhead remain relevant.
- [x] Run repository gates:
  - `cargo fmt --all -- --check`
  - `RUSTFLAGS="-D warnings" cargo build --workspace`
  - `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`
  - `python3 scripts/check_shoop_test_usage.py`
  - `python3 scripts/check_tracing_coverage.py --require-closed`
  - `python3 scripts/run_wasm_tests.py --runtime node --profile dev`
  - `python3 scripts/run_wasm_tests.py --runtime chrome --profile dev`
  - `cargo check -p shoopdaloop --no-default-features --target wasm32-unknown-unknown`
  - `cargo build -p shoop_audio_worklet --target wasm32-unknown-unknown --release`
- [x] When browser dependencies are available, build/package and run the hosted and self-contained Chromium smokes documented in `src/rust/shoopdaloop/README.md`.

**Milestone:** Commit any final test/validation adjustments separately from behavior changes.


## Validation results

Completed on 2026-08-31. Native and Wasm protocol/session/waveform tests cover mixed audio/MIDI round trips, atomic replacement, cancellation, exact multi-channel waveform assembly, the 240-request representative waveform bound, and the absence of repeated replacement encoding. The raw-host artifact contract exercises a complete trace group across the ring boundary and verifies continued draining. Repository formatting, warning-denying builds, policy checks, native tests, Node Wasm tests, and pinned Chromium Wasm tests passed. The full native suite encountered three resource-contention failures during its parallel run; all three passed when rerun in isolation with the same feature set.

## Out of scope / possible further improvements

- Transferable `ArrayBuffer` bulk commands and a dedicated JS/Wasm binary ABI.
- Converting the complete control protocol from JSON to binary.
- Backend-generated min/max waveform summaries or multiresolution waveform pyramids.
- Computing waveform summaries directly from newly decoded `.shoop` media.
- Fetching only zoomed waveform ranges.
- Progressive publication of partially loaded waveform channels.
- `Arc<[f32]>` or ownership-transfer changes to eliminate additional application/backend sample copies.
- Streaming per-channel session staging instead of assembling one complete binary session.
- Fire-and-forget or batch acknowledgements for ordered bulk chunks.
- Moving archive decoding, resampling, or session preparation into a Worker.
- Progressive/non-atomic session replacement.
- Dedicated loop-audio export that avoids capturing the complete backend session.
- Trace-retention policy changes or expanded separation of source, SAB, and retention health counters beyond the wraparound fix.

## Execution contract

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
