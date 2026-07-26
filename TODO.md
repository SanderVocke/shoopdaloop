# TODO

## Goal

Rust engine at parity with the C++ engine, plus a new egui GUI to test-drive it --
inspired by the existing GUI, with at least one pre-configured instrument (a wave
generator is fine) so it can simply be started and played, playable from the computer
keyboard, and at feature parity with the existing GUI.

The instrument requirement changes the critical path in a useful way: a built-in
oscillator means a playable application no longer waits on LV2/Carla hosting.
`src/wave_generator.rs` is that oscillator -- polyphonic, fixed voice pool so it cannot
allocate while playing, with a short envelope purely to stop clicks. Its frequency is
verified by counting zero crossings in the rendered output, not just by checking the
note table.

Scale to be aware of before promising parity: the existing GUI is about 24,000 lines of
QML across 126 files. Beyond the tracks-and-loops grid it includes composite loops, a
Lua scripting engine with its own transport, MIDI-control mapping, session save/load, an
FX-chain UI, a settings window and a connections window. The grid, transport and
instrument are the part that makes it usable; the rest is a long tail that wants
ordering by what is actually used.

## GUI (`src/rust/shoop_gui`)

Runnable: `cargo run -p shoop_gui --release`. Opens the default output device, builds a
4x4 grid of loops fed by the built-in wave generator, and plays from the computer
keyboard. Verified end to end against real hardware in `tests/end_to_end.rs` -- a played
note is recorded into a loop and the capture is checked for actual signal, not just for
length.

A library as well as a binary, so the wiring can be tested without a window. The
interesting part is instrument-to-engine-to-device, and none of it needs a UI to exercise.

Shape follows the existing QML GUI: a toolbar of global toggles (sync, solo, play after
record, stop all, DSP load, xruns, voice count) over a grid of tracks with loops down each
column, each loop showing its mode, its planned next mode, and its position within its
length.

How the instrument reaches the engine, since it is not obvious: the synth runs in the
audio callback via `cpal_driver::start_output_with_hook` and stages its output into a
session *input* port. That makes it indistinguishable from a device, so a loop channel
recording that port records the instrument, and a real input would wire up identically. It
also means no in-graph FX node was needed, which is why a playable application did not
have to wait for LV2/Carla.

Two traps found by building it, both worth keeping in mind:

- **The sync loop has to be running, and long enough.** Followers' planned transitions
  fire when their sync source wraps, so a stopped sync loop makes the sync toggle silently
  do nothing. Worse, a length derived from a session that does not yet know its sample
  rate comes out as one frame, and a one-frame loop wraps every frame: the engine caps how
  many sub-blocks a cycle may be split into, so it stalls into silence rather than playing
  fast. `ASSUMED_SAMPLE_RATE` covers the case and two tests pin it.
- **A loop shorter than `buffer / MAX_SUB_BLOCKS` frames stalls its cycle.** That cap is
  deliberate, and `n_stuck_cycles` reports it, but nothing prevents configuring one. Worth
  a guard when loop lengths become user-settable.

Per-track gain, muting and level meters are done. They needed a structural change worth
knowing about: a loop wired straight to the device leaves nowhere to put a track's gain or
read its level, so every track now has its own internal port that its loops feed and which
routes on to the device. Meters read the port's output peak once per frame and reset it, so
a reading is that interval's peak rather than the loudest moment since launch, and the UI
holds the slider values itself so a drag cannot jump when a poll returns.

Session save and load are done, in `src/persist.rs`. One JSON file with the samples
inline: the existing application uses a tar of wavs, which is the right format for real
takes, and this is deliberately the simplest thing that round-trips. **That trade wants
revisiting before this carries real recordings** -- a minute of stereo in JSON is tens of
megabytes.

Three decisions in it worth keeping: loops are addressed by grid position rather than by
index, so a file written by a build with a different creation order still lands in the
right cells; the format carries a version and refuses one it does not understand rather
than misreading it; and an unknown waveform name falls back to sine rather than making a
whole session unloadable. Capture is one round trip for everything, because a session read
across sixteen separate moments could be internally inconsistent. Restore stops a loop
before loading into it, since loading underneath a playing loop jumps its position into
material that is no longer there.

Save and load use a fixed path (`shoop-session.json`, shown in the UI) rather than a file
dialog, which would mean another dependency. Worth replacing with a real dialog.

Loop selection and grouping are done, in `src/selection.rs`, kept out of the UI so the
rules can be tested -- what a click does depends on its modifiers, and getting that subtly
wrong is annoying to use and invisible in a screenshot.

The rules follow a file manager, since that is what people already expect: plain click
replaces, ctrl or cmd toggles, shift extends from the anchor. Two choices worth keeping: a
range selects the **rectangle** between the corners rather than a reading-order run,
because "these two rows across three tracks" is what a looper user means and a
reading-order range would drag in whole tracks in between; and a toggle moves the anchor
even when it deselects, so a following range extends from where the user last pointed. A
range with no anchor acts as a plain click rather than doing nothing, which would read as
broken.

The selection handle is the loop's number, not the whole tile, so clicking a loop's
transport buttons does not also change what is selected.

Composite loops are done, in `src/composite.rs`. Built above the engine rather than inside
it, as the existing GUI does and for the same reason: the engine knows about loops and sync,
not about arrangements. A composite is a list of members with a start cycle and a length in
cycles, and running it issues the same play and stop commands a user would, at the cycle
each member is due.

Expressed in sync-loop cycles rather than frames, which is the grid a user arranges on and
makes the schedule independent of sample rate and buffer size.

Three behaviours the tests pin, each an easy thing to get wrong and hard to find by ear:

- A member continuing across a boundary -- two back-to-back entries for the same cell -- is
  **not** restarted, because stopping and restarting it leaves an audible gap.
- A member listed twice at the same cycle starts once; issuing two starts would retrigger.
- A run that ends reports *every* member so nothing is left ringing, and a repeating run
  stops what was playing before starting cycle zero again.

`CycleCounter` infers a wrap from the published position going backwards, because the engine
publishes a position rather than a cycle count. Two cases it has to get right: the first
reading is never a wrap, and a repeated position is not one either -- a paused transport
polls the same value, and counting that would advance the arrangement on its own.

MIDI-control mapping is done, in `src/midi_control.rs`, with the connections row in the UI
listing the system's MIDI inputs. Verified end to end over a real virtual port: a pad press
sent through CoreMIDI resolves to a grid action.

The existing GUI has a whole editor for this with message filters; this is the same idea
reduced to a trigger, an action, and "any channel", which is what a looper needs. A default
mapping ships so a generic controller works without configuring anything: pads from note 36
play the grid, controllers 1 upwards are track levels, and CC 123 stops everything, which is
what a panicking player reaches for.

Two matching rules the tests pin. A note **release** must not fire an action, including
note-on with zero velocity -- controllers that send it would otherwise act twice per press.
And a controller fires at *every* value including zero, or a fader could never reach silence.
Controller values scale by 127 rather than 128, so full scale reaches exactly 1.0.

Control messages are drained on the UI thread rather than in the audio callback: these are
commands, not notes, so a buffer of latency is irrelevant, and it keeps the callback free of
the mapping.

