> **Historical snapshot — not an active plan or current acceptance claim.**
> [History index](../../../README.md) · [Current plan](../../../../../PLAN.md) · [Original at `fffc1faa`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/docs/integration/PARITY.md)
> Original text follows; only relative link destinations are adapted for browsing.
> Exact original bytes remain in the snapshot bundle. Old tasks, pauses and status apply only to that snapshot.

# Compatibility inventory for the engine-service direction

This is a bounded, source-grounded inventory, not a claim of feature parity. The
initial reference is `d038c208f46c98ac8d7561ddcf09ff7c8e7a9d94`. The intended outcome
is preservation of user-visible behaviour, not preservation of every incidental
restriction of the old implementation. Proposed ownership below is architectural;
a row is implemented only when evidence explicitly says so.

## Capability and ownership map

Most application-facing contracts are in
[`Backend`](https://github.com/SanderVocke/shoopdaloop/blob/fffc1faaf44f22aec32a66848cc5eb97c2ee7f9a/src/rust/shoop_backend/src/lib.rs). Named tests below are stable
search anchors in the indicated source, not assertions that they cover the new
service. M1 verification is still pending unless explicitly recorded in
[MILESTONE_1.md](MILESTONE_1.md).

| Capability | Current source/test reference | Intended owner / reuse | M1 scope and remaining gap |
| --- | --- | --- | --- |
| Record, stop, immediate play, wrap | `shoop_engine/tests/control.rs::a_graph_built_through_the_api_records_and_plays`; loop `examples/coordinated_loop.rs` | Existing loop resolver plus public capture/playback facade; service composes them | Required deterministic stereo/MIDI slice; no second loop clock |
| Seek, clear, content selection | Backend `loop_content_contract`; loop `runtime::Operation::Select`, transitions | Resolver owns musical cuts; media ledger owns retained revisions; app owns clear policy | Seek/select retained media required; broad destructive editing deferred |
| Sync and delayed transitions | `control.rs::a_loop_can_follow_another`; backend `set_loop_sync_source` | Existing resolver topology and `Timing`; future session adapter maps application IDs | Single-loop slice only; inter-loop parity is not established by M1 |
| Composite loops and programs | Backend `engine_backend_composite_contract_is_independent_and_transactional`; loop program/topology APIs | Existing resolver programs; future session orchestration and script compiler | Out of scenario scope; do not duplicate the program engine in the service |
| Replacing / overdub-style policy | `BackendLoopMode::Replacing`; current audio/MIDI channel mode implementations | Future application mode mapping and prepared routing around loop primitives | No production dependency on old mode internals. `Replacing` is not automatically additive overdub; a general overdub contract is not claimed |
| Dry/wet modes and routing | Backend `dry_wet_routing_matches_monitor_and_transition_truth_table`, `fake_and_engine_backends_align_ordinary_dry_wet_recording_and_take_correction` | Service/session graph policy; independent processor; resolver media windows as needed | Required fixed-delay evidence probe, not complete `PlayingDryThroughWet` / `RecordingDryIntoWet` support |
| Prerecord / recent-input grab | Backend `web_audio_and_midi_grab_adopt_recent_input_without_growing_in_the_callback`; engine channel input history | Future retained-input/capture integration; exact owning layer to follow evidence | Not required production functionality; identify missing prehistory rather than manufacture it |
| Preplay / processor render advance | Engine `app_backend.rs::set_n_preplay_samples`; `shoop_session::ChannelDocument` preplay fields; `shoop_latency::ProcessorRenderAdvance` | Pure checked latency math can be reused; future playback-window and session policies | Probe must distinguish actual DSP history from a logical timestamp offset |
| Recording offset, postroll, take correction | Engine audio `compensated_record_then_play_maps_postroll_across_callbacks_and_wrap`; MIDI counterpart; backend `take_alignment_window_contract` | Reuse `shoop_latency` arithmetic when contracts match; future capture-window lifecycle | Required precise gap report. Logical recording finish need not be physical capture finish |
| Graph routing and configuration | Backend connection/bus controls; loop `GraphActivate`, `ConfigurationActivate` | Runtime validates signal DAG; loop owns resolver/role publication; service prepares supported candidates | Initial graph before frame zero and a fixed-role route cut required; no arbitrary cross-domain transaction |
| Processors, presets and FX state | Backend `builtin_fx_renders_bypasses_and_restores_transactionally`, `oxisynth_controls_snapshot_and_session_state_are_transactional` | Independent DSP/plugin modules with companion control adapters; app composition | Deterministic prepared fixture only, typed preset request/result. Real plugin hosting and UI FX state deferred |
| Devices, ports and driver changes | Backend `driver_catalog_and_switch_contracts_are_typed_and_transactional`, connection snapshots | Existing device/host layer adapted later; no device code in portable service | Explicitly excluded; do not add a second device owner |
| Loop state and command outcomes | `control.rs::an_explicit_fence_makes_a_mutation_visible`; loop `Cut`, `Reply`, execution stream | Loop settled cuts; service distinguishes delivery admission from domain terminal outcomes | Required coherent loop queries; DSP and loop observations are not an atomic multi-domain snapshot |
| Audio peaks and MIDI activity | Backend `engine_peak_publication_resets_measurement_window`; old `state_mirror.rs` | Component/observer atomic mirrors; separate from reliable commands | Required peak-since-read semantics. Independent channel reads may name different measurement windows |
| Waveforms and content snapshots | Old `content_snapshot/{manifest,audio,midi,runtime}.rs`; app browser waveform publication | Existing off-render analysis/UI code can consume an adapted immutable content interface | Live-prefix export/read required, waveform generation deferred; retention cost is not delivery cost |
| Initial MIDI state, seek/stop cleanup | New loop `tests/midi.rs::seek_wrap_and_replacement_cleanup_precede_restoration_and_equal_time_packets` | Loop MIDI renderer/capture and runtime recovery; independent DSP preserves stable fan-in | Required full packets and initial metadata; no scalar activity-count substitute |
| Media import/replacement | Backend `fake_and_engine_backends_update_loop_content_without_session_replacement`; loop `media::Input`, retained media selection | Off-render preparation plus ledger-owned immutable media | Startup imports and retained selection required; arbitrary live uploading/adoption deferred |
| Persistence and session replacement | Backend `fake_and_engine_backends_satisfy_transactional_session_io_contract`; `shoop_session/{document,archive,media,resample}.rs` | Reuse format/archive/resampling off-render; future session-to-service adapter | Out of M1. No new archive format and no assumption that old engine resource IDs are portable |
| Undo / revision history policy | Backend exposes content/session replacement, not a dedicated `undo` operation | Future application history policy over immutable revisions/snapshots | No existing general undo contract is established by this inventory. Retaining an export is not implementation of undo |
| Scripting and MIDI control rules | `shoop_scripting::LuaRuntime`, `shoop_app_api`; application `shoopdaloop/src/main.rs` | Keep Lua/app policy off-render; later adapter targets the same service semantics | Deferred; do not put scripting into the loop planner or require it for the headless scenario |
| Browser and remote deployment | Existing production `browser_audio.rs` and `audio_worklet.js`; transport proposal | Later typed domain adapters over explicit delivery infrastructure | Libraries/Wasm tests only in M1; no new worklet, IPC, network or serialization protocol |

## Semantic boundaries to preserve

1. **Musical work is not optional control work.** Admitted transitions and
   autonomous wraps must progress despite stalled normal results. Optional exports
   may refuse; they must not stop recording.
2. **A receipt has phases.** Queue admission, runtime delivery and resolver terminal
   execution are distinct. A native wrapper must not collapse them into a success
   boolean. DSP receipts must report the actual applied frame/revision separately
   from the generic transport's callback-start completion metadata.
3. **One source of timing.** DSP processing uses the `ProcessBlock` absolute frame
   supplied by the loop/runtime. There is no second clock, nested engine or extra
   `Engine::process` call to replenish callback allowances.
4. **Independent component control.** A prepared component-native bridge is only
   acceptable if bounded admission, exact application, reliable results and
   recovery are demonstrated. Direct access to another processor's mutable state,
   magic float IDs and a service-only enum without a dispatch path are not answers.
5. **State and telemetry are different cuts.** A loop `Cut` is coherent resolver
   state. Meter atomics do not make a global transaction; a retained export is an
   immutable prefix, not the current mutable recording.
6. **Do not fake latency parity.** Preserve raw physical capture, label proposed
   logical windows separately, and identify the absent retained-history/tail/window
   mechanism when the existing public facade cannot express the mapping.
7. **No per-operation old-engine fallback.** When eventual application integration
   begins, select the backend at a clear lifecycle boundary. This slice does not
   route unsupported operations into another engine behind the caller's back.

The rest of the feature-parity stack remains undescribed at architectural depth.
This inventory records likely owners and follow-up questions, not frozen APIs for
sessions, production plugins, persistence, scripting or remote hosting.
