# Latency awareness and compensation implementation plan

## Status and execution contract

This document is the implementation contract for adding latency awareness and compensation to ShoopDaLoop. It is a plan only; no stage is complete until its checklist and verification are complete.

Execution contract:

- Keep this plan updated as work progresses and check off completed items.
- Commit each completed stage or meaningful milestone.
- Implementation steps may be revised when new evidence warrants it.
- Design rules may be revised for a documented, well-supported reason.
- Goals and acceptance criteria must not be changed without explicit user approval.
- Preserve evidence from characterization work in tests or focused design notes rather than relying on undocumented observations.
- If a required backend cannot expose a defensible latency value, stop that provider at `Unknown`, retain manual control, document the evidence and attempted paths, and do not silently substitute zero.
- If no realtime-safe design remains for a required operation, stop with the violated constraint, measurements or tests that demonstrate it, attempted alternatives, and the smallest decision needed from the user.

## Goals

1. Preserve the current lowest-latency monitoring behavior: live audio and MIDI monitoring, including monitored FX, must continue through the shortest available path without compensation-induced delay.
2. Align newly recorded dry, direct, and wet material to the musical timeline using the latency that applied when the material was captured.
3. Render dry material early enough in `PlayingDryThroughWet` and `RecordingDryIntoWet` for the processed output to land on the intended musical frame.
4. Let users independently enable, disable, override, and trim meaningful latency components, including external capture, processor/FX, cue/output, backend buffering, and manual correction.
5. Obtain latency from authoritative providers where available: JACK for connected port paths, Carla for hosted processor paths, and validated OxiSynth behavior for the built-in synth.
6. Preserve raw capture and latency provenance for ordinary new recordings, and apply alignment non-destructively at playback wherever the existing destructive replace semantics do not require canonicalized writes.
7. Keep existing native, browser, dummy/offline, session, resampling, realtime, and transactional safety contracts intact.
8. Expose truthful exact, ranged, estimated, manual-only, changed, and unknown states rather than presenting uncertain timing as exact.

## Scope

### In scope

- Audio and MIDI capture latency.
- Direct, dry, and wet loop channels.
- Regular recording, prerecord, grab, replacement, wet rerecording, dry-through-wet playback, and play-after-record transitions.
- Internal hosted processors, external send/return processors, and the built-in synth.
- JACK capture/playback latency ranges and JACK latency propagation.
- Carla Rack, Patchbay, and Patchbay 16x processor latency.
- OxiSynth MIDI-to-audio algorithmic timing.
- Conditional cue/output compensation for performances made against ShoopDaLoop output.
- Per-device defaults, per-track operation policy, and per-take observations/provenance.
- Signed manual adjustments and retained preroll/postroll margins.
- Session persistence, exact media persistence, import/export behavior, sample-rate conversion, UI, diagnostics, and tests.
- Capability reporting for backends that cannot measure some components.

### Explicit boundaries

- Live signals cannot be advanced in time. Full alignment of a live monitored source with a delayed processed source would require delaying the faster source; performance monitoring will not do that automatically.
- Processor latency is not the same as a reverb tail, instrument sample attack, SoundFont envelope, predelay used as a musical effect, or processor warm-up. Tail/warm-up handling is included only where required to make compensated loop transitions and wet renders complete; it is not automatic removal of musical timing.
- Differently delayed sources mixed before reaching one ShoopDaLoop input cannot be separated afterward. They require separate application inputs/channels for independent compensation.
- Automatic acoustic roundtrip calibration is not required for the first complete implementation. Manual correction must cover unknown device and performer timing; a future calibration tool may use the same domain model.
- Global mixer PDC that delays all faster live paths to a slowest path is not enabled in performance mode. Offline/consolidated rendering may align all paths without the live-monitoring restriction.
- External MIDI implementations without a shared sample clock remain coarse-timed according to their existing backend contracts; compensation must not claim sample-exact input timing where none exists.

## Immutable acceptance criteria

The following criteria may not be weakened or removed without explicit user approval.

1. **Monitoring remains immediate.** Enabling any compensation option adds no intentional buffering or delay to the existing live monitoring path. With compensation enabled but no recording or compensated dry playback active, monitored output is sample-for-sample equivalent to the uncompensated path, apart from existing processor behavior.
2. **Deterministic capture alignment.** With a deterministic source delayed by `N` frames, a newly recorded direct or dry impulse/event plays at its intended logical frame when the relevant component is enabled and remains delayed by `N` when disabled.
3. **Deterministic wet alignment.** With input delay `I` and processor delay `P`, a newly recorded live wet impulse/event is advanced by `I + P`, plus cue/output only when that component is enabled.
4. **Dry-through-wet PDC.** A dry impulse/event processed through a deterministic `P`-frame processor emerges on the intended wet frame in `PlayingDryThroughWet`, including across callback boundaries, loop wrap, and stopped-to-playing transitions.
5. **Wet rerecording avoids double compensation.** `RecordingDryIntoWet` renders dry material ahead by the active processor path and writes the wet result on the canonical timeline; later wet playback does not apply the same processor delay again.
6. **Independent user control.** External capture, processor/FX, cue/output, backend buffering, and manual correction are independently visible and controllable. Each automatic component supports disable, manual replacement, and signed trim without changing the detected observation shown to the user.
7. **Conditional output semantics.** Cue/output latency affects record placement only when selected by policy for a performance made against ShoopDaLoop output, or when explicitly aligning to an external physical clock. It is not unconditionally added to ordinary internal playback.
8. **Stable takes.** Hardware, graph, buffer-size, or processor-latency changes do not silently retime an existing take. Observations used by a recording/render are latched at the operation boundary and persisted.
9. **Dynamic-change truthfulness.** A latency change during prerecord, recording, postroll, grab history, replacement, or wet rendering is detected and persisted as a warning/status. The implementation never silently describes a variable-latency operation as exact and stable.
10. **Complete capture windows.** Positive recording advances retain enough postroll to preserve the end of the logical take; retained preroll supports the configured negative/manual range. Requests beyond retained media fail visibly or render explicit silence with a visible incomplete status, never silent clamping.
11. **JACK awareness.** On JACK, connected application inputs expose capture ranges and selected application outputs expose playback ranges. ShoopDaLoop advertises relevant internal input-to-output latency through JACK’s latency callback without allocation or blocking.
12. **Carla awareness.** Supported Carla runtime versions query hosted processor latency from Carla-derived state. Rack and patchbay routing are not treated as equivalent; unsupported or ambiguous aggregate paths are `Unknown`/ranged and manual, never assumed zero.
13. **OxiSynth truthfulness.** OxiSynth behavior is covered at event offsets on and off its internal render boundaries. The declared latency is derived from those semantics; a fixed 64-frame value is not used unless tests prove it is a fixed end-to-end delay.
14. **Ranges stay ranges.** Exact, minimum/maximum, estimated, manual-only, and unknown observations remain distinguishable through engine, backend, application, UI, protocol, and persistence layers.
15. **Per-path correctness.** Independent application input channels and processor output paths may carry different latency observations and selected values. A scalar track value may be used only when all affected paths are proven equivalent or when the UI explicitly applies a conservative aggregate with a warning.
16. **Replacement and grab are defined.** Grab and destructive replacement have explicit latency semantics, tests, and provenance. They may not silently inherit ordinary-record assumptions that do not hold for retrospective or mixed-generation content.
17. **Persistence is exact.** A saved and reloaded session preserves raw media, logical loop timing, latency observations, applied component policies, retained margins, warning state, and current compensated playback identity at the same sample rate.
18. **Resampling is deterministic.** Latency frame counts, ranges, signed trims, retained margins, and alignment positions use checked documented conversion rules and survive a session sample-rate change without adding a cycle or changing component identity.
19. **Import/export is explicit.** Session and exact Shoop media preserve latency metadata. Standard WAV/MIDI export defaults to the logical compensated view; raw export is explicitly selectable. Imported media without metadata has zero/unknown provenance rather than invented detected latency.
20. **Realtime safety.** No feature path adds heap allocation, ordinary mutex acquisition, filesystem/network I/O, unbounded work, or logging to realtime processing. Existing no-allocation/no-lock tests remain green and new compensated paths are covered.
21. **Boundedness.** Preroll, postroll, latency history, bridge messages, path maps, and render-ahead state have explicit capacities and overflow diagnostics. No latency value can cause unchecked arithmetic, unbounded storage growth, or an unbounded sub-block loop.
22. **Backend honesty.** Dummy/test backends are deterministic; JACK uses measured ranges; CPAL/midir and browser capabilities report only values their APIs support; unavailable measurements remain manual/unknown.
23. **Transactional safety.** Failed session load, driver switch, processor restore, latency-provider refresh, or content finalization leaves the prior usable session and policy intact.
24. **Documentation and diagnostics.** User documentation explains each component and the output-latency condition. Runtime diagnostics expose detected, selected, applied, changed, incomplete, and unknown states without requiring tracing.
25. **Full validation.** Native workspace tests/build, tracing inventory, WebAssembly builds/tests, browser smoke checks where available, and feature-specific physical JACK/Carla tests pass under the project’s documented environments.
26. **Automated loop-action coverage.** Deterministic automated tests verify the exact expected raw, logical, dispatch, and audible frames for ordinary play, record, grab, planned preplay, `PlayingDryThroughWet`, and `RecordingDryIntoWet`. Each action is covered with zero and nonzero latency, relevant components independently enabled/disabled/overridden/trimmed, callback and loop-boundary crossings, and both audio and MIDI wherever the action supports them. Physical tests supplement but do not replace this automated matrix.

