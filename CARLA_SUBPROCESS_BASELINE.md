# Carla subprocess baseline

## Purpose

This records the direct-host behavior and measurements that the subprocess implementation must preserve or deliberately supersede. Results are from `4a50e1d0be18e51f963ccf524bb9478a69b5ab69` on x86_64 NixOS Linux with a PREEMPT_RT kernel, Rust 1.94.1, and installed Carla Rack/Patchbay LV2 bundles.

## Behavior inventory

| Behavior | Current ownership and path | Regression evidence |
|---|---|---|
| Chain creation and availability | The native application backend instantiates `CarlaLv2Host`, exposes unavailable creation as a non-ready FX handle, creates typed internal ports, and inserts the host into the engine session. | `app_backend::tests::carla_fx_chain_handle_instantiates_when_plugin_is_available`; unavailable branches in the same creation path |
| Audio and MIDI processing | The engine session copies routed port data into host buffers, stages frame-offset MIDI atom events, invokes the host inline, and writes wet audio to routed outputs. | `session::tests::carla_fx_chain_audio_route_runs_from_session_ports_to_wet_output`; `lv2_carla::tests::atom_sequence_buffer_roundtrips_midi_events`; dry/wet QML test surface |
| Active/bypassed behavior | The application handle updates desired activity; inactive hosts do not process and wet output is gated. | `session::tests::inactive_carla_fx_chain_bypasses_processing_and_tails`; dry/wet QML activation cases |
| External UI | `CarlaLv2Host` loads the external-UI descriptor with LV2 instance access, runs it on its UI thread, observes normal closure, and tears it down on hide/drop. | `lv2_carla::tests::shows_and_hides_carla_external_ui_when_opted_in` (manual opt-in) and descriptor discovery tests |
| State save/restore | The host calls the portable LV2 state interface and serializes URID-keyed values as base64 JSON. QML stores the returned string as the FX chain's `internal_state`. | `lv2_carla::tests::state_string_uses_base64_json_shape`; `lv2_carla::tests::instantiates_and_runs_installed_carla_rack_when_available`; application-backend Carla test; session save/load suites |
| Session compatibility | Hosting mode is not part of an FX descriptor. Chain type, ports, and state are session data; direct/subprocess selection is application policy. | `fx_chain.1` schema, QML descriptor generation, session save/load tests |
| UI controls | QML adapts ready/active/visible state and invokes active, visibility, state save, and state restore on the Rust handle. | FX-chain frontend/QML tests and track-button behavior |
| Shutdown | The engine is returned from the driver before session destruction so Carla instances are dropped on the owning control thread. | application-backend shutdown implementation and existing engine/QML shutdown suites |
| Current realtime costs | Each cycle clones the title/host map into a vector, locks each active host, formats route names, linearly searches ports, allocates MIDI staging/output vectors, and invokes Carla inline. | Source audit of the session Carla processing stage; realtime lock guard currently grants an explicit Carla lock exception |

## Baseline commands and results

### Focused engine Carla tests

```text
cargo test -p shoop_engine --features app_backend carla -- --nocapture --test-threads=1
```

Result: 9 passed, 0 failed. Carla Rack, Patchbay, and Patchbay 16x were discoverable. The external-UI smoke case passed as an explicit environment skip because `SHOOP_TEST_CARLA_UI` was not set.

The same filter with the default parallel test runner also passed, but emitted Carla/JUCE global-initialization assertions while several host tests overlapped. Carla-bearing validation must therefore run serially until process isolation removes that global in-process interaction.

### QML harness sanity

```text
QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_Backend.qml" \
  --no-crash-handling
```

Result: 3 passed, 0 failed.

```text
QT_QPA_PLATFORM=offscreen target/debug/shoopdaloop_dev.sh \
  --self-test \
  --test-files-pattern "$(pwd)/src/qml/test/tst_drywet_carla_patchbay_16_descriptor.qml" \
  --no-crash-handling
```

Result: 2 passed, 0 failed.

At baseline capture time, the larger `tst_TrackControlAndLoop_drywet_carla.qml` failed to create its root QML object because the `QtMaterialDesignIcons` submodule was absent. After initializing recursive submodules, all six activation/MIDI-gating cases pass in both direct and subprocess modes. Final bridge measurements are recorded separately in `CARLA_SUBPROCESS_BENCHMARK.md`.

### In-process microbenchmark

Command:

```text
cargo run --release -p shoop_engine --features lv2 \
  --example carla_inprocess_benchmark
```

Each row is the arithmetic mean of 2,000 active calls after 100 warm-up calls at 48 kHz. This measures the current host call and plugin work, not session routing, tail latency, or scheduler jitter.

| Chain | Channels | Frames | Mean µs | Block budget % |
|---|---:|---:|---:|---:|
| Rack | 2 | 32 | 0.246 | 0.037 |
| Rack | 2 | 64 | 0.239 | 0.018 |
| Rack | 2 | 128 | 0.271 | 0.010 |
| Rack | 2 | 256 | 0.281 | 0.005 |
| Rack | 2 | 512 | 0.296 | 0.003 |
| Rack | 2 | 1024 | 0.372 | 0.002 |
| Patchbay 16x | 16 | 32 | 4.206 | 0.631 |
| Patchbay 16x | 16 | 64 | 4.099 | 0.307 |
| Patchbay 16x | 16 | 128 | 4.798 | 0.180 |
| Patchbay 16x | 16 | 256 | 7.000 | 0.131 |
| Patchbay 16x | 16 | 512 | 10.367 | 0.097 |
| Patchbay 16x | 16 | 1024 | 15.899 | 0.075 |

Final comparison must add end-to-end direct and subprocess routing, percentile/tail data, fallback/deadline counts, and Windows/macOS evidence rather than treating these means as completion evidence.