Deliberately not saved in the session file: a mapping belongs to a controller rather than to
a piece of music, and saving it would mean sessions carrying bindings for hardware the next
person does not have.

A settings window is done: it lists the host's output devices and switches between them. The
switch rebuilds the session rather than moving it, because ports belong to the device, so a
new device means new ports and a new graph -- but loop contents are carried across, since
losing a take to a device change would be unforgivable. A name that no longer resolves falls
back to the default rather than refusing to start, which is the case that matters: a remembered
device gets unplugged.

The connections row covers what the connections window did for MIDI.

The Lua scripting engine is done, in `src/script.rs`, with a panel to edit and load a script.

**The design choice, since it was the one that mattered:** a script is a *caller* of the
control surface, not a description the surface is generated from. That is the shape the C++
side chose (`tst_LuaEngine_SessionControlHandler`), and it means a script can only ask for
things a user could ask for -- so it cannot reach a state the UI cannot also reach, and neither
the engine nor the UI has to know scripting exists.

Scripts never touch the engine. They append to a queue of `ControlAction`s that the application
drains and applies through the same code that applies a click or a MIDI binding. That is what
makes it testable with no audio device, and it means a misbehaving script cannot stall the audio
thread -- the worst it can do is ask for too much.

The transport is a `shoop.on_cycle(cycle)` function the script may define, called once per
sync-loop cycle, on the same grid composites use. It has its own wrap counter so it ticks whether
or not an arrangement is running.

Four behaviours the tests pin:

- Coordinates are **one-based**, matching what the grid shows, and a zero or negative coordinate
  is clamped rather than cast -- an unsigned cast of -1 would be catastrophic.
- Reloading **replaces** a previous cycle hook, so a script with no hook cannot leave the old
  one running.
- A hook that raises is reported **once** and then stopped, rather than raising every cycle and
  scrolling its own message away.
- A syntax error is reported at load, where the user just acted, rather than stored.

`mlua` is taken from the existing workspace dependency, which the QML frontend already uses, with
the `vendored` feature added so Lua is built from source and this crate needs no system library.
The feature is additive and self-contained, so it does not change how the frontend links.

### GUI parity: the actual gap list

Earlier reports worked from a partial survey and understated this. Enumerating all 90
top-level QML components and separating features from plumbing gives the following. Registry,
Mapper, SchemaCheck, ShoopButton and the other wrappers are QML infrastructure with no egui
equivalent and are not counted.

Built:

| Existing GUI | Here |
| --- | --- |
| `AppControls` | toolbar with sync / solo / play-after-record / stop-all |
| `LoopWidget`, `Loop`, `BackendLoopWithChannels` | loop grid with per-loop transport |
| `TrackWidget`, `TracksWidget`, `TrackControlWidget`, `TrackControlLogic` | track columns |
| `AudioLevelMeterModel`, `AudioSlider`, `AudioDial` | per-track gain, mute and meters |
| `SelectedLoops` | selection with group actions |
| `CompositeLoop`, `EditCompositeLoop` | composites (build-from-selection; no visual editor) |
| `MidiControl`, `MidiControlConfiguration`, `EditMidiControl` | MIDI mapping with a default map |
| `LuaEngine`, `LuaScript`, `LuaScriptManager`, `ScriptTransport`, `SessionControlHandler` | scripting with a per-cycle transport |
| `SettingsWindow`, `Settings` | settings window (audio device selection) |
| `ConnectionsWindow`, `ConnectionsControl` | MIDI connections row |
| `Session` | session save and load |
| `ClickTrackDialog` | metronome, divided from the sync loop |
| `WaveformCanvas`, `ChannelDataRenderer`, `LoopContentWidget` | waveform per loop with a position line |
| `LoopDetailsWindow`, `DetailsPane` | per-channel gain, start offset, pre-play, mode |
| `MonitorDspLoadGraph` | DSP load history graph |
| `MonitorWindow`, `MonitorBackendRefreshRate` | monitoring window: driver, engine and session counters |
| `NewTrackDialog` | adding tracks and loops at runtime; retiring loops |

| `FXChain` | an effect insert per track, with built-in effects |

Not built:

1. **Profiling and debug-inspection windows** (`ProfilingWindow`, `MonitorAudioBufferPool`,
   `DebugInspection*`). Developer tools rather than features, and the engine has no per-node
   profiler to feed a profiling window, so this needs engine work first.
2. `MonkeyTester`, `TestRunner`: a QML-specific fuzz harness and test runner. The Rust side's
   equivalent is the test suite, so these do not carry over.

The click track is in `src/click_track.rs`. Derived from the sync loop's position rather than a
clock of its own, so it cannot drift and changing the sync length changes the tempo for free. It
computes only *when* to click and on what note; the sound comes from the instrument, which already
renders notes into the same port.

Three cases the tests pin: a beat sounds once however often it is polled, since it is driven by
polling rather than by an event; more beats than the bar has frames falls silent rather than
machine-gunning; and enabling it mid-bar clicks the beat it lands in rather than suppressing it as
already sounded.

The waveform display is in `src/waveform.rs`, with the drawing in `app.rs`. Min and max per
column rather than an average: averaging a symmetric waveform tends to zero and draws a flat line,
which is exactly wrong for the thing a waveform is looked at to see. The tests check that a single
spike survives reduction wherever it sits including at the very end, that a symmetric wave does
not flatten, and that a ramp stays monotonic.

Samples are only fetched when a channel's sequence number changes, and only every fifteenth
frame. Contents change when a recording stops, not continuously, so polling at frame rate would
mean shipping whole buffers off the audio thread sixty times a second to learn nothing.

The DSP load history is in `src/history.rs`: a ring, because it is fed from the UI loop forever
and has to have a bound. Its tests pin the case a ring gets wrong -- wrapping exactly once, where
the whole buffer is valid and `next` is back at zero -- because reading it in the wrong order
draws a time-reversed graph that still looks plausible.

Load itself is **measured** on the cpal path rather than asked for: cpal exposes no load figure,
so the callback times its own duration against the buffer's. That is what an audio host normally
does, and it is why the graph shows anything at all on this backend, where the JACK
`sample_dsp_load` has no counterpart.

The details pane reads a channel's settings once when it opens rather than polling: these are
settings a user changes, not state that moves, and re-reading every frame would fight the widgets.
Edits are applied to the cached copy as well as queued, so a slider does not snap back before the
next read.

The monitoring window surfaces what the ordinary UI hides, and each figure is there because
nothing else would reveal the fault it indicates: **refused cycles** mean the graph was stale when
the callback ran, which sounds exactly like silence; **stuck cycles** mean a cycle gave up because
a loop was shorter than the cycle could be split, which also sounds like silence and is called out
in red; **sub-blocks last cycle** shows how finely the cycle had to be divided, which should track
the number of loops and not much else; and **capture underruns and overruns** show the duplex ring
drifting.

Read on a slow cadence, since these are diagnostics and a blocking read per frame would cost more
than the information is worth.

Tracks and loops can be added at runtime. The grid iterates the layout rather than the two
constants, so tracks may have different numbers of loops -- a ragged grid is allowed, because
forcing every track to the same depth would mean silently adding loops nobody asked for.

