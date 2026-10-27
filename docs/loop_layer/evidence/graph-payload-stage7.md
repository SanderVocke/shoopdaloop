# Stage 7 prepared graph descriptor payload foundation

## Source and scope

Source is the commit containing this report, based on
`e364d80a7359b471bcac1bf9efc7f882445ac861`. Final host reports identify that base
and `source_dirty: true`: testing preceded the containing commit. Production
runtime/loop Rust and dependency manifests/lockfiles did not change.

This is **partial Stage7**, not another completed checklist row. `LGP1` is a payload
codec/prepared materializer. It is not connected to the primitive `LLC1`/`LLR1`
command envelopes, reliable response credits, bounded two-thread payload transport
or MessagePort graph commands. The new graph route-cut scenario runs natively;
Node/Chromium check the payload receiver and local owner exchanges, not graph
activation through an AudioWorklet. Actual worklet runs below remain fresh
regressions of the existing primitive scenario.

Stages0–6 stay verified; Stage7 stays2/8 rows verified. Graph command/result
transport, program/resource payloads, immutable exports, broader host scenarios,
CI artifacts and Stages8–10 remain open. No new Miri, benchmark, hosted application
smoke, remote CI or review result is claimed. Applicable fixture interpreter cases
are pending consolidated Stage7 registration/execution in [MIRI_JOBS](../MIRI_JOBS.md).

## Implemented interfaces

- Public fixture module `graph_transfer`: `Record`, `Payload`, `Counts`, receiver-local
  `Id`, `Receiver`, `Phase` and local owning `Loan`.
