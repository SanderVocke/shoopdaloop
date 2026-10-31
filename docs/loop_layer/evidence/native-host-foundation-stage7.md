# Stage 7 native loop-host foundation

## Scope and source

This is a **partial Stage 7 milestone**, not independent AudioWorklet acceptance.
Source: the commit containing this report and `tests/loop_engine_host/`, on base
`46564433864f4ceeb62b26c3fbf47d03c332c98f`. No production runtime/loop Rust source
changed. The separate workspace has its own lockfile; `rtrb` is pinned to 0.4.0,
matching the repository and existing generic fixture rather than the initially
resolved 0.4.1.

The actual `Engine<Mix>` records stereo/MIDI during `[0,4)`, immediately plays
that take twice through half-gain FX during `[4,12)`, and stops at 12. Independent
scalar/audio and explicit MIDI expectations cover every output, not only a hash:
13 stereo frames, 134 packets and three exact requested/delivered/terminal receipts.
Whole, single-frame, irregular and zero-interspersed callback partitions agree.
Every engine callback is heap-guarded. Stopped cleanup/reclamation returns all
256 pages, 16 versions, 16 snapshots and zero media references.

The standalone native harness allocates preparation, submissions and report
storage outside callbacks. It does not yet supply prepared cross-instance ingress,
callback-safe owning completion recycling, a wire codec or semantic journal.
No immutable export-content check, native two-thread, live/offline AudioWorklet,
browser memory-growth measurement, benchmark, CI or new Miri pass is claimed.
Applicable new interpreter coverage will be considered with the consolidated
Stage 7 selection, not a routine Miri invocation for this smaller milestone.

## Fresh verification

All commands ran in `nix develop --command ...` on this Linux host. Rust builds,
tests and replay use `RUSTFLAGS="-D warnings"`; documentation uses
`RUSTDOCFLAGS="-D warnings"`. Tool versions:

- rustc 1.97.1 (`8bab26f4f 2026-07-14`)
- cargo 1.97.1 (`c980f4866 2026-06-30`)
- cargo-nextest 0.9.116 (`2416f2fb1 2025-12-26`), prescribed cached executable

| Command | Result |
| --- | --- |
| `cargo fmt --all -- --check` | Pass |
| `cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check` | Pass |
| `cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml` | 5 passed, no skips; no doctests discovered |
| Same test command with `--release` | 5 passed, no skips |
| `cargo run --locked --manifest-path tests/loop_engine_host/Cargo.toml --bin replay` | Four partitions agree on all samples/packets/receipts and pool recovery |
| Same replay command with `--release` | Same full result |
| `cargo build --locked --manifest-path tests/loop_engine_host/Cargo.toml --lib --target wasm32-unknown-unknown` | Pass; compilation only, not Wasm execution |
| `cargo build --locked --workspace` | Pass |
| `cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps` | Pass |
| `cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps` | Pass |
| `python3 scripts/check_shoop_test_usage.py` | Pass |
| `python3 scripts/check_rt_runtime_boundary.py` | Pass for all four audited targets |
| `python3 scripts/check_loop_engine_boundary.py` | Pass for all four audited targets |
| `python3 scripts/check_tracing_coverage.py --require-closed` | Pass, 207 modules |
| `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace --features shoop_engine/app_backend --profile ci` | 2141 passed, four existing ignored failure canaries; summary elapsed 34.014s |

The five independent-fixture tests are additional to the root workspace totals:
full scenario/partition/recovery comparison, positive allocator instrumentation,
and three expected-panic tests for allocation, reallocation and deallocation.
Each negative guard test requires `heap operation during rendering` in both
profiles. No larger stack, reduced workload, disabled guard or timing threshold
was used. Warm native elapsed time is not a device deadline or speedup claim.

Cargo-nextest discovery identifies the four existing ignored tests as:

- `shoop_common::nextest_capture_smoke::intentional_failure_publishes_trace`
- `shoop_wasm_test_support::tests::native_result_failure_canary`
- `shoop_wasm_test_support::tests::shared_failure_canary_is_ignored_by_default`
- `shoop_wasm_test_support::tests::shared_result_failure_canary_is_ignored_by_default`

## Retained failed attempts

1. Initial fixture compilation referenced the private
   `rt_runtime::executor::Outcome` instead of public `transport::Outcome`, producing
   E0603. Corrected the fixture import/path; no production code changed.
2. The initial independent MIDI expectation included only six original/replayed
   note packets and omitted default-state restoration at frames 4 and 8. The
   assertion failed on 128 additional packets. Corrected the explicit oracle from
   the documented restoration semantics and default projection: neutral pitch,
   CC64/69 released, sostenuto off, on all 16 channels before original events.
   The oracle does not call the production restoration helper or derive expected
   packets from actual output. Full packet comparison remains mandatory.

Raw logs and discovery are preserved in
[`native-host-foundation-stage7-logs.tar.gz`](native-host-foundation-stage7-logs.tar.gz)
(SHA-256 `78a8a94b8b4fa15f6963304c8b2bd045616c96ec2acfa945616076c027bea071`).
Original local output: `target/loop-layer-host-foundation-stage7/`.