## Design rules and constraints

### Time references and signs

Use one documented sign convention throughout:

- A positive **capture advance** means media was observed late and its logical playback reads later raw media earlier: `raw_frame = logical_frame + capture_advance`.
- A positive **render advance** means dry material is dispatched before its intended wet-output frame: `dispatch_frame = target_frame - render_advance`.
- A signed **manual trim** is added to the selected automatic value. Checked arithmetic must reject overflow and unsupported negative effective values where a provider cannot represent them.
- Existing channel `start_offset` remains media-layout geometry for prerecord/loaded content. It is not renamed or reused as detected latency.

For a live recording made against an application cue:

```text
direct/dry take capture advance = input_capture_path + selected_cue_output_path + manual
wet take capture advance = input_capture_path + processor_path + selected_cue_output_path + manual
```

Each named path is an end-to-end, non-overlapping interval. For example, a JACK capture range that already includes device and upstream graph buffering is one `input_capture_path`; the resolver must not add a second backend-buffer component covering the same interval. Likewise, a JACK playback range may already be the full cue-output path. Providers must identify path endpoints/scope, and recipe validation must reject or visibly mark overlapping automatic components rather than double count them.

For current dry-through-wet rendering:

```text
dry dispatch advance = current processor_path + backend_processor_hops + manual
```

The take’s capture alignment and the current render advance compose, but remain separately inspectable.

### Monitoring policy

- Detection and display are always allowed during monitoring.
- Capture metadata may be armed while monitoring.
- No capture or processor component delays monitoring.
- If a user asks to align a faster monitored path to a slower live path, the UI explains that doing so would add monitoring latency and leaves the path unchanged.

### Mode behavior matrix

| Operation | Direct channel | Dry channel | Wet channel | Compensation behavior |
|---|---|---|---|---|
| Live monitoring | Pass current input | Pass current input to send/processor | Pass current processor return to output | Observe only; never delay monitoring |
| `Recording` | Capture raw plus input/cue snapshot | Capture raw plus input/cue snapshot | Capture raw plus input, processor, and cue snapshot | Apply take alignment during later logical playback; retain postroll |
| `Playing` | Play logical compensated take | Silent in ordinary wet-track playback | Play logical compensated wet take | Apply frozen take alignment only; current output latency is common and not added |
| `PlayingDryThroughWet` | Play logical take where supported | Play logical take early by current processor path | Do not play recorded wet; pass current processor result | Compose frozen take alignment with current render advance |
| `RecordingDryIntoWet` | Follow existing direct replacement semantics | Play logical take early by current processor path | Capture/replace at canonical logical position | Mark processor advance applied during render; do not apply it again on wet playback |
| `Replacing` | Stage raw replacement with latched recipe | Stage raw replacement where supported | Stage raw wet replacement where supported | Commit into logical coordinates and retain bounded operation provenance |
| Grab | Adopt retrospective raw ring window | Adopt retrospective raw ring window | Adopt return ring window where available | Resolve against bounded observation history and warn on revision spans |
| Play after record | Read settled logical prefix while finalizer captures tail | Same | Same | Defer only if required raw source frames will not be ready by their playback deadline |

### Latency observation model

Introduce shared domain types in a low-level crate usable by engine, backend, protocol, application, and session adapters. Names may be refined, but the semantics must remain:

```rust
struct LatencyRangeFrames {
    min: u32,
    max: u32,
}

enum LatencyCertainty {
    Exact,
    Range,
    Estimated,
    ManualOnly,
    Unknown,
}

enum LatencyComponentKind {
    ExternalCapture,
    Processor,
    CuePlayback,
    BackendBuffering,
    Manual,
}

struct LatencyObservation {
    range: Option<LatencyRangeFrames>,
    certainty: LatencyCertainty,
    sample_rate: u32,
    revision: u64,
    source_identity: String,
}

enum LatencyValueMode {
    Automatic,
    Manual(u32),
    AutomaticPlusTrim(i32),
}

struct LatencyComponentPolicy {
    enabled: bool,
    value_mode: LatencyValueMode,
    range_selection: LatencyRangeSelection,
}
```

Rules:

- Validate `min <= max` and nonzero sample rate whenever a frame count is meaningful.
- `Unknown` has no hidden zero. A disabled unknown component contributes zero only because the user disabled it; an enabled unknown component requires manual input or yields an unresolved operation warning.
- Default automatic range selection is `max` for conservative scheduling, but the UI displays the range and permits `min`, `max`, midpoint, or manual selection. Midpoint rounding is deterministic.
- Provider observations are immutable values identified by revision. Policy is mutable. A take stores both the observation snapshot and policy used.
- Source identity is stable and bounded: application port ID/role, host port ID where relevant, processor instance/path, or backend/device identity.
- Every automatic observation identifies the interval it covers, such as physical capture edge to application input, processor input to processor output, or application output to physical playback edge. Recipe resolution detects overlapping intervals and prevents double counting.

### Runtime views and snapshots

Add:

- Per-application-port capture/playback observations in backend connection/runtime state.
- Per-processor input-to-output path observations, not merely a catalog-wide scalar.
- A selected cue path per track or global recording profile.
- A resolved operation recipe that lists every component, selected frame value, total, and unresolved warnings.
- A callback-latched take snapshot containing detected observations, policy, selected values, sample rate, operation frame/revision, retained margins, and whether a component changed before finalization.

Do not have the application infer wet totals from display strings. Put pure resolution and validation in shared typed code and test it independently.

### Media and content semantics

For an ordinary new recording:

- Keep raw captured media in arrival order.
- Retain a source window before and after the logical take according to the resolved compensation plus configured safety margin.
- Store logical loop length independently from raw media length.
- Map logical playback through the take alignment; do not rewrite raw media merely to toggle a component.
- Repeat the selected logical window on loop wrap. Do not wrap early raw startup material into the end instead of using captured postroll.

For MIDI:

- Preserve start-state messages, equal-frame order, note state, and events in retained pre/post windows.
- Resolve logical event time through the same alignment contract as audio.
- Ensure note-offs, sustain, channel pressure, pitch bend, and state restoration are not dropped when an event crosses a loop or preplay boundary.

For destructive replacement:

- Treat replacement as an explicitly destructive exception to untouched ordinary capture.
- Latch a replacement recipe at the replacement boundary.
- Stage incoming raw replacement material and retained margins in the working mutation generation.
- Commit it into the existing take’s logical coordinate system using the resolved replacement advance, retaining the operation observation/provenance as a bounded alignment region when it differs from the base take.
- Playback and persistence must resolve bounded alignment regions deterministically. If the existing content representation cannot retain independently adjustable replacement regions without violating realtime constraints, first implement a non-realtime consolidation step and require it before mixing incompatible alignment policies; do not silently discard provenance.

For grab:

- Store bounded latency-observation history keyed to processed-frame ranges alongside the existing input ring history.
- A grab with one stable observation uses that snapshot.
- A grab spanning revisions is marked variable. The first implementation may choose one documented selected revision for the whole grab only if the warning is persisted and shown; exact segment-wise alignment is preferred when bounded region metadata is available.

For wet rerecording:

- Start dry rendering by the resolved current processor advance before the target wet boundary.
- Record/replace the wet output at canonical logical positions.
- Persist processor observation as `applied_during_render`, with zero remaining processor contribution for wet playback.
- Continue enough processor input/output around transitions to settle declared latency. Musical tails/warm-up use separately named bounds.

### Transition and finalization semantics

- Resolve and arm compensation before a transition needs render-ahead. A planned quantized transition may begin pre-rendering multiple callbacks or cycles early.
- An immediate transition with insufficient future knowledge must either be deferred by the required render advance or proceed with an explicit uncompensated-first-window warning. Default to deferral when user intent allows it.
- Recording mode and content-mutation finalization are separate states. A loop may enter play-after-record while a bounded postroll finalizer completes, provided every frame read is already available before its deadline.
- Saving, destructive edits, and session replacement wait/retry/reject while finalization owns unsettled content, matching existing content-snapshot rules.
- If total advance is at least the loop length, use checked multi-cycle source mapping and delay playback until required raw frames exist; do not assume latency is smaller than one loop.

### Multiple paths

- Keep per-channel input observations.
- Keep processor latency as input-to-output path data where the host can supply it.
- If several processor inputs reach one output with different values, retain a range or path set. The selected compensation policy must state which aggregate was chosen.
- If several external sources are already mixed at one JACK application input, report the JACK range and ambiguity. Never imply separate source correction.

### Realtime architecture

- Control/non-realtime code discovers providers, builds path snapshots, allocates buffers, validates policies, and prepares commands.
- Callback code reads fixed-size/atomic observations, latches revisions, advances bounded cursors, and publishes mirrors.
- Graph schedule rebuilding remains off the callback. Latency changes that alter only numeric offsets must not force a topology rebuild.
- Processor and port latency publication uses atomics or preallocated state mirrors.
- Retained pre/post storage is reserved before arming recording. Insufficient capacity rejects arming or marks a bounded fallback before recording starts.
- Add tracing counters/plots only through realtime-safe tracing macros and update the tracing inventory.

## Backend/provider design

### JACK

Implement both observation and propagation:

1. Read `Capture` latency ranges from ShoopDaLoop JACK input ports.
2. Read `Playback` latency ranges from candidate/selected ShoopDaLoop JACK output ports.
3. Register JACK’s latency callback. The current high-level crate does not expose it through `NotificationHandler`, so add a narrowly scoped `jack_sys` wrapper or contribute/consume an upstream extension.
4. In capture mode, propagate established input ranges through genuine internal pass-through/processor paths to output ports, adding internal min/max latency.
5. In playback mode, propagate established output ranges backward through those paths to input ports.
6. Do not advertise loop recording/playback as a combinational input-to-output path. Advertise only paths that actually pass a current signal, such as monitored direct routing or dry-to-processor-to-wet routing.
7. Build a lock-free fixed-capacity latency-route snapshot outside the callback. Port registration/unregistration must retire callback-visible handles safely before unregistering them.
8. Treat graph reorder, connection change, buffer-size change, sample-rate change, and processor-latency change as observation revisions and request JACK latency recomputation where supported.
9. Include any unavoidable external send/return callback-cycle delay in the path observation. Verify it with a delayed external test client rather than assuming same-cycle return.

### Carla

Extend the processor contract with dynamic latency observation:

```rust
trait CarlaProcessor {
    fn latency(&self) -> ProcessorLatencyObservation;
    // existing methods
}
```

- Add atomic latency publication to the in-process control bridge and realtime endpoint.
- Add processor latency and revision to the Carla subprocess worker status/protocol; bump and validate the protocol version.
- Refresh after instantiate, activate, restore, plugin graph changes, plugin parameter/state changes that report latency changes, buffer-size changes, and worker restart.
- Add fake Carla processor support for exact/ranged/dynamic delayed fixtures.
- For the pinned Carla runtime, provide a version-gated Carla-derived aggregate query:
  - Rack: query the serial input-to-output path, not merely one plugin.
  - Patchbay/Patchbay16x: query or calculate each reachable input-to-output path from Carla’s actual graph; do not sum unrelated plugins.
  - Feedback/ambiguous routes return a range or `Unknown` with diagnostics.
- Prefer a supported Carla API. If the Native descriptor lacks an aggregate API, implement a small C/C++ adapter built against the exact pinned Carla source/runtime and reject incompatible runtime versions for automatic latency. Keep the existing no-SDK source-build behavior by making unsupported runtimes manual/unknown rather than failing all Carla hosting.
- Do not infer sample latency from bridge wall-clock wait time. The current Shoop bridge waits for the same submitted block and adds no intentional sample block; hosted plugin and Carla internal buffering remain part of the queried value.

### OxiSynth

- Add characterization tests for note-on, note-off, sustain, controller, and pitch bend at offsets `0`, `1`, `31`, `63`, `64`, `65`, `127`, `128`, and across consecutive process calls of non-multiple-of-64 sizes.
- Detect the first affected output frame with effects disabled and with additive effects enabled. Separate algorithmic event application from SoundFont attack by using a controlled fixture/preset or direct internal-state assertion where possible.
- Confirm whether the current 64-sample internal output cache yields phase-dependent `0..63` delay.
- If feasible without violating realtime constraints, patch or fork the dependency/wrapper to apply events sample-accurately and report exact zero algorithmic latency.
- Otherwise report the validated range, choose through normal range policy, expose manual trim, and document residual phase uncertainty. Do not label it fixed 64.
- Reverb/chorus delay and instrument attack remain musical behavior, not whole-path latency.