Startup builds the grid through the same `add_track` helper additions use, so a track made at
startup and one made later cannot drift apart. A new track gets as many loops as the widest track
already has, so it lines up instead of leaving a ragged column.

### Engine parity: what is genuinely left

**LV2/Carla plugin hosting**, and it is now the only engine item that cannot be validated here: it
needs the C++ plugin host across a cxx boundary and there are no plugins installed to test against.
The shape is recorded further down.

It is no longer blocking anything. `FxChain` exists in `src/fx_chain.rs` as an insert on the audio
port core, so any port can host one and every port type gets it from one place, with built-in effects
(a one-pole low pass and a delay with feedback) filling it. A plugin becomes one more `EffectKind`
behind the same interface rather than a prerequisite for having effects at all -- the graph position,
dry/wet handling, bypass and the UI have all settled and none of them need to change when a host
arrives.

The insert runs *before* gain and muting, so the fader is post-effect and muting silences the
effect's tail rather than leaving it audible. There is a test for exactly that, because the opposite
order is the plausible mistake. Everything is sized when the chain is configured, so changing a delay
time or a cutoff never allocates: the delay line is sized from `MAX_DELAY_SECONDS`, not from the
current time.

Also owed and mechanical rather than hard: re-pointing `frontend` and `shoopdaloop` off
`backend_bindings`, after which the C API and bindgen can go. Both touch the QML build, which cannot
be run here.

**A caution learned the hard way.** `port_connections` used to order the graph without copying any
audio, so every internally-routed port was silent -- which meant loops went inaudible the moment
per-track ports were introduced. That survived hundreds of tests because they measured the port
*before* the routing hop. When adding a signal path, assert at the far end of it: at the device, if
that is where the sound is meant to come out.

## Rust backend port (`src/rust/shoop_engine`)

Greenfield engine crate replacing the C++ backend. `#![forbid(unsafe_code)]`;
FFI (JACK, miniaudio, LV2) belongs in separate crates so this stays safe.

The seam is `backend_bindings`, not the C API: the frontend touches only
`backend_bindings`, so when the engine is complete the C API
(`libshoopdaloop_backend.{h,cpp}`) and its bindgen layer are deleted rather than
reimplemented.

Verification: C++ Catch2 cases are translated into Rust `#[test]`s, which needs
no C++ build. The C++ `test_runner` is kept as a differential reference.

### Done

- `loop_mode` — `LoopMode`, discriminants pinned to `shoop_loop_mode_t`.
- `basic_loop` — POI tracking, trigger propagation, planned transitions. All 9
  cases of `test_BasicLoop.cpp` translated, plus boundary coverage the C++ unit
  suite lacks (`position == length`, playing-to-playing position retention).
- `graph` — arena scheduler, Kahn with co-process grouping via union-find.
  Reproduces all three expected schedules in `test_graph_construction.cpp`.
- `chunked_samples` — fixed-chunk sample store behind audio channels. Retired
  chunks go to a spare list and growth takes from it, so a growing recording
  neither allocates nor frees while the reserve lasts; `n_allocations` reports when
  it runs out.
- `channel_mode` — `ChannelMode` plus the loop-mode-to-channel-flags collapse and
  pre-play/pre-record arming (`channel_mode_helpers.cpp`).
- `audio_channel` — record, replace, playback with gain/peak/pre-play/start
  offset, and pre-record carry-over. Deferred copies are index-based rather than
  pointer-based so the crate stays unsafe-free.
- `midi_storage` — fixed-capacity message ring, cursors, and truncation. Cursors
  address messages by absolute index and reconcile via `Cursor::sync`, replacing
  the C++ registry of `weak_ptr` cursors.
- `midi` — message classification and construction.
- `port` — `PortDataType` / `PortDirection` / `PortConnectability` with C ABI
  discriminants, and `AudioPort`'s signal path: gain, muting, input/output peak
  metering, always-on capture. The buffer is passed into `process` rather than
  fetched through a virtual `PROC_get_buffer`, so this needs no driver knowledge.
  Port naming, external connections and buffer acquisition stay with the drivers.
- `midi_port` — MIDI port core: state tracking, event metering, muting,
  always-on capture, and the lagging tail state that tells a retroactive
  recording what state it began in. Source events and the optional output sink are
  passed into `process` instead of reached through four virtual buffer getters.
  Note the C++ `PROC_process` ends with an unreachable branch: it guards on
  `!processed_state && read_out_buf`, but `input_buf` is non-null whenever
  `read_out_buf` is, so `processed_state` is always already true there.
- `internal_audio_port` — engine-internal audio routing port that owns its buffer.
- `dummy_port` — `DummyAudioPort` (queue in / retain out) plus
  `DummyExternalConnections`, the mock external-port registry. Connection identity
  is an explicit `PortId` rather than a raw `DummyPort*`. The name pattern is
  anchored (`^(?:..)$`) because the C++ used `std::regex_match`, which requires a
  full match unlike Rust's `is_match`.
- `dummy_midi_port` — queued input with cycle-relative times that shift as cycles
  advance, and request-scoped capture of written output rebased to the request.
- `decoupled_midi_port` — bounded queue bridging the process thread and control
  thread for controller traffic. A full queue counts the loss instead of dropping
  it silently as the C++ does. Genuinely lock-free cross-thread handoff belongs
  with the driver work, where the thread boundary exists.
- `midi_buffering_input_port` — copies a source's messages so several consumers
  can read one cycle's arrivals; muting is applied at the copy.
- `graph_build` — lowers a declarative description of ports/loops/channels into
  scheduler nodes, replacing the C++ per-node virtual edge methods. Reproduces all
  three `test_graph_construction.cpp` schedules from real topology rather than
  hand-stated edges.
- `session` — owns the port/loop/channel arenas, maintains the schedule behind a
  request/applied id pair, and runs a cycle end to end. Both audio and MIDI work:
  dummy input -> record -> loop -> play -> dummy output. MIDI routing happens in
  the loop's node, since MIDI is emitted during loop processing rather than
  deferred like audio copies. Cycles are split into sub-blocks at the earliest
  point of interest across a co-processed group, matching `process_loops`, so a
  loop ending mid-buffer is advanced in pieces and co-processed loops of different
  lengths stay sample-aligned. `n_sub_blocks_last_cycle` exposes the split count as
  a performance signal; `n_stuck_cycles` counts cycles that hit the bound.
  Sync sources are wired: every loop is co-processed with every other, as the C++
  does, and each follower's `SyncSourceState` snapshot is refreshed twice per
  sub-block -- before measuring points of interest, and again after they are
  handled so a trigger fired this sub-block is visible to followers.
- `midi_sorting_buffer` — stable time-ordering of messages written out of order
  by several sources within a cycle. Reading before sorting returns `None`
  instead of an unsorted view; an oversized message is refused and counted rather
  than thrown on (`n_rejected`), and exceeding the reservation is counted
  (`n_overflows`) instead of printed to stderr.
- `midi_ringbuffer` — rolling MIDI capture window over `midi_storage`, with the
  timestamp rebase that avoids running out of `u32`. The rebase is only correct
  while `n_frames <= n_samples` (one audio buffer against a multi-second window),
  which holds in practice but is an unstated assumption in the C++ too.
