# Stage 7 native graph command and owning-return integration

## Source and evidence boundary

Source is the commit containing this report, based on
`ca9d142ed9b3452f0779ce265117ed96044b4424`. Browser reports identify that base
and `source_dirty: true`, not a claim that testing followed the containing commit.
No production runtime/loop Rust, manifests or lockfiles changed.

`Bridge::with_graphs` now connects prepared descriptors to serialized native
Stage/Activate/Release commands, bounded results and owning-return recycling.
**This path is native verified and Wasm compiled, not yet exposed through new
Wasm bindings or MessagePort.** Actual browser runs below are fresh regressions
of the primitive scenario. The eleven binding groups still exercise primitives,
semantic observation and payload-only `BrowserGraphReceiver`, not the graph commands.

Stage7 remains2/8 checklist rows verified. New graph command bindings, two-thread
payload transport, actual worklet graph changes, structured graph-host reports,
program/resource payloads, exports and broader hosts remain open, as do Stages8–10.
No Miri, benchmark, hosted application smoke, remote CI or review pass is claimed.
The eight new native cases are pending consolidated Stage7 interpreter registration/
execution, alongside the preceding seven payload cases; see [MIRI_JOBS](../MIRI_JOBS.md).

## Implemented interfaces and ownership

- `graph_wire::{Operation, Request, Response, Client, Status, Bank}`: canonical
  96-byte `LLG1` requests and128-byte `LLS1` results. Complete byte layouts and
  lifecycle steps are in [WIRE.md](../../../tests/loop_engine_host/WIRE.md#native-graph-commandsresults-llg1--lls1).
- `Bridge::with_graphs`, `graph_payload`, `graph_ingest`, `next_graph_response`,
  `graph_cancel`, `retained_graphs`; `outstanding` now includes both protocols.
- Exactly two graph command/result credits, two prepared descriptor banks and two
  runtime graph candidates. Default primitive bridges still have no graph owners.
- Separate full-width monotonic payload/graph-command/primitive sequences and
  checked credit, bank-generation and scoped runtime graph identities.
- Stage/Release have one owning completion. Activate has separate admission and
  execution; admission is not publication. Frames/epochs/errors are not flattened.
- Stage takes only a Sealed bank with matching Begin/envelope epoch. Runtime
  validation separately checks the live catalog/epoch. Local refusal, cancellation
  and runtime rejection retain/recycle their candidate and report the next bank
  generation. Stage success reports Ready while retaining the bank loan.
- Release matches the runtime ID to its original bank before moving a returned
  candidate/retired graph into that bank. Actual capacities may shrink after the
  first activation returns the smaller initial graph; no callback resize occurs.
- The graph client keeps a fixed two-entry graph/bank mirror independent of command
  credits. Malformed/foreign/stale/duplicate/out-of-order replies, wrong returned
  banks/generations and incorrect publication epochs leave its state unchanged.
- One seven-credit collection bound per result stream per nonempty callback:
  five primitive plus two graph, never renewed per musical interval. Internal
  progression does not consume those external credits. Zero calls are inert.
- Unexpected owning completions return intact to the outer prepared quarantine;
  the outer bridge faults rather than dropping owners on the callback. A separate
  prepared slot retains an unexpected local owning refusal. Stopped extraction
  and destruction remain off-render, without fabricated returns or replay.

The primitive `LLC1`/`LLR1` formats and replay CLI are unchanged. The new graph
commands are currently exercised by native test hosts, not a new graph CLI mode.
No owner, pointer or Rust enum layout is serialized.

## Eight new native cases

Six functions under `graph_wire::tests`:

1. `graph_command_codec_preserves_full_width_fields_and_canonical_padding` compares
   independent complete request/result byte tables, full64-bit values, scoped IDs,
   all288 truncated requests/384 truncated result inputs, reserved fields and
   contradictory presence/phase metadata.
2. `graph_client_credits_validate_identity_phase_and_last_sequence_without_wrapping`
   checks two bounded credits, invalid replies, retained graph/bank mapping, maximum
   bank-generation return, wrong returned bank/generation, last sequence exhaustion,
   admission-before-execution, runtime sequence, target ID and publication epochs.
3. `actual_bridge_graph_staging_activation_and_retirement_match_all_media_and_receipts`
   runs real serialized Stage/Activate/Release with the actual facade across three
   partitions, complete independent audio/MIDI/semantic expectations, exact receipts,
   retired initial-plan capacities, another stage/release reuse and full stopped recovery.
4. `stalled_graph_credits_preserve_music_and_independent_primitive_recovery` holds all
   four normal primitive and both graph credits/results unread. Music still matches
   every original sample/packet. A second same-frame Stop fills the fourth normal
   credit without changing music; its winning execution remains exactly at12.
   Independent recovery Stop at13 succeeds, all ten primitive records and both graph
   records are consumed, both graphs release/recycle, and ownership fully recovers.
5. `graph_candidate_returns_and_domain_failure_origins_survive_128_wire_cycles`
   runs32 cancellations,32 stale-epoch refusals,32 missing-planner validation refusals
   and32 stage/release cycles. Every candidate returns and the bank reaches generation129.
   A released graph's activation is runtime-admitted then domain Failed518 (runtime
   InvalidId after delivery); releasing that stale ID is admission Rejected6 instead.