- Canonical96-byte `LGP1` records: Begin, Step, Edge, Planning, Binding, Seal, Abort.
  Framing and complete field offsets are in the [wire guide](../../../tests/loop_engine_host/WIRE.md#prepared-graph-descriptor-payload-lgp1).
- Two pre-touched full-capacity `PreparedPlan` banks. Prepared aggregate/owner byte
  bounds, actual vector capacities and configuration limits remain distinct.
- Strict nonzero session/receiver-wide u64 sequence and checked local u32 generations.
  Rejected records preserve all banks and the watermark. No wrapping identity reuse.
- One record per call, with exact next-index append and declared-count completion.
  Planner metadata/bindings remain distinct from signal connections. Seal does not
  validate a live catalog or authorize execution.
- Local owning take/recycle, with invalid returns retaining `(error, plan)` for
  off-render quarantine. Cancelled/rejected candidates and retired graphs can
  recycle without allocation/destruction on the guarded path. A smaller retired
  plan truthfully reduces the next advertised capacities; Begin does not grow it.
- Host-only `BrowserGraphReceiver` exports the same prepared materializer through
  local pointers and local loan slots. It does not submit a runtime graph.

The existing primitive facade preparation still uses the same graph. Its route
builder was factored into `scenario::routes`; no production API changed. Graph
staging is opt-in only for the new native test host, not silently enabled in the
existing primitive browser fixture.

## New native tests

Seven test functions under `graph_transfer::tests` pass in debug and release:

1. `canonical_records_preserve_full_width_fields_and_reject_all_padding_and_truncations`
   checks an independent complete byte table for all seven record variants,
   full-width fields, all672 truncated inputs, seven oversized records and every
   reserved/padding byte. Invalid host endpoints, kinds and generations are refused.
2. `receiving_is_transactional_ordered_and_independent_of_other_banks` checks
   incomplete/out-of-order batches, duplicate planning, independent bank use,
   seal/abort and generation changes.
3. `busy_stale_foreign_and_malformed_records_do_not_advance_or_destroy_owners`
   checks capacity, phase, identity, session and sequence preservation; wrong-config,
   oversized and wrong-generation returns retain their owning inputs.
4. `generation_and_sequence_exhaustion_never_wrap_and_recycled_capacities_are_truthful`
   exercises actual maximum local generation/sequence state and a returned smaller
   descriptor. Exhausted banks retire at0; oversized subsequent declarations fail
   without mutation or reallocation.
5. `receiver_descriptors_stage_activate_and_recycle_real_facade_graph_owners`
   materializes the public compiler's planned descriptor into prepared receiver
   storage, stages/activates it through the real facade, checks independent musical
   output, and recycles the returned old graph with its actual smaller capacities.
6. `cancelled_stale_and_invalid_runtime_candidates_return_the_exact_prepared_owner`
   runs128 unchanged candidate cycles:32 cancellations,32 stale-epoch refusals,
   32 missing-planner validation refusals and32 successful stage/release cycles.
   Returned descriptors and retained storage agree; the receiver generation reaches129.
7. `ingress_refusal_stalled_returns_and_disconnect_keep_all_graph_owners_explicit`
   fills all eight ingress slots and recycles the Full-refused candidate; holds
   both graph owners and all returns unread while the full musical script proceeds;
   then releases/recycles both graphs. Separate queued/Ready disconnect cases
   dispose owners off-render without fabricating returns or terminal results.

Codec/materialization, local owner exchanges, actual engine callbacks, prepared
submissions and return/recycle operations are heap-guarded. Test reports/expected
vectors and explicit stopped destruction remain outside those guards. The existing
native allocator canaries remain active in both profiles.

### Real native route-cut oracle

The first callback renders one sample after all initial commands are staged. Its
owning completion supplies the Ready graph ID. Activation is then admitted for
frame4, before its deadline; Ready is not treated as active.

The graph routes host output directly from the playback role at4, while capture
continues to receive half-gain FX. The independent13-frame left output is:

```text
1,2,3,4,1,2,3,4,1,2,3,4,0
```

Right is ten times left. All134 MIDI packets still match the original independent
packet oracle. Record/Play/Stop receipts are requested/delivered/terminal at0/4/12;
the graph receipt is requested/delivered/terminal at4 and publishes graph epoch2,
with topology epoch0. Semantic bytes match the existing independent primitive
boundary table; that projection deliberately does not encode graph epochs/routes.

After the common one-frame staging acknowledgement, partitions `[3,8,1]`, twelve
single samples and `[0,3,0,5,3,1,0]` agree. The third crosses the frame8 wrap inside
a callback. Releasing the activated stage returns the old initial graph, which is
compared with its public compiled descriptor/planning metadata and reused. Explicit
stopped cleanup recovers256 pages,16 versions,16 snapshots and zero media references.

The graph-result stall case separately compares every original stereo sample,
MIDI packet and exact transition receipt before collecting/releasing both staged
graphs. Disconnect tests are intentional abandonment, not successful credit return.

## Public-binding Wasm checks

Two new shared check groups run in each Node/Chromium profile, bringing the named
binding inventory to eleven:

- Canonical records, full64-bit session/sequence/epoch, malformed reserved fields,
  foreign/stale/duplicate records, ordered phases and local owning refusal.
  All six u64 index/count locations reject `2^32` with Capacity on Wasm32 rather
  than truncating: Step input-end, four Planning indices/counts, Binding decision slot.
- 128 alternating-bank local owner exchanges, duplicate take/return refusal,
  generation advancement and fixed prepared memory.

These are public-binding checks, not the31 native Rust tests executed as package-
Wasm test functions. They exercise a payload-only receiver with four node/binding
slots and sixteen edges; they do not establish runtime graph compatibility or
actual worklet graph-command transport.

## Fresh complete gates and browser observations

| Gate | Result |
| --- | --- |
| Fixture native debug/release | 31 passed per profile; zero skips; zero doctests |
| README exact-filter route-cut commands | One test executes/passes in each profile |
| Native direct/wire/two-thread replay | All six profile/mode combinations, four partitions each |
| Public-binding checks | Eleven in each Node/Chromium profile |
| Actual primitive AudioWorklets | Offline/live pass in both profiles |
| Real processor/heap worklet canaries | Each exits1 with exact Rust-origin/result/heap witnesses in both profiles |
| Python decoder/failure-gate tests | Five pass |
| Workspace nextest | 2141 passed/four existing skips;33.020s summary |
| Warning-denied builds/rustdoc, formatting and applicable policies | Pass |

Final ordinary memory sizes remain fixed in both instances:

| Profile | Main/worklet prepared = final bytes | Offline callbacks/engine frames | Live callbacks/engine frames |
| --- | ---: | ---: | ---: |
| Debug | 1,835,008 | 2 / 256 | 4 / 512 |
| Release | 1,769,472 | 2 / 256 | 8 / 1024 |

Offline first engine browser frame is128; these final live runs start at0. Live
startup/callback counts remain schedule-dependent, not a fixed deadline assertion.
Full actual downstream Web Audio output, the512-frame offline context output,
MIDI/receipts, independent memories, returned buffers, zero successful heap counts
and stopped256/16/16/0 recovery all pass. The complete2440-byte semantic stream
still hashes to `1c37c5d4f3607432c147852004a985b6ad2f6f3d1cf369287680ed9e9d5899dd`.

Failure witnesses remain processor origin1/code10/heap`[0,0,0]` and heap origin2/
code`0xffff0001`/heap`[4,20,4]`. Failed sessions are not successful disposal evidence.
No guard, workload, stack limit or watchdog was weakened. Warm summary durations
are not cold-cache, deadline, performance or CI-margin measurements.

Final Wasm SHA-256:

- Debug: `b25c18769a79f6b8930549ba036ac8793e94c43bbd557c70961702a78124e1b7`
- Release: `8189d5dccc0ccb7bf10d5737c5aa8e12044f24f505637ba754bd61fb78a58037`

## Commands, tools and retained failure

Commands run in `nix develop --command ...`, with `RUSTFLAGS="-D warnings"` and
`RUSTDOCFLAGS="-D warnings"`. Tools: rustc1.97.1, cargo1.97.1, prescribed nextest
0.9.116, wasm-pack0.15.0, Node22.22.2, Chromium/ChromeDriver147.0.7727.137.
Provisioned Node/browser/driver and nextest directories were on PATH. Host-only
wasm-bindgen remains0.2.127 and rtrb remains0.4.0.

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

The initial native build failed with `E0599`/`E0616`: the new test helper assumed
nonexistent `Stopped::reclaim`/`media` methods and direct public resource fields.
It now uses the existing resources callback/reclaim and usage APIs. This was a
fixture-helper API error, not a production bug; assertions/workloads were not
reduced. `initial-native.log` retains the failed build, followed by passing focused,
expanded and final runs.

[`graph-payload-stage7-logs.tar.gz`](graph-payload-stage7-logs.tar.gz) preserves those
logs, final native/workspace/docs checks and complete final browser/Node/native/
canary artifacts. SHA-256:
`26f7afec8474ae4c6b14e3d78f01599f7c22b0e2cb4c8df50660b7ddf0d14829`.
Local originals: `target/loop-layer-graph-payload-stage7/` and
`target/loop-engine-host/reports/{debug,release}/`.
