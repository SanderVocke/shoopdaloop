# Loop features assessment

[Plan](PLAN.md) · [Architecture](ARCHITECTURE.md) · [Validation](VALIDATION.md)

Assessed on 2026-10-30 at `900f7831`. This compares the looping features of the
current application (legacy engine `src/rust/shoop_engine`, backend
`src/rust/shoop_backend`, application `src/rust/shoop_app`, session format
`src/rust/shoop_session`) with the portable loop library `src/rust/shoop_loops`,
and estimates the feasibility and effort of closing the gaps. It is an analysis,
not an approved plan: the designs are proposals and the open decisions are listed
at the end.

Effort is for one experienced developer and includes this project's usual
validation: native debug/release tests with heap guards, Node Wasm tests, docs and,
where render or ownership paths change, host fixture coverage in `tests/loops_host`
and Miri inventory cases. **S** ≤ 3 days, **M** ≈ 1–2 weeks, **L** ≈ 2–4 weeks,
**XL** > 1 month.

## Summary

All eight gaps are feasible within the loop library's callback contract: no
allocation, freeing, locking or waiting on render paths, finite prepared capacities.
Full parity is roughly 5–6 months of focused work; a useful first version of each
feature is roughly 2 months. Most of the cost sits in four shared foundations.

| Feature | Full version | First version | Main risk |
| --- | --- | --- | --- |
| Per-channel gain and stereo balance | S (+2–3 d for ramped gain changes) | Itself | None |
| Start offset and take trimming | M (~2 wk) | Offset/length windows over existing takes, no pre-play (~1.5 wk) | Edits must publish as new media versions |
| Pre-play | L (3–4 wk) | Deterministic starts only | Lead-in sounds before a start that may still be cancelled |
| Retroactive grab | L (~3 wk, given playback windows) | One-cycle grab from a per-loop history (M) | Cost of always-on capture |
| Recordings longer than prepared storage | L (2–3 wk) | Tests, docs and low-storage reporting (S) | No off-render allocator in a browser AudioWorklet |
| Recording latency compensation | L (3–4 wk, given history and windows) | Positive offsets only (M) | Playing a take whose tail is still being captured |
| Replace (punch-in) | L, risk XL | One-cycle replace without splicing (M) | Spliced media change the one-snapshot media model |
| Dry/wet loops | L (~3 wk, without latency) | Two application-driven loops plus the one-cycle re-record (S–M) | Choosing the shape |
| Modes inside composites | M (after replace and dry/wet) | Per-loop default playback mode only (S) | Depends on the modes existing |

### Already covered, not gaps

- Sync loop, synchronized versus immediate actions, and any loop as a sync source
  (`Timing::AfterTicks { source, skip }`), which covers targeting.
- Fixed-length recording with auto-play: `Record`, then `Play` scheduled a number of
  sync pulses later. `Record` itself has no duration
  (`src/rust/shoop_loops/src/resolver.rs:1322-1332`).
- Silent sync and trigger loops: timing-only media can have a length without audio
  or MIDI (`src/rust/shoop_loops/docs/media.md:27`).
- Composites with delays, repetitions, nesting and iteration seeks; MIDI state
  restoration; boundary smoothing; exports and reads for waveform/detail views;
  content import and selection.
- Application policy that stays outside the loop library: selection, solo, auto-arm,
  click-track generation, file formats and resampling, MIDI control, scripting, undo.

## Shared foundations

| Foundation | Needed by | Effort |
| --- | --- | --- |
| Playback window: render a logical window (start, length, lead-in) over immutable raw media | Start offset and trimming, realignment, grab, latency compensation, pre-play | M (~2 wk) |
| Input history: a bounded, always-on recent-input take per capture source | Grab, negative latency offsets | Part of grab |
| Fixed-length, cycle-aligned capture: record exactly one loop length from a wrap, then publish it | One-cycle replace, dry-into-wet re-record | M (~1–1.5 wk) |
| Segmented media versions: a version assembled from parts of other immutable versions | Full punch-in replace | Most of replace's cost |

Today `shoop_loops` has none of these:

- A loop plays its whole media from position zero. The planner requires media frames
  to equal the resolver's `content_length`
  (`src/rust/shoop_loops/src/runtime/planner.rs:392-396`), the ledger requires a
  description's frames to equal its snapshot's (`src/rust/shoop_loops/src/media.rs:336-340`)
  and the audio renderer uses the logical position as the raw read offset
  (`src/rust/shoop_loops/src/audio.rs:196-206`; MIDI likewise in `src/midi.rs`).
- Capture is prospective and append-only: storage or copy-budget exhaustion stops
  capture with an explicit incomplete prefix, never overwrite
  (`src/rust/rt_runtime/src/resource/capture.rs:244-245`).
- A media version is exactly one audio snapshot plus one MIDI snapshot
  (`src/rust/shoop_loops/src/media.rs:58-68`).

## Features

### Per-channel gain and stereo balance

**Legacy.** Each channel has its own gain. The backend applies loop gain times a
linear side factor for exactly two channels (`src/rust/shoop_backend/src/lib.rs:5564-5571`,
`5615-5634`); gain is clamped to [0, 1] and balance to [-1, 1] (`lib.rs:6855`, `6864`).
The legacy smoother ramps gain changes.

**Today.** One `Operation::Gain { target, value }` per loop
(`src/rust/shoop_loops/src/runtime/mod.rs`), applied on delivery and multiplied into
every channel (`src/rust/shoop_loops/src/audio.rs:233`). Smoothing restarts only on
discontinuities, so gain changes step.

**Design.** Add a balance value next to gain in the binding, view and cut, set by an
extended `Gain` or a new `Balance` operation; the renderer applies the legacy stereo
factors. Optionally add a prepared per-channel ramp. An application-owned gain node
is an alternative that needs no library change.

**Effort.** S; a gain ramp adds 2–3 days.

### Start offset and take trimming

**Legacy.** Per channel, raw position = logical position + start offset + capture
alignment (`src/rust/shoop_latency/src/lib.rs:174-184`,
`src/rust/shoop_engine/src/audio_channel.rs:598-605`); positions outside the raw media
play silence. The details pane edits offset, pre-play and length for the whole loop,
and the backend applies one common offset to every channel
(`src/rust/shoop_backend/src/lib.rs:7397-7424`). Edits are refused while recording
is active or armed. MIDI length edits truncate storage, which is destructive
(`src/rust/shoop_engine/src/midi_channel.rs:668-695`). The session format stores
`start_offset_frames` and `preplay_frames` per channel
(`src/rust/shoop_session/src/document.rs:291-294`).

**Design.** Put the window in immutable media metadata: the description gains a raw
window (start, frames, lead-in) and validation becomes "window inside the raw
snapshot". The resolver sees only the window's length as `content_length`, so wraps,
cycles, sync and composites are unchanged. The planner maps logical to raw positions
when building the render view; the renderers already accept any raw offset, and the
MIDI index can reconstruct state at any frame. An edit creates a new ledger row that
shares the original take's leases and is published through the existing `Select` at
a settled boundary; old windows stay retained for export and undo. Length changes
reach sync-dependent loops at that boundary, as `Select` does today. Composite
durations derived from a child's length need re-staging by the application.

**Effort.** M (~2 weeks): core 4–5 days, tests 3–4, host and Miri 1–2, docs 1.

### Pre-play

**Legacy.** Only a stopped loop whose next planned transition is to Playing, one
trigger away with a known ETA, pre-plays (`src/rust/shoop_engine/src/channel_mode.rs:104-118`).
The ETA is a prediction from the loop's and its sync source's positions
(`src/rust/shoop_engine/src/basic_loop.rs:165-177`). There is no pre-play on wraps,
seeks or retriggers. MIDI events before the window are folded into a restore state;
pickup notes keep sounding into the loop start
(`src/rust/shoop_engine/src/midi_channel.rs:1310-1368`).