6. `malformed_graph_requests_preserve_receiver_state_and_queued_ready_owners_drop_off_render`
   checks foreign sessions, Begin/envelope epoch mismatch, duplicate commands,
   refused Abort of a loaned owner, zero callbacks and queued/Ready disconnect.
   Pending credits are abandoned explicitly, not fabricated as successful returns.

Two functions under `graph_bridge::tests`:

7. `local_transport_refusal_returns_the_candidate_and_next_bank_generation_without_rendering`
   fills eight real control ingress slots, invokes the prepared graph handler and
   validates local Rejected8/zero runtime sequence/next generation2/full capacities,
   while engine time stays0. The intact candidate is back in its original bank.
8. `unexpected_owning_completion_is_returned_intact_without_recycling_or_fabricating_a_reply`
   obtains an actual owning cancelled completion, deliberately corrupts its expected
   epoch, then verifies the component returns that completion unchanged under the heap
   guard: same plan address/storage, bank still Loaned, no reply/credit fabricated.
   This directly tests the component quarantine handoff, not an injected end-to-end
   worklet graph fault. The outer bridge's quarantine/fault branch is implementation,
   not a claim that a new public worklet graph-failure canary was executed.

All new ingestion, materialization, prepared submission, callback collection and
recycling paths are guarded. Test reports/expected vectors and stopped disposal
allocate/destroy only off-render. Existing native heap-positive/negative controls
remain active in both profiles. Payload helper reuse changes only native test
visibility; the receiver additionally exposes its expected epoch for preflight.

## Native musical/reference result

After a common one-frame Stage acknowledgement, the graph activates at4 and routes
host playback directly, while capture still receives the half-gain FX path. Left:

```text
1,2,3,4,1,2,3,4,1,2,3,4,0
```

Right is ten times left. All134 MIDI packets match the existing independent table.
Record/Play/Stop requested/delivered/terminal frames are0/4/12. Activation has
requested/effective/delivered/terminal frame4 and publishes graph epoch2, topology0.
The full semantic journal remains byte-identical to the independent primitive table;
that timeline projection deliberately does not encode graph routing/epochs.

Partitions after acknowledgement are `[3,8,1]`, twelve single samples and
`[0,3,0,5,3,1,0]`. Returned initial-plan capacities are3 steps/12 edges/2 bindings;
that smaller owner stages/releases again without growth. Successful command/graph
counts reach zero and stopped recovery is256 pages/16 versions/16 snapshots/zero
media references. Queued/Ready abandonment is tested separately from successful return.

## Fresh gates and primitive browser regressions