- `buffer_queue` — bounded rolling capture FIFO behind the "grab" feature
  (`BufferQueue.{h,cpp}`). Retains one buffer at a zero limit, where the C++ pops
  unconditionally and then dereferences the destroyed back buffer.
- `midi_state` — `MidiStateTracker`: notes, controls, programs, plus
  `state_as_messages` and `diff_to`. Replaces both the C++ `MidiStateTracker` and
  `MidiStateDiffTracker`; the diff is computed by comparing two trackers instead
  of pushing changes through `*mut dyn Subscriber`.
- `midi_channel` — record, playback, pre-record carry-over, start-state capture
  and restoration, playback-interruption All Sound Off, validity window with
  pre-play.
- `audio_midi_loop` — audio and MIDI channels attached to a loop, both folded into
  the loop POI. `LoopError` says which channel kind failed.

Each unit was mutation-tested (`scratchpad/mutate.sh`); every surviving mutant
either got a new test or is recorded under Notes as genuine redundancy.

### Remaining, roughly in dependency order

1. Keep translating the C++ Catch2 suite. It is the differential oracle, and
   translating it has found every real divergence so far. Status by file, with case
   counts as the C++ declares them (generated variants make the runner report more):

   | C++ file | Cases | Translated as |
   | --- | --- | --- |
   | `test_MidiStorage.cpp` | 4 | `tests/midi_storage.rs` |
   | `test_AudioMidiLoop_midi.cpp` | 12 | `tests/audio_midi_loop_midi.rs` |
   | `test_MidiStateDiffTracker.cpp` | 3 | `tests/midi_state_diff.rs`, reinterpreted |
   | `test_BasicLoop.cpp` | 9 | `tests/basic_loop.rs` |
   | `test_AudioMidiLoop_audio.cpp` | 20 | `tests/audio_midi_loop_audio.rs` |
   | `test_MidiChannel.cpp` | 1 | `tests/midi_channel.rs` |
   | `test_MidiRingbuffer.cpp` | 6 | `tests/midi_ringbuffer.rs` |
   | `test_BufferQueue.cpp` | 8 | `tests/buffer_queue.rs` |
   | `test_DummyPorts.cpp` | 14 | `tests/dummy_ports.rs` |
   | `test_InternalAudioPort.cpp` | 6 | `tests/internal_audio_port.rs` |
   | `test_DummyAudioMidiDriver.cpp` | 6 | `tests/dummy_driver.rs`, partly reinterpreted |
   | `test_JackPorts.cpp` | 22 | `tests/midi_port.rs` for the MIDI core; the rest needs the JACK driver |

   Five of the translations are not literal, for reasons worth keeping:

   - `test_MidiStorage.cpp` sizes its buffers as `n * sizeof(Storage::Elem)` and
     asserts on `bytes_occupied` / `bytes_free`, because the C++ storage is a byte
     ring of variable-length elements. This storage counts fixed-size elements so as
     not to depend on C++ struct padding, so those become a capacity of `n` elements
     and `is_full`.
   - `test_MidiStateDiffTracker.cpp` inspects an incrementally maintained set of
     `(status, data1)` keys, which this design does not have -- differences are
     computed by comparing two trackers at restore time, which removes the
     subscriber wiring. Its three cases all guard one bug class, channel pressure
     keyed under the pitch wheel's status byte, and that bug class survives the
     redesign because the two are neighbouring fields of one struct. So they are
     asserted through the messages a restore emits instead, plus the converse
     direction the C++ suite leaves unguarded.

   - `test_AudioMidiLoop_audio.cpp` uses `AudioChannel<int>` so its sample
     comparisons are exact; this engine is `f32`-only, which keeps them exact anyway
     because every value those cases use is a small integer. Its
     `BufferPool<int>(10, 5, 64)` contributes only a chunk size, since recording here
     recycles chunks from a spare list rather than borrowing from a shared pool, and
     `add_audio_channel(pool, 10, ...)` passes an initial buffer count rather than a
     size -- easy to mistake for the chunk size, and it changes playback granularity.

   - `test_DummyAudioMidiDriver.cpp`'s first two cases start a driver thread, wait
     for it, and inspect what a tracker recorded. This driver owns no thread -- the
     caller runs the cycle and the driver only decides how many frames each cycle
     gets -- so those become assertions about the chunk sizes handed out, which is
     where the behaviour actually is. Its four port cases translate directly.

   - `test_JackPorts.cpp` opens JACK ports against a fake JACK API and injects a
     buffer per cycle. What its MIDI half actually exercises -- mute, event counters,
     note tracking, output ordering -- is the `MidiPort` core a JACK port would
     delegate to, and `MidiPort::process` takes a cycle's events directly, which is
     the same shape as the mock's buffer, so those are asserted against the core.
     Its input and output variants collapse into one case each, because there is a
     single core rather than a port class per direction. Its audio half duplicates
     `tests/dummy_ports.rs` almost exactly and is not repeated.

     Still owed there once a driver exists: JACK port registration, reading and
     writing its buffers, and the direction-dependent access flags, which belong to
     the port type rather than the core. The dummy port is not a substitute -- it
     hard-codes all four access flags to true, as the C++ `DummyMidiPort` does, and
     its input queue is rebased per cycle rather than being a fresh per-cycle buffer.

   The `process_synced` helper in `tests/audio_midi_loop_midi.rs` mirrors
   `process_loops.h`: a transition driven by a sync source only lands on the right
   frame if the buffer is split at points of interest and the sync snapshot is
   refreshed between sub-blocks. Without it the transition slips to the end of the
   buffer, so the four cases that transition mid-buffer need it.
   `tests/audio_midi_loop_audio.rs` has the same helper as `advance_synced`, which
   only advances: the audio cases finalize their channels once at the end, so queued
   copies accumulate across sub-blocks.

   Two things the audio translation pinned that are easy to get wrong: an audio
   channel's recording-buffer cursor advances through frames processed while
   *stopped*, so a recording that starts later reads from the corresponding offset
   rather than from the start; and `plan_transition` with a sync cycle takes effect
   immediately, moving the position to where it would have been had the loop been
   running all along.

   Two things the translations turned up that were not test-only:

   - `set_contents` dropped messages instead of growing. Built with room for one
     message and handed three, my channel kept only the last; the C++ allocates a
     storage sized to `max(current capacity, needed)`. Both storages now grow
     together, because the process path adopts pre-recorded material with
     `copy_into`, which resizes its destination -- growing only one of them would
     move that allocation onto the audio thread.
   - The ringbuffer's overflow rebase is confirmed against the oracle's own expected
     values, so the earlier note doubting it is settled. Worth knowing: the C++ case
     that covers it advances to the overflow point in *one* call, not in 512-frame
     steps as it appears to. Its `std::min(512, (int)(target - end))` casts a value
     near 2^32 to a negative `int`, so the first call consumes the whole distance and
     leaves the current buffer starting at 0, which is what its expected times
     depend on. Stepping properly gives a different base and nothing lines up.