**Design.** A committed lead-in decision, not render look-ahead. The resolver cuts an
interval at F − P when a stopped primitive has a pending start whose frame F follows
from settled state: an admitted `Timing::At` request, or an `AfterTicks` request
with no skips left and a playing sync chain. The lead-in renders raw [start − P,
start) without pulses or cycles. If a later operation invalidates the prediction, the
lead-in ends at that cut with normal cleanup. At F the renderers must treat the start
as contiguous, otherwise MIDI cleanup would cut and re-strike held pickup notes.

**Risk.** It is the first committed decision driven by a prediction.

**Effort.** L (3–4 weeks). Pre-play on freshly recorded takes needs material before
the logical start, from latency retention or the input history.

### Retroactive grab

**Legacy.** A per-port rolling ring of recent input (`src/rust/shoop_engine/src/buffer_queue.rs:35-138`),
30 seconds by default (`INPUT_CAPTURE_CAPACITY_SECONDS`,
`src/rust/shoop_backend/src/lib.rs:1480`). The application derives the window from the
sync length, the targeted loop's cycles and position, and the record-cycle value
(`src/rust/shoop_app/src/lib.rs:8229-8327`). Short history is silently shortened,
audio is copied into prepared storage, MIDI is copied without an initial state, and
grab is refused when the recording offset is nonzero
(`src/rust/shoop_backend/src/lib.rs:5573-5609`, `6875-6884`, `6905-6946`).

**Today.** Group takes with exact freeze, ledger reserve and publish
(`src/rust/shoop_loops/src/runtime/planner.rs:128-167`, `306-340`), resolver content
replacement, checkpointed MIDI seek, and capture nodes that run and track MIDI input
every interval (`src/rust/shoop_loops/src/runtime/processor.rs:162-194`).

**Design.** One planner-owned input history per capture source: an always-on group take
rotated before it exceeds a bound. A synchronized grab freezes the history and selects
the last cycles before the boundary; an immediate grab relabels the running history
take as the loop's recording and keeps going. The resolver owns the window arithmetic
through a new grab transition family; preflight checks history coverage and refuses
the whole family rather than padding. The MIDI initial state comes from the projection
at the history's start. Stopped extraction and cleanup must freeze and release history
takes. Bounding options:

| Option | Cost | `rt_runtime` change |
| --- | --- | --- |
| Two overlapping ordinary takes | About twice the capture copies and up to twice the storage | None |
| Chain of segments | Single copy | Multi-source media; renderers read across segments |
| Head-trimmed capture version | Single copy | Page-index deletion, MIDI checkpoint ring, leases with a base offset |

**Risks.** Always-on capture cost; rotation retires pages every period and the
budgeted reclaim can fall behind.

**Effort.** L (~3 weeks given playback windows); XL with head trimming. First version
(M): per-loop history with two extra banks rotated on the loop's sync edges; a
one-cycle synchronized grab adopts the take frozen at the last edge, and a one-cycle
immediate grab relabels the take running since that edge. It needs no playback
windows and no `rt_runtime` change.

### Recordings longer than prepared storage

**Legacy.** The physical backend caps each channel at 120 seconds
(`RECORDING_CAPACITY_SECONDS`, `src/rust/shoop_backend/src/lib.rs:1479`). Other port
models grow chunked storage by allocating on the audio thread
(`src/rust/shoop_engine/src/chunked_samples.rs:7-11`).

**Today.** There is no per-take limit: a take can use every free page of the store,
and pages released elsewhere are recycled during callbacks. The store is one fixed
allocation (`src/rust/rt_runtime/src/resource/mod.rs:212`) with a ceiling of
`i32::MAX` bytes on native and Wasm (`src/rust/rt_runtime/src/config.rs:90-91`).
Exhaustion already yields a truthful incomplete take.

**Design.** Split the store's backing into segments, with capacity for the maximum
page count prepared up front. The control side allocates a segment and sends it as an
owning resource command; the render side only takes ownership and pushes page indices.
Fully free segments return through the owning-return path and are freed off-render.
Page addresses become segment-relative. A low-water signal tells the application to
add storage. Preflight is unchanged: late replenishment ends the take truthfully.

**Risks.** A non-isolated AudioWorklet has no allocator off the audio thread, and the
architecture forbids allocating owners there, so the browser needs a preallocated
maximum or an explicit allowance to allocate between render quanta. One long take can
starve other loops unless takes get page budgets. MIDI checkpoint rows are fixed per
version.