| Gate | Result |
| --- | --- |
| Fixture native debug/release | 39 passed per profile; zero skips; zero doctests |
| README exact-filter graph route-cut commands | One test executes/passes per profile |
| Native primitive direct/wire/two-thread replay | Six mode/profile combinations; four partitions each |
| Node/Chromium existing binding groups | Eleven per runtime/profile; no new graph command binding execution |
| Actual primitive AudioWorklets | Offline/live pass in both profiles |
| Actual worklet processor/heap canaries | Each exits1 with exact diagnostic and Rust-origin/result/heap witnesses per profile |
| Python decoder/failure-gate tests | Five pass |
| Workspace nextest | 2141 passed/four existing skips;33.029s summary |
| Warning-denied builds/rustdoc, formatting and applicable policies | Pass |

The final native/profile rerun includes the strengthened four-normal/two-graph
saturation assertions and the runnable guide commands. Fresh final browser reports
were rebuilt and rerun afterwards; their scenario is still primitive, not graph replay.

| Profile | Main/worklet prepared = final bytes | Offline callbacks/frames | Live callbacks/frames |
| --- | ---: | ---: | ---: |
| Debug | 1,835,008 | 2 / 256 | 4 / 512 |
| Release | 1,769,472 | 2 / 256 | 8 / 1024 |

Offline first engine browser frame128; final live runs start at0. Callback counts
remain schedule-dependent, not a deadline assumption. Full actual downstream Web
Audio and512-frame offline output, MIDI/receipts, independent ordinary memories,
returned buffers, zero normal heap counts and stopped256/16/16/0 recovery all pass.
The2440-byte semantic stream still hashes to
`1c37c5d4f3607432c147852004a985b6ad2f6f3d1cf369287680ed9e9d5899dd`.
Failure witnesses remain origin1/code10/heap`[0,0,0]` and origin2/code`0xffff0001`/
heap`[4,20,4]`. Failed sessions are not successful disposal evidence.

Final Wasm SHA-256:

- Debug: `bac00408ab926a3086c196cef507cb951a0edebf3961386a24da6a85d7f58c81`
- Release: `05ffb5057a2b6ec4505df693b0ce162edef6f0a841cbaa297dc19e69e056408e`

## Reproduce and inspect

All Cargo/Python commands ran inside `nix develop --command ...`, with
`RUSTFLAGS="-D warnings"` and warning-denied rustdoc. Tools: rustc1.97.1,
cargo1.97.1, prescribed nextest0.9.116, wasm-pack0.15.0, Node22.22.2,
Chromium/ChromeDriver147.0.7727.137. Host-only wasm-bindgen0.2.127/rtrb0.4.0 unchanged.

```sh
cargo fmt --all
cargo fmt --all -- --check
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml
cargo fmt --manifest-path tests/loop_engine_host/Cargo.toml -- --check
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml
cargo test --locked --manifest-path tests/loop_engine_host/Cargo.toml --release
python3 scripts/run_loop_engine_hosts.py --profile debug --verify-failures
python3 scripts/run_loop_engine_hosts.py --profile release --verify-failures
cargo build --locked --workspace
cargo doc --locked -p rt_runtime -p shoop_loop_engine --no-deps
cargo doc --locked --manifest-path tests/loop_engine_host/Cargo.toml --no-deps
python3 -m unittest discover -s scripts/tests -p test_loop_engine_hosts.py
python3 scripts/check_shoop_test_usage.py
python3 scripts/check_rt_runtime_boundary.py
python3 scripts/check_loop_engine_boundary.py
python3 scripts/check_tracing_coverage.py --require-closed
SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --locked --workspace \
  --features shoop_engine/app_backend --profile ci
```

No assertion, guard, workload, stack limit or watchdog was weakened. All captured
build/test commands pass; `initial-native.log` contains the earlier31-test discovery
before adding graph command tests, not the final39-test inventory. Timing summaries
are warm observations, not benchmark, deadline, cold-cache or CI-margin claims.

[`graph-wire-stage7-logs.tar.gz`](graph-wire-stage7-logs.tar.gz) retains initial/
focused/expanded/final native/workspace/docs logs and complete final browser/Node/
native/canary reports. SHA-256:
`7aa3417ca987312302919a8cea393cf57df65a5460460f9d9428d50d91b3583c`.
Local originals: `target/loop-layer-graph-wire-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