3. Channel and port removal on `session`: done, and the earlier cost estimate here was wrong.

   I had costed a tombstoned arena at roughly a hundred call sites across `session.rs` and
   `graph_build.rs`. That was over-scoped. Removal does not need the arena to change at all: a
   removed object is made **inert** instead. A channel is disconnected and set to `Disabled`, which
   `next_poi` already returns nothing for; a loop is stopped, emptied, its channels removed and
   anything syncing to it detached; a port is disconnected in both directions and dropped from any
   channel that read or wrote it. Scheduling a node that does nothing is harmless, so the graph
   builder needed no change.

   That keeps the property the tombstone design was chosen for -- no index ever moves, so
   `control.rs` handles and the drivers' port maps stay valid -- at a fraction of the cost. What it
   gives up is reclaiming the slot, which for a session-lifetime arena is not worth a refactor.

   The trap the tests pin: **a follower left syncing to a removed loop waits for triggers that never
   come**, so its planned transitions never land and it appears simply not to work. Removal clears
   it from every other loop's sync source.

   In the GUI a retired cell is hidden rather than forgotten, since the engine kept the slot -- so
   "restore retired" brings it back by putting its channel into `Direct` again, with nothing to
   rebuild. Retiring also drops the cell from the selection and from any arrangement, because acting
   on a hidden loop would be a surprise.

5. Move schedule recomputation off the audio thread once a driver exists. The
   session currently refuses to run a stale graph rather than papering over it,
   and recomputation is an explicit call.
6. Drivers. The JACK one exists behind the `jack` feature
   (`src/jack_driver.rs`); miniaudio duplex is still owed, and MIDI on non-JACK
   backends comes from `midir`, whose timestamps are host-clock, so MIDI has to be
   carried as `(frame_offset, bytes)` -- that path costs about one buffer of extra
   MIDI jitter. Do not assume a shared audio/MIDI callback or a fixed buffer size.

   **Still not "behind one trait", deliberately.** JACK is only the second driver, so
   the shape it shares with the dummy driver is still a guess. Extract the trait when
   miniaudio makes it three.

   What the JACK driver does, and why it is arranged that way:

   - It owns an `Engine` (`src/engine.rs`), which owns the session on JACK's realtime
     thread. Control work is queued as closures and applied at a cycle boundary, the
     same arrangement as the C++ `WithCommandQueue`. `Engine` and `Session` are
     asserted `Send` in `engine.rs`, so a driver moving the engine onto its own thread
     is a compile error away from being wrong rather than a runtime surprise.
   - Executed command boxes are **sent back** to the control thread rather than
     dropped. Freeing is as forbidden on the audio thread as allocating, and
     `assert_no_alloc` catches both; reverting the return queue to a plain drop fails
     `applying_commands_does_not_allocate`. The return queue is as large as the command
     queue, so returning can never fail and the audio thread never has to choose
     between leaking and freeing.
   - JACK hands out port buffers only inside the callback and only for its duration.
     Rather than teach every port to borrow from a `ProcessScope`, the driver copies:
     one memcpy per port per cycle, and the engine stays free of JACK's lifetimes.
   - Input is **staged** before the cycle and picked up by the port's `prepare`. It
     cannot be written straight into place, because `prepare` runs partway through the
     schedule -- ordered against the channels that read the port -- and clears the
     buffer so that a cycle nobody fed reads as silence, which the oracle pins.
   - `ExternalAudioPort` and `ExternalMidiPort` exist for this. The dummy ports are not
     usable: their queues span cycles and are rebased by however many frames were
     processed, dropping anything now in the past. Right for a test that sets up a
     sequence up front, wrong for a driver handing over one cycle at a time. The
     external MIDI port also gates reads on mute, as the C++ JACK port does.

   Not covered by tests, and honestly so: the JACK code itself needs a running
   server. Everything it delegates to is covered, and `tests/external_ports.rs` drives
   the session exactly as the driver does. Building the driver needs
   `PKG_CONFIG_PATH` pointing at `build/vcpkg_installed/arm64-osx/lib/pkgconfig`;
   running needs `DYLD_LIBRARY_PATH` to include that tree's `lib`, as the C++ oracle
   does. libjack also probes for a versioned `libjack.0.dylib` that vcpkg does not
   ship, prints two `dlopen error` lines, and works anyway via the direct link.

   Since done: JACK's connection API is exposed (`find_external_ports`, `connect`,
   `disconnect`, with already-connected and not-connected treated as success so a
   caller reapplying a saved session need not care); xruns arrive through a
   `NotificationHandler` and DSP load is sampled on demand into `engine::Stats`; and a
   buffer-size change is handled on `ProcessHandler::buffer_size`, which JACK calls on
   the process thread but explicitly allows to allocate, so the session is resized
   there rather than through an atomic the callback has to poll.

   A second real driver now exists and, unlike the JACK one, **has actually run**:
   `src/cpal_driver.rs`, behind the `cpal` feature, verified against this machine's
   CoreAudio output at 48 kHz / 2 channels in `tests/cpal_driver.rs`. The test asserts
   the device callback drives cycles and that a playing loop advances, and it skips
   rather than fails where there is no output device, so headless CI stays green.

   `cpal` rather than the miniaudio duplex the design originally preferred, for a
   reason found by trying it: the `miniaudio` Rust binding is pinned to bindgen 0.54 and
   no longer builds at all. `cpal` is pure Rust and works. It is pinned to 0.16, which is
   what `rodio` already brings in for the frontend -- cpal declares `links = "alsa"`, so
   two versions cannot coexist in one workspace.

   Capture works too, via `start_duplex`, and it has run: 1 input and 2 output channels
   on this machine, 47 cycles, 1 underrun, 0 overruns.

   The original objection to cpal was real and had to be solved rather than avoided. It
   gives independent input and output streams with separate callbacks and no shared clock,
   so capture cannot be handed to the same cycle the way JACK's single callback allows.
   What bridges them is a ring: the input callback pushes, and the output callback --
   which drives the engine -- takes a cycle's worth out.

   Drift is handled by refusing to hide it. A cycle that finds less than it needs gets
   silence for the shortfall and counts `capture_underruns`; a full ring drops the oldest
   samples and counts `capture_overruns`. Both live in `engine::Stats`, so persistent
   drift is a number rather than a mysterious glitch. The single underrun the test sees is
   the input stream spinning up; the test asserts `underruns < cycles` rather than zero,
   because demanding zero would be demanding that the two devices start together.

   `ExternalAudioPort::stage_input_strided` de-interleaves one channel straight into a
   port, so the callback builds no per-channel buffer. `cpal` hands over all channels
   interleaved, where JACK gives one buffer per port.

   MIDI for non-JACK hosts is done too: `src/midir_driver.rs`, behind the `midir`
   feature, and **validated end to end** against CoreMIDI in `tests/midir_driver.rs` --
   a note sent over a real virtual-port connection is captured, staged into a port, and
   recorded by a loop. An oversized sysex is refused and counted rather than truncated.

   Not a driver of its own, on purpose: `midir` has no audio clock, so it pairs with
   whichever audio driver is running. It offers a `MidiCapture` to drain into a port from
   that driver's callback and a `MidiPlayback` to send what a port produced.

   **Timing is coarser than JACK's and cannot be otherwise.** `midir` timestamps in
   host-clock microseconds with no relation to the audio callback's frame counter, so
   everything pending is staged at frame 0 of the next cycle -- up to one buffer of
   jitter. That is the price of the non-JACK path, and it is why JACK's single callback
   is what makes sample-exact MIDI possible there. Do not "fix" this without a shared
   clock; there is nothing to compute a true offset from.

   The trait exists now, in `src/driver.rs`, and having three drivers is what made it
   possible to draw honestly.

   What it does *not* contain is the point. There is no `process` method. Cycles arrive
   differently in every one: the dummy driver is pulled by its caller, JACK pushes from
   one callback covering both directions, and cpal pushes from two callbacks bridged
   through a ring. A trait with `process` would have fitted two of the three and misfitted
   the one whose shape is hardest. So `Driver` covers only what is genuinely shared --
   sample rate, buffer size, client name, stats, and the engine handle -- and driving
   cycles stays with each driver.

   Extracting it also turned up that the old `DummyDriver` was never the same kind of
   thing as the other two: it decides chunk sizes but owns no engine, so it is a clock
   source, not a driver. `DummyEngineDriver` in the same module is the third real driver,
   pulled by its caller via `request_frames`, which is what a headless self-test wants --
   an exact number of frames rather than whatever a device asked for. It reuses
   `DummyDriver` for chunking rather than duplicating it.

   Two things worth knowing about the cpal implementation of the trait: its device gives
   no name in some configurations, so `client_name` falls back to `"cpal"`; and cpal does
   not commit to a buffer size, so `buffer_size` is published by the callback and reads
   zero until a cycle has run.