### External dry/wet processors

- Derive send-to-return range from JACK path observations where possible.
- Include Shoop callback-cycle buffering, external client/plugin latency, bridge/network/hardware latency reported by JACK, and manual correction as separately identified components where they can be distinguished.
- If an external route is absent or forms an unresolvable graph, report unknown/manual.
- Latch the external path at dry-through-wet/record start and flag connection or range changes during the operation.

### CPAL/midir

- Report configured callback/buffer quantities separately from actual device latency.
- Use an authoritative CPAL API value only if the selected host exposes one with defined semantics.
- Otherwise expose capture/playback as manual-only/unknown, with optional estimated backend-buffering clearly labeled estimated.
- Preserve the current coarse MIDI timestamp contract. Manual MIDI input correction may shift recorded event association but cannot make arrival sample-exact.

### Browser/Web Audio/Web MIDI

- Carry latency types and policies through `shoop_audio_protocol`, `shoop_audio_worklet`, and `shoop_worklet_client` with bounded wire representations.
- Report the AudioWorklet render quantum as known engine scheduling granularity, not automatically as physical device latency.
- Publish `AudioContext.baseLatency`/`outputLatency` only where browser support and semantics are available; otherwise output is unknown/manual.
- Treat microphone capture latency as unknown/manual unless the browser supplies an authoritative value.
- Preserve the documented next-quantum Web MIDI timing class and do not claim sample-exact compensation.
- Built-in OxiSynth processor compensation remains available in the worklet using the same engine semantics as native.

### Dummy/test backend

- Add configurable exact/ranged input, output, and processor delays.
- Make delayed fixtures deterministic across arbitrary callback sizes.
- Use these fixtures for most acceptance tests so physical backends are not required in normal CI.

## Application policy and UI design

### Defaults

Register machine-level settings for future operations:

- Enable automatic external capture compensation.
- Enable automatic processor compensation.
- Enable cue/output compensation for overdubbing.
- Default range selection.
- Default signed manual trim.
- Retained preroll/postroll safety margin in frames or milliseconds with an explicit bounded maximum.
- Immediate-transition behavior: defer for pre-render versus proceed with warning.

Settings affect new operations and new track drafts, not existing take snapshots. Use the settings registry and migration mechanism; update the settings format documentation.

### Track/operation controls

Provide a latency panel reachable from processed track controls and selected loop details. It must show:

- Current input observations per channel/port.
- Current processor path observations per wet output.
- Candidate and selected cue/output observations.
- Backend buffering observations.
- Manual correction.
- Resolved totals for `Record dry/direct`, `Record wet`, `Play dry through wet`, and `Record dry into wet`.
- Frames and milliseconds at the active sample rate.
- Exact/range/estimated/manual/unknown status and revision.
- Whether the next operation will defer for pre-render.

Controls:

- Component enable checkbox.
- `Automatic`, `Manual`, and `Automatic + trim` selector.
- Range-selection selector when needed.
- Signed frame/ms editor with deterministic conversion.
- Cue output selector based on stable application/host connection identities.
- `Use current observations for this take`, `Restore take snapshot`, and optional `Consolidate/Bake logical alignment` actions.
- Clear warning and remediation text for unknown, changed, ambiguous, insufficient margin, unsupported backend, and incomplete operations.

Do not hide advanced components behind one total. A compact summary may show the total, but detailed controls remain available.

### Take display

Extend waveform/MIDI details with:

- Logical loop region.
- Raw retained media bounds.
- Capture advance and component markers.
- Current played logical/raw frame.
- Postroll/finalization and changed-latency warnings.
- Toggle between logical compensated view and raw captured view.

Editing the existing loop start/preplay/end remains media-layout editing and must not silently edit detected latency.

## Persistence and compatibility design

### Session document

Bump the exact accepted session document version and update validation, fixtures, archive tests, and `docs/session_format_v1.md`.

Add typed documents for:

- Latency observation: range, certainty, sample rate, source identity, revision-at-capture.
- Component policy: kind, enabled state, selection mode, selected frames, trim.
- Take alignment: component snapshots, resolved total, retained before/after frames, operation kind, applied-during-render flags, changed/incomplete status.
- Optional bounded replacement/grab alignment regions.
- Track latency policy and selected cue role/identity for future operations.

Rules:

- Missing metadata on imported or programmatically generated content means zero applied capture advance with unknown/no-capture provenance.
- Generated click content has exact zero capture latency.
- Loaded latency values are validated before backend mutation.
- Unavailable current providers do not invalidate a take whose frozen values are playable; they only make current automatic re-detection unavailable.
- Processor-state compatibility and latency metadata are validated together for wet renders where path identity matters.

### Exact media

Update exact Shoop audio/MIDI documents as required to preserve raw bounds and take alignment without losing equal-frame MIDI ordering or start state. Keep format-major compatibility rules explicit; reject unsupported future versions transactionally.

### Standard export/import

- WAV and standard MIDI export default to the logical compensated loop of declared loop length.
- Provide explicit raw export including retained pre/post material where the format can represent it.
- Standard imports begin with no detected capture provenance and zero applied alignment unless the user supplies an import offset.
- Exact Shoop imports restore metadata and ask for confirmation/resampling when sample rates differ.

### Resampling

- Convert unsigned ranges/margins with checked rational nearest or ceiling rules chosen and documented by semantic category.
- Convert signed trims/advances with checked nearest, ties away from zero, matching existing signed-offset rules.
- Preserve `min <= max` after rounding, widening by one frame if needed rather than inverting a nonempty source range.
- Recompute milliseconds only for display; frames remain canonical session values.
- Convert alignment-region boundaries and raw source references consistently with media resampling.

## Automated latency behavior test matrix

The feature test suite must use deterministic frame-domain fixtures and explicit timing oracles. Provider/hardware tests supplement this suite; they do not replace it. Tests must inspect frame indices, recorded raw media, logical media, processor input/output, state transitions, and persisted metadata rather than relying on listening, wall-clock sleeps, or peak-only assertions.

### Common deterministic fixture

Build a reusable harness with:

- Logical loop length `L` and callback size `B`, independently configurable.
- A source that emits uniquely identifiable impulses and MIDI messages at logical frames `E`.
- Exact input capture delay `I`, exact processor delay `P`, exact cue/output observation `O`, backend-hop delay `H`, signed manual trim `T`, and performance-reference offset `Q`. Use `Q = O` when simulating a performer responding to an application cue and `Q = 0` for an external/world-timed source.
- Separate direct, dry, and wet audio channels plus dry MIDI where supported.
- A processor that records the exact frame at which each audio impulse/MIDI event was dispatched and emits it exactly `P + H` frames later.
- Access to raw recorded frames/events, logical rendered frames/events, processor dispatch frames, wet output frames, loop mode/position, finalization state, and latched component snapshot.
- Deterministic driver pumping with no sleeps.

For exact fixtures, all frame assertions use equality with no tolerance. Audio samples/events must carry unique IDs so duplicate, dropped, wrapped, or reordered data is detected.

