# Output bus parity evidence

This covers P13 under the user-approved scope in
[the implementation plan](../../../PLAN_OUTPUT_BUSES.md): ordinary output buses,
including zero buses, within negotiated replacement-engine limits. Bus FX (P08)
remains unavailable. [PR #926](https://github.com/SanderVocke/shoopdaloop/pull/926)
targets `feat/backend-replacement`. The implementation branch is `feat/output-bus-parity`, based
on integration head `64a4ad0634de1664b1ff723b36c635cd243d819b`.

## Acceptance coverage

Paths below are relative to `src/rust/`. These tests exercise production code;
the four retained reference bus lifecycle/control/default-routing fixtures also
run in the complete workspace suite.

| Plan criterion | Evidence |
| --- | --- |
| 1. Ordinary buses and zero buses | `shoop_rt_engine/tests/structure.rs`: `ordinary_buses_sum_independently_and_last_removal_keeps_direct_audio` removes the initial bus, exercises fresh generational buses, removes all buses and retains direct audio. `shoop_app/tests/service.rs`: `buses_round_trip_order_routes_zero_and_reconcile_admitted_mutations_after_ui_loss` saves/loads zero buses, adds again, removes again and creates a fresh default with New Session. The browser canary repeats the file workflow. |
| 2. Honest capacity | `shoop_rt_backend/tests/adapter.rs`: `bus_pool_limits_reuse_and_fx_refusals_preserve_confirmed_state` fills all eight slots, refuses overflow/width/FX, removes/reuses capacity and preserves confirmed state. Engine tests cover maximum-width output, stale generations and repeated reuse. Host/settings/worklet/protocol tests verify the advertised pool and wire versions. |
| 3. Mixing and meters | Guarded engine structure/control tests verify explicit summing, independent controls, stereo balance restrictions, output peaks, ramps and unrelated paths. The multi-bus test produces exact direct output 1.0 and summed bus output 3.0, then 2.0 after independently muting one bus. Existing bus UI/reference control and meter fixtures remain enabled. |
| 4. Routing and defaults | The service round-trip creates a mono source feeding stereo Master/Cue and eight-channel Wide (12 explicit routes), skips a missing name and preserves routes after load. `default_bus_bootstrap_and_reset_from_zero_fan_out_to_a_mono_host` verifies both default channels reach the single host output. Existing named-default fixtures cover equal-width, incompatible and duplicate names. Persistence tests preserve missing external-port intent. |
| 5. Identity and presentation | The service round-trip persists visual order Wide/Master/Cue separately from logical Master/Cue/Wide order, checks fresh loaded identities and exact controls, and verifies confirmed removal cleanup. Client control snapshots enumerate by stable identity; engine tests reject removed generations. |
| 6. Confirmed operations and recovery | The service round-trip detaches during admitted creation, control and removal, checks pending ownership on a fresh attachment and reconciles confirmed outcomes without replay through typed and encoded paths. `refused_bus_structure_reconciles_ports_and_retains_a_removable_survivor` injects engine refusals. Existing `routing_and_removal_accepted_before_client_loss_settle_without_reinitialization` covers admitted mixer routes. `shoop_host/tests/recovery.rs`: `bus_lifecycle_uses_both_carriers_and_socket_recovery_does_not_recreate_master` covers native socket/shared-memory delivery and socket recovery during a bus control. |
| 7. Persistence and replacement | `shoop_persistence_codec/src/codec/tests.rs`: `session_files_preserve_ordinary_bus_shapes_order_and_an_empty_bus_set` encodes/decodes maximum-width, zero-bus and missing-port documents. Service typed/encoded round-trips check display/logical order, controls, fresh IDs, routes and zero buses. Existing atomic replacement, malformed/capability refusal, PCM/MIDI round-trip and accepted-load UI-loss tests run with the generalized bus image. The browser file canary records and restores media with multiple buses before the zero-bus workflow. |
| 8. FX exclusion and deployment | Adapter and service tests retain BusFx=false and reject forged FX creation without mutation; whole-document codecs reject FX-bearing buses. UI test `add_bus_dialog_uses_negotiated_channels_and_refuses_a_stale_fx_choice` clamps the negotiated channel maximum and clears unsupported FX. The native carrier test and actual hosted/single-file AudioWorklet canaries exercise deployment paths. Final platform CI and review are recorded below. |

## Limits and compatibility

The default prepares eight independent slots with eight channels each, including
the initial stereo default. Startup configuration can reduce the pool; active bus
widths are 1 through the negotiated maximum. Separate bus storage preserves track,
loop and media budgets. Route masks retain their existing 64-bit bound. No legacy
64-channel promise is made. All buses may be removed; neither load nor reconnect
synthesizes a missing Master. New Session explicitly creates a fresh default.

RT protocol version 3, host protocol version 2, application protocol version 12
and worklet configuration version 2 carry the changed records. Persisted `.shoop`
documents use the existing format and migrations. Old saved pool preferences
receive the new default bus limits.

## Validation record

Local Linux validation uses warning-denying builds, heap-guarded native engine
tests and production dummy-host processes. The complete nextest suite passed
2,543 tests with four optional/platform skips, including the admitted bus mutation
recovery expansion, multi-target Lua balance prevalidation and delayed slot
reclamation after confirmed bus removal. Runtime validation is recorded at
`884f1b90441f7e304cda3e4bf7c57c14e1f6ec7b`, including the independent-meter
assertions and confirmed-revision native recovery checks.
The refusal fixture queues a new bus while an old bus removal refuses and verifies
that both surviving buses remain usable. Formatting, test-attribute usage, parity inventory,
closed tracing inventory and all four target dependency-boundary checks pass;
the service and adapter also compile without their default features.

CI identified a native test ordering race: operation results can precede the
snapshot at their confirmed revision. Native test helpers now await that revision;
all ten recovery tests pass with CI tracing, and the bus socket/shared-memory
case passes twenty consecutive traced repetitions. The 33 MiB browser canary's
new zero-bus file stages use the same 120-second budget as its other large-media
stages; ordinary-stage deadlines remain unchanged.

The [selected Miri report](miri/output-buses/result.json) and its hashed log cover
the selected engine test
`ordinary_buses_sum_independently_and_last_removal_keeps_direct_audio`, which passed under
nightly-2026-05-04 Miri with seed 0 and default isolation and borrow checks. Its
source is `59812699`; subsequent production changes are in service/adapter admission,
while the native engine test adds independent-meter assertions. The recorded wall
time is 645 seconds under concurrent build/test load.
Its render/control heap guards remain active. This is focused ownership/generation
coverage, not a claim that the entire native test suite ran under Miri.

The actual hosted and single-file Chromium AudioWorklet canaries passed at 900×600
with fake microphone input and zero xruns when run without competing native test
loads. Both optimized forms also pass the 33 MiB WAV/66 MiB canonical-media
workflow, exact resave comparison and subsequent zero-bus save/load/add/New Session.
Built-worklet and codec-worker artifact contracts both pass. Local device tests do not claim
physical hardware coverage. Direct `file:`
navigation is blocked by this environment's browser policy; the single-file
canary therefore serves a directory containing only its embedded HTML.

The [complete runtime matrix](https://github.com/SanderVocke/shoopdaloop/actions/runs/41645542902)
is green on `884f1b90`: all eight Linux/macOS/Windows/web debug and release jobs,
coverage and bare-metal checks pass. Chromium hosted/direct-file and Firefox
checks pass in actual CI; the required Node Wasm suite passes. CodeQL,
documentation and both Codecov checks pass too. P13 is complete with the explicit
capacity qualification; the other fifteen rows, including P08, remain open. No automated
code-review submission was provided; the coverage report passes the configured
80% patch floor at 94.87%. The documentation-only completion commit's checks are
available on the [PR](https://github.com/SanderVocke/shoopdaloop/pull/926/checks)
and must pass before handoff.

Native shared-memory delivery retains its existing single-attachment contract;
reattachment uses sockets. Shared-memory reattachment, simultaneous clients,
host/page crash recovery and Lua VM restoration remain outside the recovery scope.