7. State readback for the control side: done, in `src/engine.rs`.

   With the session on the audio thread, a query cannot just read it, so the engine
   publishes a `StateSnapshot` each cycle and `EngineHandle::poll` takes the newest.
   `LoopSnapshot` carries what `backend_bindings::LoopState` does -- mode, length,
   position, and the next planned mode and its delay -- because that is what the UI
   reads.

   The mechanism is the same shipped-box pattern as the command queue, for the same
   reason: three snapshots circulate, the audio thread fills one and publishes it, and
   the control side hands the spent ones back. Nothing is allocated or freed on the
   audio thread. Two things fall out of that and are worth not undoing:

   - Publishing is **skipped**, not queued, when no box is free. The reader polls, so
     it wants the newest state and losing an intermediate costs nothing.
   - The audio thread fills only as far as a box already has room for and reports the
     shortfall in `n_loops`; `poll` then grows the box it is holding, so after a few
     cycles every box in circulation fits. Growing on the audio thread would allocate.
     `publishing_more_loops_than_fit_does_not_allocate` pins this, and it only pins it
     because the loops are created *after* `split` -- creating them first sizes the
     boxes to fit and the truncation path never runs.

8. Blocking reads for the control side: done, `EngineHandle::send_and_wait`.

   A snapshot cannot carry everything -- a channel's audio data, for one -- so there
   has to be a way to ask the engine a question and wait. The result comes back through
   a single-slot queue, so the audio thread stores it and moves on; the caller polls,
   as the C++ `CommandQueue::queue_and_wait` does, and for the same reason: the audio
   thread should never have to signal a condition variable.

   One deliberate difference from the C++. When it decides the process thread looks
   idle, `queue_and_wait` runs the command on the *calling* thread instead. This
   refuses and reports a timeout, because the handle has no session to run it against,
   and reaching around the engine to find one is how two threads end up inside it at
   once. A timeout means nothing is driving the engine, which is worth being told.

   For a large read, have the caller send a buffer in and the engine fill it, rather
   than having the engine allocate one: `returning_a_result_from_a_command_does_not_allocate`
   covers the handover, but a `Vec` built on the audio thread would still be an
   allocation there.

9. Resampling: done, in `src/resample.rs`, replacing the C++ `resample_multi` and
   its zita-resampler `VResampler`. Offline only -- it runs when audio is loaded,
   never in a cycle -- so it allocates freely.

   Two behaviours are carried over because callers depend on them: the output is
   *exactly* the requested length, padded by repeating the last frame rather than
   falling silent (a resampler's length depends on filter delay and rounding, and a
   silent gap clicks); and the ratio is clamped to `[1/16, 64]`.

   `rubato` is pinned to 0.16 rather than 4.0 on purpose: the 4.x API works through
   `audioadapter` traits instead of plain sample planes, which buys nothing here.

   No oracle for this one -- `resample_multi` has no Catch2 case -- so these tests are
   behavioural, derived from reading the C++. What *was* verified against the C++ is
   the buffer layout, which mattered: `shoop_multichannel_audio_t` carried the comment
   "Channels are not interleaved", but both `resample_multi`'s tail fill and
   `backend_bindings`' `at`/`set` index it as `frame * n_channels + channel`. The
   comment was wrong and is fixed; the layout is interleaved.

10. Extend `tests/no_alloc.rs` as more of the engine lands, and add a case that
   records past a chunk boundary once item 1 is done.
11. LV2/Carla hosting stays in C++ behind a ~14-method cxx trait
    (`ProcessingChainInterface` + `ExternalUIInterface` +
    `SerializeableStateInterface`), passing buffer slices rather than ports since
    the engine owns ports. `livi` is not usable: no state extension, no UI.
    Hand-rolled `lilv-sys`/`lv2-sys` when it is eventually ported.
12. Re-point `backend_bindings` at the engine; delete the C API and bindgen.

    Mostly done, as `src/control.rs`: `Backend`, `Loop`, `AudioChannel` and
    `MidiChannel` over the engine's queues, tested in `tests/control.rs` with the engine
    on its own thread, which is the arrangement a real driver provides.

    The handle-per-object shape is kept rather than reshaped around snapshots. Python
    and QML consume these types, so `Loop` and the rest stay handles -- an index plus a
    shared `EngineHandle` in place of the old `Mutex<*mut T>`. Reshaping might read
    better, but the consumers cannot be run here to find out, and a wrong guess is
    expensive to unwind.

    Which primitive each call uses follows from what it needs: a mutation queues and
    returns, as the C API's setters do; a read a snapshot covers reads the snapshot;
    anything else blocks. `create_loop` and `add_*_channel` block despite being
    mutations, because the caller needs the index before it can use the result, and
    guessing it would race any other creator. They also reschedule in the same command:
    a session whose graph is stale refuses to run, so a half-applied structural change
    would silence everything until someone noticed.

    Two things deliberately not carried over. `data_dirty` was a flag the caller
    cleared, which races anything else watching it; this reports the channel's data
    sequence number instead. And `clear_audio_channel_data_dirty` has no counterpart,
    for the same reason.

    Ports are done too: one `Port` handle for both data types, since the session keeps
    ports in one arena so an index is all that identifies one. Asking an audio port for
    MIDI counts errors rather than returning a different answer. Channel handles carry
    two indices -- the one within the loop, which is how the loop finds the channel, and
    the one within the session's channel arena, which is what connections use. Confusing
    those two is the obvious way to wire the wrong channel up.

    One trap worth knowing: `ringbuffer_n_samples` on a port state reports what is
    currently *retained*, not the window that was asked for. The C++ getter does the
    same, so it is carried over rather than corrected, but the name invites the other
    reading and a test written on that assumption fails.

    Still owed here: `FxChain` (waits on item 11), the `AudioDriver` handle, and then
    deleting the C API and bindgen once nothing calls them.