Core parameter sets must include:

- `B`: at least `1`, a non-power-of-two size, `64`, and `127`.
- Effective component values: `0`, `1`, `B - 1`, `B`, `B + 1`, `L - 1`, `L`, and `L + 1` where valid.
- Event positions: loop start, one frame after start, one frame before a callback boundary, on a callback boundary, one frame after it, one frame before loop end, and loop end/wrap.
- Policy variants: all automatic components enabled; each relevant component disabled alone; all disabled; manual replacement; positive and negative automatic trim; exact, ranged, and unknown observations.
- Transition variants: already active, planned from stopped with sufficient lead time, planned with exactly sufficient lead time, immediate with insufficient lead time, play-after-record, stop/restart, and repeated loop cycles.

Use table-driven tests to cover the full action/component contract. Pairwise reduction may be used for redundant callback/sample-rate combinations, but every behavior row and every component toggle must have at least one exact assertion; boundary cases listed below are mandatory, not pairwise-optional.

### Expected-frame oracles by action

#### Ordinary play (`Playing`)

Given raw take data whose event is at `E + A`, where `A` is the frozen resolved take capture advance:

- [ ] With take compensation enabled, direct or wet playback emits the event at logical frame `E`.
- [ ] Disabling one frozen component of value `C` emits it at `E + C`, with checked wrap semantics.
- [ ] Manual replacement and signed trim move playback by exactly the selected delta.
- [ ] Current device, cue, or processor observations do not change the take’s ordinary playback frame.
- [ ] Playback across callback boundaries and loop wrap emits one event per cycle in stable order.
- [ ] Starting, stopping, restarting, and play-after-record do not lose the first or last compensated frame.
- [ ] Audio and MIDI tests use the same logical mapping, including equal-frame MIDI ordering and state restoration.

#### Record (`Recording` followed by `Playing`)

For a source event intended for logical frame `E`, physically performed at `E + Q`:

- [ ] Direct/dry raw capture records it at `E + Q + I`; wet raw capture records it at `E + Q + I + P + H` for the deterministic live processor path.
- [ ] In the cue-followed case, `Q = O` and the latched direct/dry recipe resolves `I + O + T`, while the live wet recipe resolves `I + P + H + O + T`. In the external/world-timed case, `Q = 0`, cue compensation is disabled, and `O` contributes zero. Both cases enforce non-overlapping path validation.
- [ ] Subsequent compensated playback emits each event at `E`; disabling each component leaves exactly that component’s delay audible.
- [ ] In the cue-followed scenario, cue/output enabled removes exactly the simulated `Q = O` performance-reference offset. In the external/world-timed scenario, cue/output disabled contributes exactly zero. A deliberately mismatched toggle produces the expected `O`-frame early/late result.
- [ ] A final event requiring postroll is retained and aligned, including when total advance crosses a callback or exceeds one loop.
- [ ] Prerecord material needed by a negative/manual alignment is retained; out-of-margin requests produce the specified visible incomplete/error state.
- [ ] Latency changes during record/finalization preserve the latched value and set the changed warning.
- [ ] Play-after-record reads available compensated data on time or deterministically defers according to readiness policy.
- [ ] Direct audio, dry audio, wet audio, direct MIDI, and dry MIDI are covered where present.

#### Grab

For retrospective ring data with known source and latency revision history:

- [ ] A stable-history grab selects the exact raw window and emits its event at logical `E` under the latched recipe.
- [ ] Component disable/manual/trim variants alter the grabbed take by the exact expected frame delta.
- [ ] A grab whose selected window crosses a latency revision is marked variable and follows the documented revision/region selection rule.
- [ ] Grab windows crossing ring wrap, callback boundaries, and loop boundaries retain event identity and MIDI order.
- [ ] Insufficient retained history fails without partially mutating the target.
- [ ] Grabbed direct/dry/wet audio and supported MIDI paths are covered.

#### Planned preplay

Preplay tests must distinguish existing media lead-in from processor render-ahead:

- [ ] Ordinary compensated play with a retained media lead-in starts reading at the exact existing `start_offset`/preplay boundary, then applies take alignment independently.
- [ ] Planned dry-through-wet begins dry processor dispatch exactly `P + H + T` frames before the audible transition while the loop’s public mode remains in the expected pre-transition state.
- [ ] The processor’s first valid wet event lands exactly on the target transition frame; no uncompensated duplicate or unintended audible pre-echo is emitted.
- [ ] Zero-latency preplay performs no unnecessary early dispatch.
- [ ] Exactly sufficient lead time succeeds; insufficient immediate lead time deterministically defers or warns according to policy.
- [ ] Preplay crossing callback boundaries, sync-cycle boundaries, loop wrap, and advances greater than one loop is covered.
- [ ] Audio and MIDI preplay restore the correct first sample/event and MIDI controller/note state.

#### Play dry through wet (`PlayingDryThroughWet`)

For a dry take event with frozen capture advance `A` and current processor path `P + H`:

- [ ] The logical dry event is dispatched to the processor at target wet frame `E - (P + H + T)` and emerges wet at `E`.
- [ ] Frozen take alignment `A` selects the correct raw dry frame independently of current render advance.
- [ ] Disabling processor compensation makes wet output late by exactly `P + H`; manual and trim values shift by exactly their selected delta.
- [ ] Current processor latency changes before a new operation affect that operation; changes during an active operation latch and warn without mid-cycle retiming.
- [ ] Start from stopped, steady cycling, loop wrap, stop, restart, and multiple simultaneous loops are covered.
- [ ] Dry audio and dry MIDI through deterministic audio/MIDI processors are covered, including notes held across wrap and state cleanup on stop.
- [ ] Recorded wet channels remain silent in this mode while the current processor return is audible exactly once.

#### Record dry into wet (`RecordingDryIntoWet`)

For a dry logical event intended at wet frame `E`:

- [ ] Dry media is dispatched at `E - (P + H + T)` and deterministic wet output is written/replaced at canonical logical frame `E`.
- [ ] The wet operation snapshot records processor observation/provenance as applied during render, with zero remaining processor playback contribution.
- [ ] Subsequent ordinary wet playback emits at `E`, proving processor delay was not double compensated.
- [ ] Disabling processor compensation during the rerecord operation writes the wet event late by exactly `P + H`; later playback preserves that chosen result without inventing compensation.
- [ ] First-frame pre-render, last-frame completion, callback boundaries, loop wrap, replacement range boundaries, and repeated rerecord cycles are covered.
- [ ] Dry MIDI state, note-on/off, sustain, equal-frame order, and generated wet audio timing are covered.
- [ ] Monitoring is forced/routed according to existing mode semantics without adding a duplicate live path.
- [ ] A processor-latency change during preread/rerecord marks the operation changed and follows the latched policy.

### Cross-action invariants

- [ ] The same frozen take played by `Playing` and used as the source of `PlayingDryThroughWet` has identical logical event times before the additional current processor render advance.
- [ ] Record then play, record then dry-through-wet, grab then play, and dry-into-wet then wet play round trips preserve event identity and expected frame.
- [ ] Component totals are not double counted when JACK/provider observations already cover backend/device intervals.
- [ ] Enabling compensation while only monitoring changes no monitored frame/sample.
- [ ] Audio and MIDI state mirrors publish raw/logical/dispatch positions consistent with rendered data.
- [ ] Save/load and sample-rate conversion preserve each action’s expected timing when the same action is replayed after restoration.
- [ ] Native dummy and Wasm test backends run the same shared action matrix wherever their declared capabilities overlap.

