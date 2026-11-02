# Prepared audio operations

`audio::AudioRenderer` and `audio::AudioCapture` are low-level execution operations. They do not own loop mode, musical position, wrap occurrence, resource leases, or capture lifecycle authority. The resolver/planner supplies those decisions. The [loop planner](runtime.md) binds them to distinct playback/capture consumers; use [coordinated_loop](../examples/coordinated_loop.rs) for stereo/MIDI record → playback → FX → capture and export.

## Preparation and playback

Prepare a renderer with fixed channel count, maximum interval length, and `Smoothing`. All byte-conversion scratch and per-channel last-output/ramp anchors are allocated and pre-touched during preparation. Dimension arithmetic, including the aggregate scratch size, is checked against the common native/Wasm ceiling.

For each interval, pass `Playback { source, position, gain, balance, discontinuity }` plus planar output slices, or call `render_ports` with the runtime's `AudioPorts`. Both paths share conversion/DSP logic. The port path writes only each output port's current interval window; it does not allocate a temporary channel-reference vector or retain callback borrows. `source` is a scoped `ReadIndex` over an existing lease; it does not acquire another lease. `None` writes immediate silence and resets smoothing. All output channels must have the same length within the prepared maximum. A zero-frame call does not change output or DSP state. Nonempty calls require finite gain and a balance in [-1, 1]; negative gain is supported. Balance scales the left or right channel of stereo output and leaves other layouts unscaled. Audio sample values themselves are not sanitized.

The operation obtains the pinned snapshot's actual metadata through `RenderIo::snapshot`, not caller-supplied metadata or the resource's newer active version. It validates channels and the complete half-open source range, then performs indexed positioning and sequential reads into prepared byte scratch. Little-endian interleaved `f32` becomes planar output. Removed/replaced resources remain playable through retained leases; stale or foreign indexes are refused.

An interval must not cross a musical wrap. The resolver cuts at the wrap and supplies the new position and discontinuity flag; the renderer has no parallel playhead. `discontinuity` is required for start/retrigger/seek/wrap/replacement, not for an extra host callback cut. Use the resolver's authoritative `Decision::playback_discontinuity`, not its potentially exhausted diagnostic journal. The flag is transactional, includes successful same-position seeks/replacements even when visible state is unchanged, and is false when the final state has no playback. Idempotent Play and continuing monitored playback do not restart DSP; natural wraps and starting monitored playback do. The test driver uses these flags with journal capacities 0, 1 and 1024, as do the planner's immutable decision consumers. Gain is applied to each raw sample. `None` and `LinearRestart { frames: 0 }` are unsmoothed. For an N-frame restart, each channel retains its last actual output as an anchor, and sample i uses `anchor * (1 - (i+1)/N) + raw_gained_sample * ((i+1)/N)`. The ramp continues across host cuts; a new discontinuity restarts it from the last actual output. Stop has no release tail. `set_smoothing` changes the length for later restarts, including the next silence reset; a ramp in progress keeps its length. It needs no further storage.

Refused operations leave output and DSP state unchanged; scratch and resource diagnostic counters may have changed. This is not permission to replay a failed `GraphRuntime` callback: the runtime's frame/fault contract still applies. Necessary I/O does not consume or refresh background transfer allowance. Runtime resource-copy counters cover storage access, not every DSP/conversion instruction or a callback deadline.

## Capture and distinct revisions

Prepare `AudioCapture` with channel count and maximum interval size. `stage` converts planar input slices into its fixed interleaved sample scratch and stages one member of an already reserved capture interval. `stage_ports` reads the input windows of `AudioPorts` through the same conversion path, ignoring output ports and retaining no input borrows. It cannot activate/freeze a take or commit the group. Invalid port shapes, interval sizes, stale tokens and duplicate contributions fail without publishing a partial prefix. The planner commits only after all members are staged.

Playback and capture use distinct operations and can refer to different revisions of the same logical resource: read a retained immutable take, apply gain/FX, and stage the resulting samples into a fresh reserved take. A frozen take is immediately indexed/readable without an asynchronous build. The tests verify these operations directly, including old-content preservation. The planner supplies separate signal nodes and coordinated group lifecycle; its public example also checks the retained old export. The [configuration example](../examples/configuration_cut.rs) changes the prepared capture route while preserving old content.

## Shared history and editable takes

`rt_runtime::resource::shared_audio::AudioStore` supplies interleaved history and
independently editable takes. `AudioCapture::append_shared` and
`append_shared_ports` append a common multichannel interval through `RenderIo`.
They accept partial pages and arbitrary intervals within the prepared maximum.
Required audio copies are counted separately from background transfer allowance.

`grab` owns a history window, `fork` owns another editable view, and `export` owns
an immutable revision. A copied read index borrows its owner and becomes invalid
when that owner is released. `replace` publishes one multichannel edit atomically,
including page and loop boundary crossings, without changing the take's length.
Other takes and exports retain their original pages. Musical decisions and the
effective cut remain the caller's responsibility.

`SharedPlayback`, `render_shared` and `render_shared_ports` use the same
conversion, gain, balance and smoothing path as ordinary resource playback.
The resolver still supplies non-wrapping intervals and discontinuity flags.
See the [shared audio ownership reference](../../rt_runtime/docs/reference/shared-audio.md)
for preparation, capacity and release contracts.

`tests/shared_audio.rs` manually composes the production resolver, audio capture
and audio renderer. An independent scalar oracle checks two overlapping stereo
grabs, replacement across pages/wraps, retained original and intermediate exports,
stop/resume and continued history capture with background transfers exhausted.
Whole, single-frame and irregular callback partitions produce identical samples.
A separate run refuses an export at its owner limit and preserves running content.
All execution and cleanup are heap guarded.

This is an audio-resource composition proof. `LoopPlanner` admission/media-ledger
integration, replacing mode, MIDI, host protocols and application P04/P05 capability
restoration remain separate work. Native tests do not establish browser/device
deadlines or a new cross-platform acceptance campaign.

## Verification boundary

The six audio tests use the production resolver with an independent scalar sample/ramp oracle over three host partitions, plus mono/stereo/five-channel page crossings, 128 one-sample intervals, immediate freeze/read, separate old-playback/gain/new-capture operations, refusal/zero-frame/stale-scope cases and complete resource recovery. All operations run under heap guards, including failure paths; the full workloads are included in pinned Miri and package Node/Chromium runs.

These are foundation tests with their original evidence scope, not a new documentation-time campaign. MIDI emission/restoration, coordinated adapters and coherent mixed-media export are implemented separately. Neither these tests nor the offline examples establish an independent loop AudioWorklet, new performance evidence or hard deadlines; no such host/benchmark is supplied in this snapshot.