13. QML `--self-test` as the final integration gate (needs Qt).

### Realtime safety

`tests/no_alloc.rs` proves the process path neither allocates nor frees, using
`assert_no_alloc`. It is a separate integration test because it installs a global
allocator. Writing it found four real violations that reading the code had missed:

- `buffer_queue` grew a `Vec` every cycle. Now a genuinely fixed ring with all
  buffers allocated up front, which removed the need for a pool here entirely.
- `MidiChannel` cloned a `MidiStateTracker` on every playback restart, which
  happens at every loop wrap. The three optional trackers are now persistent with
  validity flags, matching the C++ `valid()` flags, so arming one reuses its
  buffers via `clone_from`.
- `MidiStateTracker::diff_to` allocated a `Vec<Vec<u8>>` per restore. Added
  `diff_to_into`, which appends into a caller-owned buffer.
- The dummy audio port queued blocks as `Vec`s and freed one per cycle when
  consumed. Freeing on the audio thread is as much a violation as allocating, so
  the queue is now a flat `VecDeque<f32>`; draining only moves its head. The C++
  has the same problem via `spsc_queue<std::vector<...>>`.
- `chunked_samples` allocated a chunk whenever a recording grew. It now recycles
  through a spare list, so growth is allocation-free up to the reserve. This is
  what `refilling_pool` was meant to solve; solving it inside the store made the
  pool unnecessary for both this and `buffer_queue`.

Past the reserve, growth still allocates rather than failing. `n_allocations()`
reports it, so a test can assert the reserve was sized correctly instead of the
overrun passing silently.

Scratch buffers are reserved when the graph is applied, not on first use: an idle
playing loop still emits All Sound Off at every wrap, so even a session with no
messages needs room.


The state-restoration work added two findings that the no-alloc harness caught and
code review had not:

- **The standard stable sort allocates.** `sort_by_key` on a slice past its
  insertion-sort threshold (about 20 elements) allocates a scratch buffer, so
  `MidiSortingBuffer::sort` and the dummy port's output sort were both unsafe on the
  audio thread. Every existing test emitted small bursts and so never tripped it. A
  playback state restore emits hundreds of messages at once and does.
  `midi_storage::sort_by_time` is an in-place insertion sort: stable, allocation-free,
  and linear on the nearly-sorted and all-equal-timestamp inputs this actually sees.

- **The MIDI output path has to be sized for a restore burst.** A restore is bounded
  by `midi_state::MAX_DIFF_MESSAGES` (every channel differing in every controller,
  note, program, pressure and pitch wheel: 4144 messages), and nothing on the output
  side refuses an overflow -- a `Vec` just grows. So the channel's restore scratch,
  the session's per-channel output scratch, the sorting buffer and the dummy port's
  output buffers are all reserved from that bound rather than from a round number.

### Sync

Sync reaches a loop as a snapshot rather than a live query. Two consequences worth
knowing:

- Sync cycles are harmless. The C++ `PROC_is_triggering_now` recursed into the
  sync source, so a cycle would recurse until the stack ran out; a snapshot cannot
  recurse. There is therefore no cycle check, deliberately.
- The snapshot must be refreshed at both points in a sub-block, not one. Points of
  interest and trigger ETAs read the source's length, position and ETA before
  loops advance; `handle_sync` reads its `triggering_now` after they have. Refresh
  only once and followers see triggers a sub-block late.

Every loop in a session is co-processed with every other, matching
`BackendSession::recalculate_processing_schedule`, which hands every loop node to
every loop's co-process callback. That is what makes sync correct: all loops reach
the same position before any trigger resolves.

### Divergences the translated tests caught

Translating the C++ suite's own expected values found a real divergence that
reading the implementation had not:

- An unassigned channel buffer contributes a point of interest of **0**, not
  nothing. The C++ `MidiChannel` constructor overrides the header's `= nullopt`
  and builds its buffer state up front with zero frames and a null pointer, so the
  frame accounting always exists while only the pointer gates processing. The audio
  channel is the same by construction, its buffer size simply starting at 0. Mine
  collapsed both into one `Option` and returned `None`, meaning a loop asked to
  record before its channel had a buffer would happily advance a whole cycle into
  nowhere instead of stalling and making the misconfiguration visible.

- **State restoration undoes the drift of the input, not of the output.** This one
  invalidated a whole model, not a single value. My `MidiChannel` diffed
  `output_state` against the record-start snapshot; the C++
  `TrackedRelativeMidiState` tracks the *input* against that snapshot and replays
  the snapshot's values for whatever has since changed. The two differ precisely
  because a recording channel emits nothing of its own: messages that reached the
  receiver while it recorded did so by passing through the port, so only the input
  saw them. Mine would leave a pitch bend or a held pedal stuck after recording.
  The C++ `resolve_to_a` naming is misleading -- it emits the values of the *other*
  tracker -- so this only became clear from `diff->reset(t, state, ...)` fixing
  which tracker is which.

- **Some MIDI state is known before it is ever observed.** The C++ uses sentinels,
  not options, and its defaults are semantic rather than blank: pitch wheel starts
  at centre (`0x2000`) and controllers 64 and 69, the hold pedals, start at 0,
  because a receiver that has been sent none of these is at exactly those values.
  Everything else -- other controllers, program, channel pressure -- starts
  genuinely unknown. Restoration then treats them differently again: an unknown
  controller is sent as 0 anyway (leaving it drifted is worse than guessing
  neutral), while an unknown program or channel pressure is skipped, since there is
  no neutral program to fall back to. My uniform `Option` model emitted nothing in
  all of those cases and so produced no restore at all for the C++ suite's own
  expectations.

- A restore emits **one** message per differing key. Mine sent a note-off/note-on
  pair to change a note's velocity, re-attacking the note twice.

Fixing the buffer-state divergence turned up four of my own tests that had encoded
the wrong behaviour,
including two that were reaching a state the C++ also forbids: a channel's
out-of-bounds error is only reachable through `Replace`, which `next_poi`
deliberately omits. MIDI has no `Replace` path, so a MIDI channel is never asked to
exceed its buffers once the point of interest is respected.

Fixing the state-restoration model turned up six more, all of which had recorded a
note-on with no matching note-off and so had quietly encoded "no restore happens".
Two of them were replaced by tests that pin the real rule from both sides: drifted
state is reverted, undrifted state is left alone.

### Notes

- Logging is absent from the engine so far. `tracing` allocates and locks, so the
  RT path needs a lock-free deferred sink rather than direct `tracing` calls.
- `BasicLoop::process_with` panics when asked to cross its next POI, matching the
  C++ throw. Both are wrong on an audio thread; prefer clamping plus an error
  counter once there is somewhere to report it.
- `BasicLoop` drops the C++ `ma_maybe_next_planned_*` atomic cache and reads the
  planned-transition deque directly, since the engine is single-threaded on the
  process thread.
- `handle_transition`'s `!playing_to_playing` guard is redundant for every real
  target mode: `Stopped` and `Recording` reset position by other means, so it is
  only observable transitioning to `Unknown`.