### Test layers and ownership

- Pure domain tests verify recipe arithmetic, overlap rejection, range selection, and signs.
- Engine channel/loop tests verify raw/logical cursors and every action oracle above.
- Session graph tests verify dry processor ordering and simultaneous/co-processed loops.
- Backend tests verify prepared recipes, operation latching, state publication, and deterministic delayed providers.
- Application tests verify intents, per-operation policies, warnings, and no silent retiming.
- Persistence tests rerun timing oracles after save/load/resample rather than checking metadata fields alone.
- Provider contract tests verify JACK, Carla, and OxiSynth observations against deterministic or measured signal behavior.
- Realtime tests execute all major compensated loop actions under allocation/lock guards.

## Staged implementation

Stages are ordered. A later stage may begin only when its stated dependencies are complete or when work is isolated behind test-only types that cannot leak incomplete behavior.

### Stage 0 — Baseline characterization and design fixtures

Dependencies: none.

- [x] Add focused tests that pin current uncompensated direct/dry/wet monitoring, ordinary play, record, play-after-record, planned preplay, `PlayingDryThroughWet`, `RecordingDryIntoWet`, prerecord, grab, and replacement timing across callback boundaries.
- [x] Add the common deterministic action-matrix harness described above, including uniquely identified audio impulses/MIDI events, raw/logical/dispatch observation, and configurable `I`, `P`, `O`, `H`, `T`, `Q`, `B`, and `L`.
- [x] Add an engine-level deterministic delayed audio/MIDI source and processor fixture without exposing production settings yet.
- [x] Measure current external JACK send/return callback behavior with a small test client at at least two buffer sizes; record whether an unavoidable callback-period delay exists.
- [x] Add OxiSynth off-boundary characterization tests before changing its wrapper.
- [x] Confirm Carla 2.5.10’s available Native/internal aggregate-latency surfaces for Rack and Patchbay and capture the chosen adapter boundary in a focused design note or test helper documentation.
- [x] Inventory every serialized and wire type that carries channel timing, processor state, or backend status.
- [x] Establish explicit maximum supported compensation, retained margin, latency path count, and observation-history count from existing recording/storage bounds.

Verification:

- [x] Targeted engine/backend tests demonstrate current behavior and fail when the deterministic delay is incorrectly assumed zero.
- [x] Characterization results distinguish measured facts from intended behavior.
- [x] No production behavior changes in this stage.

### Stage 1 — Shared latency domain and pure policy resolution

Dependencies: Stage 0 bounds and terminology.

- [x] Add shared exact/range/estimated/manual/unknown observation types with checked constructors.
- [x] Add component kinds, policy modes, range selection, signed trim, source identity, and revision.
- [x] Add pure operation recipe resolution for direct/dry record, live wet record, dry-through-wet, wet rerecord, grab, and replacement.
- [x] Encode conditional cue/output semantics in the resolver rather than scattered callers.
- [x] Add checked summation and maximum enforcement.
- [x] Add take snapshot and changed/incomplete status domain types independent of UI/backend representations.
- [x] Add per-path aggregation rules and explicit ambiguity results.

Verification:

- [x] Table-driven tests cover every component toggle/mode, unknown/manual behavior, range strategy, signed trim, overflow, and output-latency condition.
- [x] Property tests or exhaustive bounded tests prove `min <= selected <= max` where automatic selection resolves.
- [x] Wasm-compatible tests pass for the shared types.

### Stage 2 — Engine and processor latency contracts

Dependencies: Stage 1.

- [x] Add latency observation to generic processor routes and the Carla processor trait.
- [x] Add a test processor that delays audio and MIDI by exact or dynamically changing frame counts while remaining allocation-free.
- [x] Add per-port latency observation storage and callback-readable revisions in engine port/state mirror types.
- [x] Add callback-latched latency recipes to channels/loops without changing playback yet.
- [x] Publish current and latched observations through state mirrors.
- [x] Ensure numeric latency updates do not invalidate/rebuild graph topology.

Verification:

- [x] Processor/port observations update atomically and latch only at defined operation boundaries.
- [x] Dynamic changes increment revisions and mark active test operations changed.
- [x] Realtime no-allocation/no-lock tests include observation reads/publication.

### Stage 3 — Raw capture windows and non-destructive playback alignment

Dependencies: Stages 1–2.

- [x] Separate media-layout offset, take capture alignment, and ephemeral render advance in audio and MIDI channel processing.
- [x] Reserve bounded retained-preroll/postroll storage before arming.
- [x] Continue recording finalization after the logical stop until required postroll is captured.
- [x] Map logical playback to the selected raw take window across callbacks and loop wraps.
- [x] Support positive, zero, and bounded negative effective alignment.
- [x] Preserve MIDI start state and events crossing retained window boundaries.
- [x] Publish logical and raw played positions independently.
- [ ] Integrate content snapshots so finalization remains an unsettled mutation while safe play-after-record can consume already available frames.
- [x] Define readiness/defer behavior when advance is greater than or equal to loop length.

Verification:

- [ ] The ordinary `Recording` followed by ordinary `Playing` rows of the automated action matrix pass for direct/dry/wet audio and supported MIDI, including every relevant component toggle and exact raw/logical frame oracle.
- [ ] The ordinary `Playing` rows pass for start/stop/restart, play-after-record, callback boundaries, loop wrap, and frozen-take stability.
- [ ] Exact delayed impulse and MIDI fixtures satisfy capture acceptance criteria at all mandatory boundary values in the common parameter set.
- [ ] End-of-take impulses/events survive positive compensation due to postroll.
- [ ] Loop wrap repeats the selected logical window, not raw startup material.
- [ ] Play-after-record is gapless when data readiness permits and defers visibly otherwise.
- [ ] Saving/editing during finalization follows the existing wait/retry/reject contract.
- [ ] No callback allocations or storage growth occur after arming.

### Stage 4 — Dry render-ahead and dry/wet mode semantics

Dependencies: Stage 3 and processor fixture from Stage 2.

- [ ] Add cyclic dry source render-ahead independent of media `start_offset`.
- [ ] Start processor input early for planned `PlayingDryThroughWet` transitions.
- [ ] Implement the configured defer/warn behavior for immediate transitions lacking pre-render time.
- [ ] Compose take capture alignment with current processor render advance.
- [ ] Restore MIDI state early enough for processor output to be valid at the target boundary.
- [ ] Handle note-off/sustain cleanup when stopping or changing render advance.
- [ ] Implement `RecordingDryIntoWet` canonical wet writes and `applied_during_render` provenance.
- [ ] Continue bounded processor work around transition boundaries for declared latency and separately named warm-up/tail policy.
- [ ] Ensure live monitoring still uses the uncompensated current input path while dry-loop rendering uses lookahead.

Verification:

- [ ] Every planned preplay row in the automated action matrix passes, including separate media-lead-in and processor-render-ahead assertions.
- [ ] Every `PlayingDryThroughWet` row passes for audio and MIDI, component toggles/manual/trim, start, steady loop, wrap, stop, restart, and dynamic-latency latching.
- [ ] Every `RecordingDryIntoWet` row passes, including canonical wet writes, first/last frames, replacement boundaries, and subsequent ordinary wet playback proving no double compensation.
- [ ] Exact `P`-frame processor fixtures emerge on target frames at start, steady loop, wrap, stop, and restart.
- [ ] `RecordingDryIntoWet` round-trips without double applying `P`.
- [ ] MIDI state and note cleanup tests pass at loop boundaries.
- [ ] Advances larger than one callback and one loop are either correctly mapped or rejected/deferred according to the documented bound.
- [ ] Monitoring equivalence acceptance test remains green.

### Stage 5 — Grab and replacement semantics

Dependencies: Stage 3; Stage 4 for wet replacement.

- [ ] Add bounded latency observation history aligned with input ringbuffer frame history.
- [ ] Latch stable grab observations and mark multi-revision grabs variable.
- [ ] Add bounded alignment-region metadata or a non-realtime consolidation prerequisite for incompatible replacement observations.
- [ ] Stage replacement raw material and margins before committing logical writes.
- [ ] Preserve undo/content snapshot generation behavior.
- [ ] Cover direct, dry, wet, audio, and MIDI replacement where each is currently supported.
- [ ] Reject unsupported mixed-policy operations before mutating content.

Verification:

- [ ] Every grab row in the automated action matrix passes for stable history, revision-spanning history, ring/callback/loop wrap, component policy variants, and supported audio/MIDI channel roles.
- [ ] Stable-history grab aligns exactly.
- [ ] Variable-history grab reports and persists a warning.
- [ ] Replacement with the same and different observations has deterministic playback and undo behavior.
- [ ] Failed consolidation/replacement leaves prior content and provenance intact.

### Stage 6 — Backend/application API and operation policy integration

Dependencies: Stages 1–5 engine semantics.

- [ ] Extend backend snapshots with per-port, per-processor-path, and backend-buffering observations.
- [ ] Add backend commands for track/operation latency policy, cue selection, take policy update, and optional consolidation/bake.
- [ ] Extend backend session capture/restore types with take snapshots and alignment regions.
- [ ] Resolve policy before arming and transfer a bounded prepared recipe to the callback.
- [ ] Update native, dummy/test, and worklet client backend implementations without defaulting unsupported observations to zero.
- [ ] Add application model state and intents for editing policies and inspecting take provenance.
- [ ] Ensure optimistic UI state reconciles with authoritative callback-latched state and reports mutation failures.

Verification:

- [ ] Backend contract tests cover unsupported, pending, accepted, latched, changed, and failed policy updates.
- [ ] Application tests prove settings affect future operations but not existing take observations.
- [ ] Driver/processor changes during an operation produce warnings without silently retiming it.

### Stage 7 — JACK observation and latency propagation

Dependencies: Stages 2 and 6.

- [ ] Implement the narrowly scoped JACK latency callback integration.
- [ ] Build and atomically publish fixed-capacity callback route snapshots for live internal paths.
- [ ] Query application input capture and output playback ranges.
- [ ] Propagate internal direct/monitor/processor min/max latency in both JACK callback modes.
- [ ] Safely retire callback-visible port handles during dynamic track/port removal.
- [ ] React to graph reorder, connection, sample-rate, buffer-size, and processor-latency revisions.
- [ ] Publish observations to backend state without callback logging/locking.
- [ ] Include verified external send/return callback buffering.

Verification:

- [ ] Unit tests cover range aggregation and route filtering.
- [ ] JACK integration tests with deterministic source/sink/processor clients observe expected capture/playback totals.
- [ ] Port add/remove stress does not expose stale handles or deadlock.
- [ ] JACK latency callback path passes realtime allocation/lock checks.
- [ ] Physical loopback test procedure and expected tolerance are documented and executed where JACK hardware is available.

### Stage 8 — Carla latency provider

Dependencies: Stages 2, 6, and Stage 0 Carla evidence.

- [ ] Implement the version-gated Carla aggregate latency adapter for the pinned runtime.
- [ ] Represent Rack and Patchbay/Patchbay16x path semantics separately.
- [ ] Return range/unknown for feedback or unsupported graph cases.
- [ ] Publish latency and revision through in-process Carla control/realtime endpoints.
- [ ] Extend subprocess shared/control protocol and worker status; bump protocol version and update validation/fixtures.
- [ ] Refresh after all graph/state/parameter/buffer lifecycle events that can change latency.
- [ ] Preserve manual operation on unsupported Carla runtime versions.
- [ ] Add diagnostics identifying Carla-derived versus manual values.

Verification:

- [ ] Fake Carla dynamic-latency tests cover both hosting modes.
- [ ] Real Carla tests load known zero- and nonzero-latency plugins in Rack and branched Patchbay arrangements and compare queried path totals with impulse output.
- [ ] Worker restart preserves/re-publishes latency revision and does not apply stale generation data.
- [ ] Unsupported runtime test reports unknown/manual while Carla audio remains usable.

### Stage 9 — OxiSynth timing provider or correction

Dependencies: Stage 0 characterization and Stage 4 MIDI render-ahead.

- [ ] Decide from tests whether to patch/fork for sample-accurate event application or publish a phase-dependent range.
- [ ] If patched, add dependency/source provenance and prove exact behavior at every characterized offset.
- [ ] If ranged, expose selection/trim and residual uncertainty through normal processor policy.
- [ ] Keep musical attack/reverb/chorus behavior out of algorithmic latency.
- [ ] Apply the same semantics in native and Wasm worklet builds.

Verification:

- [ ] Offset matrix tests pass across consecutive odd callback sizes.
- [ ] No fixed 64-frame claim remains without exact proof.
- [ ] OxiSynth dry MIDI through wet and wet recording tests report/apply the declared semantics consistently on native and Wasm.

### Stage 10 — CPAL, browser, and protocol capability completion

Dependencies: Stage 6; Stage 9 for browser synth.

- [ ] Publish truthful CPAL/midir manual/estimated capabilities.
- [ ] Add browser AudioContext latency observations where supported and unknown states where absent.
- [ ] Extend audio worker/worklet protocol messages with bounded latency policy, observations, take state, and errors.
- [ ] Bump protocol versions and update raw host fixtures, worker fixtures, capacity validation, and stale-generation handling.
- [ ] Preserve Web MIDI coarse-timing documentation and behavior.
- [ ] Ensure browser permission/device loss changes current observations without moving frozen takes.

Verification:

- [ ] Shared Wasm tests cover policy resolution, protocol roundtrip, unknown/manual capability, and built-in synth compensation.
- [ ] Browser fixture tests cover missing `baseLatency`/`outputLatency`, device restart, and worklet continuation.
- [ ] Message sizes remain under protocol maxima and overflow is explicit.

### Stage 11 — Session, exact media, resampling, and I/O

Dependencies: Stages 5–10 define all persisted state.

- [ ] Bump session document version and add validated latency documents.
- [ ] Update session archive encode/decode, media index, resource limits, deterministic ordering, and transactional replacement.
- [ ] Update exact Shoop audio/MIDI formats or metadata envelopes to preserve raw/logical identity.
- [ ] Implement deterministic resampling of observations, trims, margins, and alignment regions.
- [ ] Make standard export render the logical compensated view and add explicit raw export.
- [ ] Define import defaults and optional manual import offset.
- [ ] Update duplicate/clone/composite/session-switch flows to preserve or intentionally reset provenance.
- [ ] Ensure generated click tracks and source-only imports receive explicit no-capture provenance.
- [ ] Update all session fixtures and format documentation.