**Effort.** L (2–3 weeks); about a week more each for shrinking and for MIDI
checkpoint replenishment. First version (S): tests and docs showing one take using the
whole pool and recycling released media mid-take, plus low-water reporting.

### Recording latency compensation

**Legacy.** A signed recording offset is latched when recording starts
(`src/rust/shoop_engine/src/audio_channel.rs:420-444`). Retention covers the logical
length plus margins before and after (`audio_channel.rs:555-590`). Insufficient
preroll aborts the take after it starts; postroll keeps recording after the record
mode ends; playback and data reads wait while the take is unsettled. Wet channels use
the recording offset plus the processor latency
(`src/rust/shoop_engine/src/latency_runtime.rs:42-56`). The offset arithmetic lives in
`src/rust/shoop_latency`.

**Design.** A per-row offset, changeable only while not armed and latched at
activation; the take's metadata records its retained margins. Negative offsets relabel
an input-history take that started early enough; preflight refuses the start if
coverage is short, instead of aborting after it. Positive offsets keep capturing after
the logical end until the offset has passed, with the take marked as settling; a
per-take page budget reserved at the end guarantees the postroll. Realignment of a
completed take is a playback-window edit.

**Open questions.** Whether a settling take may play (this needs a new read of the
active take's committed prefix) or must wait. A capture group holds at most one audio
and one MIDI member with interleaved channels, so separate dry and wet offsets need
separate groups or loops.

**Effort.** L (3–4 weeks given history and playback windows); add M if the history is
built here. First version (M): positive offsets only, with settling state, page budget
and exact freeze; playback, export and new recordings wait until settled.

### Replace (punch-in)

**Legacy.** `Replacing` is a playing mode that keeps the position and length
(`src/rust/shoop_engine/src/loop_mode.rs:20-28`) and continues across wraps until the
next transition. It sets only the replace flag, so the old content is silent and only
live monitoring is heard (`src/rust/shoop_engine/src/channel_mode.rs:78-80`). Audio is
overwritten in place and the old content is destroyed
(`src/rust/shoop_engine/src/audio_channel.rs:1131-1186`); MIDI is spliced block by
block (`src/rust/shoop_engine/src/midi_channel.rs:1059-1135`). It is refused at
nonzero recording offsets and is reachable only from script composites and Lua.

**Today.** Only stopped, playing and recording modes (`src/rust/shoop_loops/src/program.rs:7-13`);
`Record { monitor_previous }` keeps the old content audible while a new take records.
The existing `Transition::Replace` swaps in a prepared content version and always
marks a playback discontinuity; it is not a punch-in.

**Design.** A resolver mode in which the playhead keeps advancing over the old length
while an ordinary append-only take records. When replacing ends, the new version is a
splice of old, new and old content (at most three segments, or a rotation of the take
after a full cycle). It must be playable from the end frame onward, so it needs
segmented media versions in the ledger and render path; MIDI segment edges get the
cleanup and restore of a seek, and later compaction needs a content-equivalent swap
without a discontinuity. Stop publishes the splice; exhaustion publishes the truthful
prefix as incomplete; cancellation or a fault keeps the old version.

**Risk.** Changing the one-snapshot media model ripples into exports, peaks and
persistence.

**Effort.** L (3–4 weeks), with a risk of XL. First version (M, ~1–1.5 weeks): replace
exactly one cycle — record a fixed-length take from a wrap and publish it as the new
version of the same length — which needs no splicing.

### Dry/wet loops

**Legacy.** One loop whose channels have roles (`src/rust/shoop_backend/src/lib.rs:3984-4002`):
dry channels record the dry input and play into the processor input; wet channels
record the processor output and play to the wet output. Modes
(`src/rust/shoop_engine/src/channel_mode.rs:62-84`):

| Mode | Dry channels | Wet channels | Heard |
| --- | --- | --- | --- |
| Recording | Record | Record | — |
| Playing | Silent | Play | Wet recording |
| Playing dry through wet | Play into the processor | Silent | Live processor output |
| Recording dry into wet | Play into the processor | Replace from the processor output | Live processor output |

In both dry modes the dry playback runs early by the processor latency. The one-cycle
re-record is application policy (`rerecord_targets`,
`src/rust/shoop_app/src/lib.rs:8124-8170`), and the default playback mode is a per-track
setting (`src/rust/shoop_engine/src/default_playback_mode.rs`).

**Today.** One playback node and one capture node per loop, without channel roles.
Routes from playback through effects to capture already work in the service
(`src/rust/shoop_engine_service/src/preparation.rs:280-345`); its `CaptureTap::DryWet`
is a latency-probe tap, not dry/wet semantics.

**Design (recommended).** Two primitive loops, dry and wet, coordinated by a small pair
node in the loop library that moves both members between modes at one boundary with a
shared position and length; routes stay with the application. Pair modes: record (both
record), play (dry parked, wet plays), dry through wet (dry plays, wet parked), dry into
wet (dry plays, wet takes one aligned fixed-length cycle). "Parked" is a new silent
state that keeps the position. Both members succeed or are refused together; a failed
or stopped wet take keeps the old wet version. The alternative, one loop with channel
roles, needs versions made of parts and per-role modes in the ledger and planner.

**Effort.** L (~3 weeks), excluding latency compensation and track/UI integration.
First version (S–M): two loops driven by the application with `Timing::At`, plus the
fixed-length re-record.

### Modes inside composites

**Legacy.** Child modes are a resolved default or an explicit mode
(`src/rust/shoop_engine/src/composite_plan.rs:134-144`); regular composites resolve each
child's track default when it is triggered and latch it
(`src/rust/shoop_engine/src/composite_timeline.rs:1404-1420`). Script composites can
schedule any mode, and regular composites can themselves record.

**Today.** Regular entries compile to playing (`src/rust/shoop_loops/src/program.rs:301`);
programs emit only stop, record, play and seek; a composite can only be played, sought
by iteration or stopped.

**Design.** A program entry mode enum (default, play, record, replace, dry through wet,
dry into wet) with compile-time checks that each mode suits its target; a per-loop
default playback mode in the resolver, read when a composite triggers the child.

**Effort.** M (1–1.5 weeks) once replace and dry/wet exist; default resolution alone is
S. Recording a regular composite is a further gap.

## Decisions needed

1. **Dry/wet shape:** two coordinated loops (recommended) or one loop with channel
   roles?
2. **Replace:** full punch-in with segmented media, or one-cycle replace only? Silent
   while replacing, as in legacy, or optionally hear the old content?
3. **Pre-play:** speculative as in legacy, or only for starts at a known frame? Do
   composite-triggered starts pre-play?
4. **Input history:** per loop or per track? Bounded by overlapping takes, segment
   chains or head trimming?
5. **Long recordings in the browser:** preallocated maximum, or allocation between
   render quanta? Page budgets per take?
6. **Latency:** may a settling take play? Separate dry and wet offsets?
7. **Trimmed takes:** may a length run past the recorded media? Legacy pads with
   silence; the architecture currently forbids implicit padding.

## Suggested order

1. **Foundations (~4 weeks):** gain and balance, playback windows, fixed-length
   cycle-aligned capture, low-storage reporting.
2. **Capture features (~5–6 weeks):** one-cycle grab, then full grab with the history;
   positive latency offsets, then negative offsets using the history.
3. **Modes (~5–6 weeks):** dry/wet pair and re-record, one-cycle replace, composite
   modes.
4. **Higher-risk extensions, as needed:** full punch-in replace with segmented media,
   pre-play, growing storage.

## Basis

Three read-only investigations covered the capture side, the playback window and
levels, and the modes. These claims were rechecked directly in code: media length must
equal the loop length and playback reads from position zero; gain is one value per loop
multiplied into every channel; capture is append-only; `Record` has no duration; regular
composite entries default to playing; legacy replacing is silent and re-recording is
application logic; storage is one fixed allocation with an `i32::MAX` ceiling; legacy
caps recordings at 120 seconds and input history at 30 seconds. Other citations come
from the investigations' code reading. Designs and estimates are proposals.