- Audio playback is bounded by chunk granularity, not exactly by recorded length:
  the length check gates entry to each chunk but not the copy size within it, so
  a loop shorter than one chunk still sounds the whole chunk. Faithful to the C++
  behaviour; likely a latent bug there, worth revisiting once MIDI is done.
- `AudioChannel::next_poi` omits `Replace` from its calculation, as the C++ did,
  so a replacing channel can be handed a longer span than its input buffer holds.
  Here that surfaces as `ReplaceInputOutOfBounds` rather than an overrun.
- `process_replace` uses a saturating subtraction where the C++ subtracted
  unsigned; past the recorded length the C++ wrapped to a huge count and wrote
  outside the recorded region, while this returns `ReplaceOutOfBounds`.
- `MidiChannel::send_all_sound_off` targets channel 0 only, as the C++ does.
  That looks wrong — a hanging note on any other channel is not silenced — but
  it is faithful, so changing it is a deliberate fix to make separately.
- A loop wrap counts as a playback interruption, because `pos_after` is the
  pre-wrap position (`length`) while the next cycle's `pos_before` is 0. So every
  wrap emits All Sound Off. Faithful to the C++, which computes the same way;
  worth revisiting as it may be more than is wanted.
- The group-level `handle_poi` after each sub-block is redundant: `BasicLoop`
  already handles its own point of interest at the end of `process`, in both the
  C++ and here, and a second call is a no-op unless the POI is exactly at zero.
  Kept for faithfulness and because it costs nothing.
- No test drives a loop that reports a point of interest it never clears, so the
  `MAX_SUB_BLOCKS` bound and `n_stuck_cycles` are unexercised. Both are defensive
  against a loop misreporting its POI; the counter exists so such a bug shows up
  as a number rather than as an audible glitch.
- `session::advance_loop` sorts MIDI mappings by channel index, which is currently
  a no-op: channel indices follow add order while nothing can be removed. Kept
  because removal will break that assumption.
- Two `midi_channel` mutants survive as genuine redundancy, not test gaps:
  - `take()` vs `clone()` of the pending playback state. Because restoration is
    computed as `output_state.diff_to(target)`, a second resolution produces an
    empty diff, so clearing the pending state changes nothing observable. It is
    kept because it is clearer and avoids repeated work.
  - The `e.time >= rec.n_frames_processed` lower bound when recording. Reaching it
    requires an unconsumed event older than the current window, which the event
    index normally rules out. Defensive against inconsistent buffer bookkeeping.

### API ordering requirement

Port buffer sizes bound the loop's point of interest, so a cycle must go:
assign buffers on every channel, `resync_poi()`, then `process()`. Skipping the
resync leaves the previous cycle's exhausted POI in place and `process` trips its
own assertion.

`session` now owns this: the `channel::prepare_buffers -> loop::process` graph edge
guarantees every channel of a loop has been given its buffer sizes before the loop
runs, so the resync happens at the start of the loop's node. Callers of `session`
cannot get the order wrong.

## Build environment

`cargo build` needs nightly: `.cargo/config.toml` sets `[unstable] bindeps`.
`rust-toolchain.toml` pins it, so plain `cargo` works without `+nightly`. CI
selects nightly separately in `.github/actions/prepare_build`.

Backend-only builds need no Qt. Deps come from vcpkg into
`build/vcpkg_installed`; `scripts/vcpkg_prebuild.py` is manifest-mode and pulls
Qt, so it is not usable for a backend-only build.

## Pre-existing issues

- `src/rust/common` has four clippy warnings that predate this work and are untouched by
  it: a redundant import, a `'static` lifetime on a constant, an `and_then` that should
  be `map`, and a useless `format!`. Left alone to keep this diff about the port, but
  they are one `cargo clippy --fix -p common` away.

- `vcpkg/ports/vcpkg-tool-meson-test/vcpkg.json` declares the name
  `vcpkg-tool-meson` while sitting in a directory named
  `vcpkg-tool-meson-test`. Current vcpkg rejects the entire overlay-ports
  directory on that mismatch, so the overlays are unusable as-is. Renaming the
  directory re-enables a meson 1.7.2 pin; deleting the port drops the override.
  These differ in intent, so it needs a decision. CI does not hit this because it
  restores prebuilt binaries from a NuGet feed.
- Fixed: the `config` crate used `serde`'s derive macros without declaring the
  `derive` feature, in both `[dependencies]` and `[build-dependencies]` (its
  `build.rs` pulls in `src/config.rs`). It only ever compiled because
  `crashhandling` enables the feature and Cargo unifies features across a
  full-workspace build, so `cargo check -p config` failed on its own. Reproduced
  on pristine `master` before fixing.
- CI depends on that NuGet binary cache, so a fork without credentials cannot
  reproduce the CI build and will surface latent breakage that CI hides. The
  `fmt/core.h` break was one instance.

## The fixed-bar model (current default)

A loop is always exactly one bar long. Every loop shares the bar's position. "Record" is
`LoopMode::Replacing`: it writes over the bar in place, so nothing can come out longer than a bar
and silence overwrites, which makes a second pass an erase rather than a layer.

Pinned by `fixed_bar` in `shoop_gui/tests/end_to_end.rs`; removing the sizing in
`layout::add_loop` kills 5 of those 6.

Nothing waits for a boundary. Record, play and stop all take effect at once, positioned at the bar's
shared cursor via `plan_transition(mode, None, Some(0))`. That is why the `sync` toggle is gone: with
every loop the same length and every cursor the same position, a transition is in time by
construction, so a control for it would control nothing. The boundary wait was also an active bug --
anything played before the boundary arrived was discarded while still being audible through the
monitoring path, so it read as the recording silently failing.

Two consequences left open:

- **`clear` does not zero; `silence` does.** `AudioChannel::clear` is a faithful port of the C++
  `PROC_clear`, which only makes samples addressable. Clearing to a non-zero length therefore leaves
  the old take audible and drawn. `AudioChannel::silence` is the one to use for any fixed length;
  `resize_loop` does. Pinned by `clearing_a_recorded_loop_leaves_silence`.
- **Emptiness is no longer length.** Every loop's length is the bar from creation, so `length > 0`
  cannot mean "has content". Nothing depends on it today (the grid draws a flat line for a silent
  bar, which is honest), but a future "is this loop empty" check needs peak amplitude or explicit
  bookkeeping, not length.
- **Loading a session can produce a loop that is not a bar.** `set_contents` sets the channel length
  from the material. A session saved by this application round-trips, since its material is bar
  length; a hand-written or older one would not. Decide whether load should resize to the bar
  (dropping the tail) or the bar should follow the loaded material.

## Known: adding a track allocates on the audio thread

`add_track` / `add_loop_to_track` are sent as commands, and they create loops, add channels, and
size storage -- all of which allocate. Pre-existing in this GUI, not introduced by the fixed-bar
change, and only reachable by a deliberate button press rather than during play. The fix is to build
the loop off-thread and hand the finished object over, which needs a command that can carry one.

`MAX_BAR_SECONDS` exists because of the same boundary: the bar control only ever shrinks storage
that was sized once at creation. A probe (temporarily asking for 30 s inside the guard) aborts with
`memory allocation of 1024 bytes failed`, which is why the ceiling is enforced rather than advisory.
