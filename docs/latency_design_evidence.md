# Latency compensation design evidence

This note records Stage 0 facts that constrain the latency implementation. It is not a claim that compensation is implemented.

## OxiSynth event timing

The engine uses `oxisynth` 0.1.0. `OxiSynthProcessor::process` renders up to each MIDI event offset, applies the event, and then resumes rendering. The dependency's voice renderer nevertheless updates on 64-frame internal boundaries.

The characterization tests in `shoop_engine::oxisynth::tests` cover offsets 0, 1, 31, 63, 64, 65, 127, 128, and 255. They establish:

- note-on, note-off, sustain, modulation, pitch bend, and channel pressure are applied at the supplied event offset by the wrapper;
- a timestamped render is sample-for-sample identical to explicitly rendering the prefix and applying the same events at offset zero for the suffix;
- consecutive odd-sized process calls are sample-for-sample identical to one contiguous render after event timestamps are rebased;
- the first note-on-dependent output occurs at the next 64-frame internal render boundary, plus the preset's constant two-frame onset in this fixture;
- the internal-boundary contribution is phase-dependent `0..63` frames, not a fixed 64 frames;
- the boundary behavior is the same with reverb and chorus disabled and with both additive sends enabled.

The two-frame fixture onset is not declared as processor latency: it belongs to the selected SoundFont/preset rendering behavior. Until OxiSynth is patched or replaced, its processor observation must therefore retain the validated `0..63` algorithmic range and must not claim exact zero or fixed 64-frame latency.

Verification command:

```sh
cargo test -p shoop_engine oxisynth::tests
```

## JACK external send/return cycle behavior

The application JACK client, a deterministic external copy client, and a sink were connected as a cycle:

```text
source -> Shoop input -> Shoop send -> external copy -> Shoop return -> Shoop output -> sink
```

`external_send_return_adds_one_callback_period_at_two_buffer_sizes` timestamps one unique pulse with JACK frame time at the source and sink. Against a dedicated JACK2 1.9.22 dummy server, it measured exactly one callback period at both tested sizes:

| JACK period | Measured source-to-sink frame delta attributable to the cyclic send/return |
| ---: | ---: |
| 64 frames | 64 frames |
| 128 frames | 128 frames |

The test changes the dedicated server period between measurements, restores it on exit, and serializes the JACK integration-test binary because period size is server-global. The result is evidence for one separately identified backend-hop/callback-cycle component on this external route; it is not a claim about plugin or physical-device latency.

### Physical JACK loopback procedure

When physical capture/playback ports are available, connect one application output to a hardware output and the corresponding hardware input to one application input with a direct cable. Disable monitoring, resampling, and device effects. At 44.1 kHz and 48 kHz, and at two JACK periods, emit isolated identified impulses at least four periods apart. Record JACK frame time at dispatch and at capture, subtract the callback-route contribution reported by the JACK latency callback, and report minimum/maximum residual over at least 32 impulses. The accepted residual tolerance is the hardware converter's published or repeatedly measured range plus one frame; an unexplained callback-period residual fails validation. Repeat after graph reorder and port remove/re-add, and retain JACK port latency ranges with the measurements.

This development environment exposed a running software JACK server (the deterministic latency callback and send/return tests ran without the missing-backend allowance) but no enumerated physical capture/playback endpoints or ALSA enumeration tools (`aplay`/`arecord` were unavailable). The physical cable run is therefore recorded as not applicable here rather than represented by the software-client measurement.

The production OxiSynth provider therefore publishes a phase-dependent `0..=63`
frame range with maximum/minimum/midpoint selection and signed trim available
through the shared latency policy. This is algorithmic event-application timing;
SoundFont attack, reverb, and chorus behavior is not included. Native and Wasm
engine routes use the same observation constructor.

## Carla 2.5.10 latency surfaces

The bundled runtime is pinned by `third_party/carla/runtime-lock.json` to Carla 2.5.10, revision `ad09259060a4e660a5033024406a1c3cc9f9c198`. The checked Native header digest is `c1b1a806a95ee2e4935eec9699c233e6a3ee27fcc8da37002bb0034c9d81854f`.

Inspection of that exact source/runtime establishes:

- `NativePluginDescriptor` has no latency query.
- `NATIVE_PLUGIN_OPCODE_GET_INTERNAL_HANDLE` exposes a `CarlaEngine*`, and the runtime exports `carla_get_native_plugin_engine` and `carla_create_native_plugin_host_handle`.
- The Native runtime does not export `carla_get_plugin_latency`, `CarlaEngine::getPlugin`, or `CarlaPlugin::getLatencyInFrames`; an independently linked adapter cannot defensibly call those hidden symbols.
- Internally, each hosted `CarlaPlugin` has `getLatencyInFrames()`. Carla updates that value for its supported plugin formats.
- Rack processing visits enabled plugins serially in plugin-ID order. Its aggregate input-to-output latency is therefore the checked sum of the enabled serial processors that actually participate in the rack.
- Patchbay and Patchbay16x use an explicit graph. Their latency cannot be inferred by summing the plugin catalog: each reachable input-to-output route must be traversed, branches must retain min/max path totals, and feedback or an uninspectable graph is ambiguous.

Chosen adapter boundary:

1. Add a small, versioned C ABI to the pinned Carla Native runtime build, rather than relying on C++ vtable layout or wall-clock bridge timing.
2. Implement the ABI inside the Carla source build, where `CarlaEngine`, `CarlaPlugin`, and the internal Rack/Patchbay graph are available.
3. Return per-input/per-output exact or ranged path records plus an explicit unsupported/ambiguous result. Use checked frame summation.
4. Validate an ABI version and the exact runtime identity before enabling automatic values.
5. Keep unmodified or mismatched runtimes usable for audio, but report processor latency as unknown/manual-only.

Relevant upstream surfaces in the pinned source are `source/includes/CarlaNative.h`, `source/plugin/carla-native-plugin.cpp`, `source/backend/CarlaPlugin.hpp`, and `source/backend/engine/CarlaEngineGraph.cpp`.

## Uncompensated action baseline

The current behavior is pinned before compensation changes by the following focused tests:

| Behavior | Evidence |
| --- | --- |
| Immediate audio monitoring | `current_monitoring_is_sample_identical_across_callback_sizes` passes twelve unique samples through callbacks of 3, 5, and 4 frames without recording or alteration. |
| Immediate MIDI monitoring and cleanup | `muting_midi_passthrough_cleans_forwarded_notes_exactly_once` and the JACK MIDI fanout test pin timestamped passthrough and state cleanup. |
| Direct/dry/wet recording and ordinary playback | `audio_record`, `audio_playback`, `midi_record`, `midi_playback`, and session end-to-end tests pin current raw frame placement and role routing. Session wrap tests cover callbacks that split at loop boundaries. |
| Play after record | `recording_only_uses_first_occurrence_and_honors_both_pass_end_options` pins the record-pass boundary action and iteration-zero playback transition. |
| Planned preplay | `audio_preplay`, `midi_preplay`, and the preplay MIDI state tests pin the stopped-to-playing boundary and media lead-in across sync/callback splits. |
| `PlayingDryThroughWet` | Existing mode-matrix tests pin direct/dry/wet routing. `current_dry_through_wet_dispatches_without_render_ahead` proves the dry event is currently dispatched at logical frame `E` and a deterministic `P`-frame processor is audibly late by exactly `P`. |
| `RecordingDryIntoWet` | Existing mode-matrix tests pin role routing. `current_dry_into_wet_records_the_uncompensated_delayed_return` proves current wet replacement lands at `E + P` across callback boundaries. |
| Prerecord | `audio_prerecord`, `midi_prerecord`, and MIDI state edge tests pin adoption, `start_offset`, and event ordering around the transition. |
| Grab | `current_grab_adopts_raw_history_across_callback_boundaries` pumps 3-, 5-, and 4-frame cycles and proves the retrospective raw window is adopted unchanged. The transactional no-allocation test pins all-or-nothing multi-loop adoption. |
| Replacement | `audio_replace`, `audio_replace_onto_smaller`, `replacing_midi_through_a_session_overwrites_loaded_events`, and MIDI replacement allocation tests pin current in-place and wrap behavior. |

These tests describe current uncompensated behavior. In particular, the dry processor tests intentionally assert lateness; later stages must replace those expectations with compensated target-frame oracles rather than treating the baseline as desired behavior.

## Existing timing and state transport inventory

Every listed shape either carries channel timing, processor state, or backend/runtime status and must be reviewed when latency fields are introduced.

