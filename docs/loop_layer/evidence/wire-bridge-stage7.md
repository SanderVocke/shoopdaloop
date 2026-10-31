# Stage 7 primitive wire bridge and two-thread replay

## Source and scope

Source is the commit containing this report and the corresponding
`tests/loop_engine_host` changes, based on
`3c5b70b37ba24d695e7595496c103177bea013e1`. No production runtime/loop Rust source
or dependency version changed. This is another **partial Stage 7 milestone**.

The bridge supports absolute-frame primitive Record/Play/Stop/Seek, not complete
host graph/program/resource staging. It adds:

- Fixed little-endian 64-byte `LLC1` requests and 96-byte `LLR1` responses, with
  explicit version, session, sequence, wire slot, logical generation and full-width
  frame/position fields. Reserved/unused fields are canonical zero.
- Four normal wire credits and one independent Stop recovery credit. Admission and
  terminal domain results remain distinct. Client and renderer retain each slot
  until rejection/cancellation or terminal response consumption.
- Guarded prepared submission, actual engine callbacks, owning completion
  collection and response recycling. Only empty-payload transition commands are
  currently constructed; unexpected owning returns are quarantined for off-render
  disposal. Fixed external collection bounds do not cap musical intervals.
- Strict sequence checks, no watermark advancement on Busy/malformed/foreign
  requests, no wrap at `u64::MAX`, and client credit preservation on invalid replies.
- Direct, wire and native two-thread replay. The threaded driver uses capacity-one
  request/response channels; channel waits and join are outside callbacks. This is
  offline execution, not a live-device scheduling or hard-deadline claim.

All three drivers compare four callback partitions against the independently
checked native oracle: all13 stereo frames,134 MIDI packets and three exact-frame
transition receipts agree; all256 pages/16 versions/16 snapshots are recovered,
with zero media references.

## Fresh verification

Commands ran in `nix develop --command ...`, with `RUSTFLAGS="-D warnings"` for
Rust builds/tests/replay and `RUSTDOCFLAGS="-D warnings"` for documentation.
Toolchain: rustc1.97.1 (`8bab26f4f`), cargo1.97.1 (`c980f4866`), prescribed
cargo-nextest0.9.116 (`2416f2fb1`). Full version output is retained in the raw logs.

- Workspace and independent-fixture formatting pass.
- `cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml` and the
  same command with `--release`: **16 passed per profile**, no skips. The fixture
  currently has zero doctests; the unit tests and executable drivers do real work.
- `cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay
  -- <mode>` for each of `--direct`, `--wire`, `--two-thread`, in debug and release:
  **six passing driver/profile combinations**, each comparing all four partitions.
- `cargo build --locked --manifest-path tests/loop_engine_host/Cargo.toml --lib
  --target wasm32-unknown-unknown`, debug and release: pass, **compilation only**.
- `cargo build --locked --workspace`: pass.
- `cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps` and independent
  fixture rustdoc with `--manifest-path tests/loop_engine_host/Cargo.toml --no-deps`:
  pass with warnings denied.
- Test-attribute policy, runtime/loop production dependency boundaries and closed
  tracing coverage checks pass. The inventory remains207 modules.
- `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features
  shoop_engine/app_backend --profile ci`: **2141 passed/four existing ignored
  failure canaries**, summary elapsed33.579s. Fixture tests are additional to this
  root-workspace total. This warm timing is not a measured speedup or deadline.

## Additional exercised cases

- Exact independent request bytes and1024 full-width frame/position round trips,
  seed `0x73ce98b4a01365d2`; all request/response truncation lengths, reserved
  response bytes, invalid versions/kinds/status/error combinations and oversize
  response rejection. Error-origin ranges round-trip independently.
- 512 requests over128 slot-reuse rounds:64 transport cancellations,128 applied
  outcomes and320 superseded outcomes. Old cancellation identities and stale
  responses cannot affect replacement requests. Every round recovers wire credits.
- Full normal wire slots with unread results while automatic playback continues;
  an independent recovery Stop still completes. A Busy normal request does not
  consume its sequence, and unread recovery results prevent another recovery.
- Cancellation before dequeue, stale target generation, empty-content domain
  refusal, late delivery, future recovery refusal, pending-future disconnect and
  repeated stopped cleanup.
- Permanent bridge fault after invalid callback frame, rejection of further
  ingress/rendering, no fabricated responses and complete off-render recovery.
- Three retained expected-panic heap canaries require the exact diagnostic
  `heap operation during rendering` in both native profiles.

## Failure retained and corrected

The first integrated bridge run passed11 tests and failed the stale-target/empty
content case with `InvalidState`. The bridge incorrectly assumed every successful
runtime delivery contained `Reply::Transition`; an early domain validation failure
instead returns `Reply::Failed`. The bridge now preserves that result explicitly,
including whether its reason is timeline or runtime-resource failure. The initial
stale-target test also incorrectly expected runtime delivery error6; the actual
contract is successful delivery with timeline `InvalidId` (wire259). Full tests now
assert that distinction, empty-content wire268, and future-recovery admission
rejection7. No production behavior, guard or workload was weakened.

Raw logs, including this failed attempt and intermediate narrower successful runs,
are in [`wire-bridge-stage7-logs.tar.gz`](wire-bridge-stage7-logs.tar.gz), SHA-256
`642d57396ce97c4762664284c56c4aab38115e46f2c25a8adc0901c0a4145225`.
The `final-*` logs identify the final16-test source. Local originals are under
`target/loop-layer-wire-bridge-stage7/`.

## Still outstanding

There are no browser bindings or actual Wasm/AudioWorklet runs yet. Native copied
bytes do not prove independent Wasm memories, fixed browser memory or actual Web
Audio output. Graph/program/resource payload staging, semantic journals, immutable
exports, broader composite/graph/exhaustion scenarios, browser failure canaries and
the unified host runner remain Stage7 requirements. No new benchmark, CI or Miri
pass is claimed; applicable new interpreter work belongs to the consolidated
Stage7 selection, not a per-commit routine invocation.