Verification:

- [ ] Same-rate save/load gives exact raw bytes/events and identical compensated output.
- [ ] Cross-rate save/load/driver switch follows documented rounding and retains valid ranges.
- [ ] Malformed, overflowing, inconsistent, and unsupported latency metadata fails before mutation.
- [ ] Logical and raw exports have expected lengths/timestamps and do not mutate the loop.
- [ ] Clone/duplicate/composite tests preserve timing identity.

### Stage 12 — Settings and user interface

Dependencies: Stages 6 and 11.

- [ ] Register machine defaults and add settings migration/documentation.
- [ ] Add track/loop latency panel and component editors.
- [ ] Add cue-output selection integrated with normalized application/host port identities.
- [ ] Display exact/range/estimated/manual/unknown and selected/applied totals in frames and milliseconds.
- [ ] Add take snapshot/current observation comparison and changed/incomplete warnings.
- [ ] Add logical/raw waveform and MIDI view markers.
- [ ] Add operation deferral/finalization status and actionable errors.
- [ ] Add explicit consolidate/bake and raw export confirmation where implemented.
- [ ] Ensure touch mode and keyboard navigation can operate all controls without hover-only behavior.

Verification:

- [ ] UI unit tests cover component toggles, mode changes, signed edits, range selection, cue selection, warnings, and no-backend states.
- [ ] Snapshot-driven tests prove UI displays authoritative latched values after optimistic edits reconcile.
- [ ] Settings save/cancel/reset/migration behavior follows existing contracts.
- [ ] Manual usability pass covers direct, External, Carla, and Built-in Synth tracks.

### Stage 13 — Diagnostics, tracing, and operational hardening

Dependencies: all runtime provider stages.

- [ ] Add bounded counters for unresolved recipes, observation changes, insufficient margins, deferred transitions, finalization overruns, path ambiguity, and provider failures.
- [ ] Add realtime-safe plots for applied capture/render advance and active postroll.
- [ ] Add non-realtime diagnostic summaries to backend/application snapshots.
- [ ] Update tracing inventory and ensure every added event/span is classified.
- [ ] Add stress tests for rapid policy edits, graph churn, processor latency changes, driver switches, loop transitions, and session saves.
- [ ] Confirm all arithmetic and capacities under maximum supported latency/channel/loop counts.

Verification:

- [ ] Diagnostics identify failure source and remediation without realtime logs.
- [ ] Stress tests show bounded memory and stable callback work.
- [ ] Tracing coverage check is closed.

### Stage 14 — Documentation and final end-to-end validation

Dependencies: all prior stages.

- [ ] Update user documentation with component definitions, sign convention, cue/output example, range behavior, backend capabilities, dry/wet modes, and troubleshooting.
- [ ] Update session, settings, port model, Web MIDI, worklet, Carla runtime, and run documentation where contracts changed.
- [ ] Document JACK physical loopback and Carla known-latency validation procedures.
- [ ] Document OxiSynth’s validated behavior and any dependency fork/patch.
- [ ] Re-run every immutable acceptance scenario with deterministic fixtures.
- [ ] Run the complete automated loop-action matrix for ordinary play, record, grab, planned preplay, `PlayingDryThroughWet`, and `RecordingDryIntoWet`; retain direct frame-oracle evidence for each required row.
- [ ] Run targeted physical JACK and real Carla scenarios where facilities are available; record skips with reasons where not.
- [ ] Run complete project gates in the selected development environment.

Final verification commands and surfaces:

- [ ] `cargo fmt --all -- --check`
- [ ] `RUSTFLAGS="-D warnings" cargo build --workspace`
- [ ] `SHOOP_ALLOW_MISSING_BACKENDS=1 cargo nextest run --workspace --features shoop_engine/app_backend --profile ci`
- [ ] `python3 scripts/check_shoop_test_usage.py` after adding/changing Rust tests.
- [ ] `python3 scripts/check_tracing_coverage.py --require-closed`
- [ ] Build `shoopdaloop` and `shoop_audio_worklet` for `wasm32-unknown-unknown`.
- [ ] Run the shared Wasm test suite and the browser smoke commands documented by the application crate when browsers are available.
- [ ] Run JACK tests without `SHOOP_ALLOW_MISSING_BACKENDS=1` where a real JACK server/provider is required.
- [ ] Run real Carla tests with the required Carla test environment and both in-process/subprocess hosting modes.
- [ ] Verify a manual end-to-end matrix at 44.1 kHz and 48 kHz, at two callback sizes, covering:
  - direct audio record/playback;
  - direct/dry MIDI record/playback;
  - live wet record/playback;
  - External dry-through-wet and wet rerecord;
  - Carla Rack and branched Patchbay;
  - Built-in Synth on native and browser;
  - cue/output enabled and disabled;
  - automatic, disabled, manual, and automatic-plus-trim policies;
  - graph/latency change during recording;
  - save/load and sample-rate-changing driver switch;
  - logical and raw export;
  - monitoring latency/equivalence.
- [ ] Confirm `git diff --check` and review the final diff for unrelated formatting or behavior changes.
- [ ] Confirm all goals and immutable acceptance criteria have direct test, documentation, or physical-verification evidence.

## Expected primary implementation surfaces

This list guides investigation and ownership; implementation should follow actual dependency boundaries discovered during each stage.

- Engine timing/content: `src/rust/shoop_engine/src/channel_mode.rs`, `audio_channel.rs`, `midi_channel.rs`, `audio_midi_loop.rs`, `basic_loop.rs`, `session.rs`, `graph_build.rs`, port/state mirror modules, and engine tests.
- Native driver and processors: `src/rust/shoop_engine/src/app_backend.rs`, `carla_processor.rs`, `carla_native.rs`, `carla_subprocess.rs`, `carla_shared_memory.rs`, `oxisynth.rs`.
- Backend abstraction/runtime: `src/rust/shoop_backend/src/lib.rs`, `native.rs`, backend tests and fixtures.
- Application state/control: `src/rust/shoop_app_api/src/lib.rs`, `src/rust/shoop_app/src/lib.rs`.
- UI/settings: `src/rust/shoop_egui`, `src/rust/shoop_settings`, `src/rust/shoopdaloop/src/settings.rs`.
- Browser/worklet transport: `src/rust/shoop_audio_protocol`, `shoop_audio_worklet`, `shoop_worklet_client`, and browser host fixtures.
- Carla subprocess protocol: `src/rust/shoop_plugin_protocol` and worker entry/runtime tests.
- Persistence/media: `src/rust/shoop_session/src/document.rs`, `archive.rs`, `resample.rs`, media codecs, and format fixtures.
- Documentation: `docs/session_format_v1.md`, `docs/settings_format_v1.md`, `docs/port_model.md`, `docs/web_midi_contract.md`, application README, and any focused latency contract added during implementation.

## Completion definition

The feature is complete only when every stage is checked, all immutable acceptance criteria have evidence, automatic providers are truthful on supported runtimes, unsupported providers remain visibly manual/unknown, monitoring remains undelayed, persisted takes reproduce their logical timing, and the final validation stage passes or records environment-dependent physical checks with explicit, user-accepted exceptions.