| Layer | Current shapes and timing-bearing fields | Required latency impact |
| --- | --- | --- |
| Engine callback state | `LoopState`; `AudioChannelState`; `MidiChannelState`; `AudioPortState`; `MidiPortState`; state mirrors for each | Add current per-port observations, raw/logical positions, latched operation recipe, revisions, and warnings without callback allocation. Keep `start_offset` and `n_preplay_samples` as media geometry. |
| Engine backend session transfer | `AudioContentReplaceItem`; `MidiContentReplaceItem`; `AudioDriverState`; Carla/OxiSynth control state strings | Add bounded scalar take snapshots, retained bounds, and provider status. Validate before mutation. |
| Backend domain | `BackendStatus`; `BackendTrackState`; `BackendLoopState`; `BackendAudioContent`; `BackendMidiContent`; channel data/chunks and update types; `BackendLoopContent`; `BackendSessionTrack`; `BackendSessionData`; `BackendSnapshot`; connection snapshots | Carry observations and policy without converting unknown to zero. Session capture/restore must include frozen take provenance and operation warnings. |
| Audio worklet protocol v13 | `Command::SetLoopTiming`; loop content/session transfer commands; `WireGrabRequest`; `WireMidiEvent`; `WireSnapshot`; `WireTrackState`; `WireTrackFxState`; `WireLoopState`; `WaveformChunk`; `MidiDataChunk`; application/host port and confirmed-link records | Bump the protocol when adding bounded policy, observation, take, and error records. Recheck `COMMAND_MAX_BYTES`, transfer limits, journal supersession, stale generation, and raw-host fixtures. |
| Carla worker protocol v2 | `WorkerHello`; `ControlRequestKind::Instantiate`; `ControlRequest`; `WorkerStatus`; `ControlResponseKind`; `ControlResponse`; `PrototypeBlock`; `PrototypeBlockResult`; `MidiEvent`; parent/worker envelopes | Add processor observation and revision to status/control records, bump the protocol, and validate all capacities and generations. Shared-memory audio remains timing data but does not itself report latency. |
| Exact session document v6 | `SessionDocument`; `TrackDocument`; `LoopDocument`; `ChannelDocument` (`data_length_frames`, `start_offset_frames`, `preplay_frames`, recording FX reference); `PortDocument` (`ringbuffer_frames`); `FxChainDocument`; `FxStateDocument` | Add typed observation, policy, take alignment, retained bounds, warning, and one scalar alignment document. Bump the exact accepted session document version. |
| Exact media v1 | `ExactMidi` (`sample_rate`, `length_frames`, start state, ordered events); `ExactMidiEvent`; `LoopAudio`; manifest `MediaRecord` frame counts | Preserve raw bounds and alignment identity. Equal-frame MIDI `order` remains authoritative. Bump only the affected exact-media envelope/document version. |
| Application model/intents | `StatusState`; `AudioDriverRuntimeState`; `TrackControlState`; `TrackFxState`; `LoopState`; application/host/connection states; `WaveformChannelState`; `MidiSequenceChannelState`; loop details; loop timing intent | Expose detected, selected, applied, changed, incomplete, and unknown values. Keep UI frame/ms conversion out of authoritative engine arithmetic. |

Standard WAV and MIDI are not metadata-preserving transports. Their exporters must render a logical compensated view by default; raw export must be an explicit separate choice.

## Initial bounded capacities

These Stage 0 limits are the implementation targets for checked constructors and prepared realtime storage. They may only be raised with matching memory/work analysis and boundary tests.

| Capacity | Initial limit | Basis |
| --- | ---: | --- |
| Active sample rate used for frame-bound calculations | 384,000 Hz | Existing application device selector maximum. |
| Effective automatic/manual compensation per recipe | 768,000 frames | Two seconds at the supported maximum sample rate, matching the existing two-second bounded internal delay facility. Values are stored in frames and rejected, not clamped. |
| Retained preroll per take | 768,000 frames | Same explicit manual-negative envelope; allocated before arming. |
| Retained postroll per take | 768,000 frames | Same positive-advance envelope; allocated before arming. |
| Latency path records in one prepared snapshot | 256 | Existing maximum audio ringbuffer adoption channel count. Per-provider sublimits may be lower, such as Carla's 16 audio channels. |
| Automatic components in one recipe | 16 | More than the five public component kinds while keeping overlap/path splitting bounded. |
| Observation revisions retained per ring history | 4,096 | Existing engine command queue capacity; overflow must increment a diagnostic and make affected grabs variable/incomplete rather than overwrite exact provenance silently. |
| Callback sub-block iterations | 16 | Existing `Session::MAX_SUB_BLOCKS`; compensation must not introduce an unbounded split loop. |

The existing session archive permits large media (`16 GiB` uncompressed by default), but that decode limit is not permission to allocate margins on the realtime thread. Audio and MIDI retained storage must be prepared before arming, and insufficient capacity must reject or visibly degrade the operation.
