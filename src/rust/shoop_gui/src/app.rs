//! The window: a toolbar of global controls over a grid of tracks and loops.
//!
//! Everything the UI knows about the engine comes from the published snapshot, polled
//! once per frame. Everything it asks of the engine goes through queued commands. It
//! never touches the session, which lives on the audio thread.

use crate::click_track::ClickTrack;
use crate::composite::{Composite, CycleCounter, Entry, Playback};
use crate::history::History;
use crate::instrument;
use crate::keyboard;
use crate::midi_control::{ControlAction, Mapping};
use crate::persist::{
    waveform_from_name, waveform_name, SavedInstrument, SavedLoop, SavedSession, SavedTrack,
    FORMAT_VERSION,
};
use crate::script::Script;
use crate::selection::{Cell, Click, Selection};
use crate::session::{self as layout, Layout, N_LOOPS_PER_TRACK, N_TRACKS};
use crate::waveform::{self, Column};

use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::cpal_driver::{
    default_output_device_name, output_device_names, start_output_on_device, CpalDriver, CycleHook,
};
use shoop_engine::driver::{driver_state, Driver};
use shoop_engine::engine::LoopSnapshot;
use shoop_engine::fx_chain::{EffectKind, FxChain};
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::midi;
use shoop_engine::session::Session;
use shoop_engine::wave_generator::Waveform;

use std::collections::HashSet;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;

/// Everything that only exists once the audio device opened.
struct Running {
    driver: CpalDriver,
    keys: instrument::Keys,
    settings: Arc<instrument::Settings>,
    n_voices: Arc<AtomicU32>,
    layout: Layout,
    /// The metronome's own voice, so the click never reaches a loop's input.
    click_keys: instrument::Keys,
}

/// Engine internals worth watching, none of which the ordinary UI surfaces.
///
/// Read from the session rather than from `Stats`, because these are the session's own counters:
/// how finely a cycle had to be split, and whether any cycle gave up.
#[derive(Debug, Clone, Default)]
struct MonitorReading {
    sub_blocks_last_cycle: u32,
    stuck_cycles: u32,
    n_ports: usize,
    n_loops: usize,
    n_channels: usize,
    graph_up_to_date: bool,
}

/// One loop channel's settings, read when the details pane opens.
#[derive(Debug, Clone)]
struct ChannelDetails {
    gain: f32,
    start_offset: i32,
    n_preplay_samples: u32,
    mode: ChannelMode,
    length: usize,
    output_peak: f32,
}

/// An edit made in the details pane.
#[derive(Debug, Clone, Copy)]
enum ChannelEdit {
    Gain(f32),
    StartOffset(i32),
    PrePlay(u32),
    Mode(ChannelMode),
}

/// A track's controls, as the UI holds them.
///
/// Kept here rather than read back from the engine: a slider the user is dragging must
/// not jump because a poll returned a slightly older value.
#[derive(Debug, Clone)]
struct TrackState {
    gain: f32,
    muted: bool,
    /// Last level seen, for the meter.
    peak: f32,
    /// Effect inserted on the track, as the UI holds it.
    fx: EffectKind,
    fx_wet: f32,
    fx_bypassed: bool,
}

impl Default for TrackState {
    fn default() -> Self {
        Self {
            gain: 1.0,
            muted: false,
            peak: 0.0,
            fx: EffectKind::None,
            fx_wet: 1.0,
            fx_bypassed: false,
        }
    }
}

pub struct App {
    running: Result<Running, String>,
    /// Latest published loop state, refreshed each frame.
    loops: Vec<LoopSnapshot>,
    tracks: Vec<TrackState>,
    /// Notes currently held, so key repeat does not retrigger and a release is sent once.
    held: HashSet<u8>,
    octave: i32,
    /// Global toggles, mirroring the existing GUI's toolbar.
    solo_active: bool,
    play_after_record: bool,
    /// Which cells the group actions apply to.
    selection: Selection,
    /// The arrangement, and where it has got to.
    composite: Composite,
    playback: Playback,
    /// Detects sync-loop wraps, which is what advances a composite.
    counter: CycleCounter,
    /// A second counter for the script, so it ticks whether or not a composite is running.
    script_counter: CycleCounter,
    /// Cycles since the application started, which is what a script is told.
    cycle: u32,
    /// The metronome, driven by the sync loop's position.
    click: ClickTrack,
    /// The click note currently sounding, released on the next frame.
    click_held: Option<u8>,
    /// Bar length in seconds, which is what everything aligns to. Held here so the widget does not
    /// fight a poll while it is being dragged.
    bar_seconds: f32,
    /// Reduced waveforms per loop, and the data revision each was made from.
    ///
    /// Cached against the channel's sequence number, so a loop's samples are only fetched when
    /// they actually change. Fetching every frame would mean shipping whole buffers off the audio
    /// thread sixty times a second.
    waveforms: std::collections::HashMap<usize, (u32, Vec<Column>)>,
    /// Columns a waveform is reduced to. Enough for the tile it is drawn in.
    waveform_width: usize,
    /// Frame counter, so waveforms are refreshed on a slower cadence than the UI redraws.
    waveform_tick: u32,
    /// DSP load over time, which shows a periodic spike that a single number hides.
    dsp_history: History,
    /// Loop whose details are shown, if the pane is open.
    details_of: Option<usize>,
    /// Its channel settings, read once when the pane opens.
    channel_details: Option<ChannelDetails>,
    /// Cells the user has retired. The engine keeps their slots, so the UI hides them.
    retired: std::collections::HashSet<Cell>,
    /// Whether the monitoring window is open.
    monitor_open: bool,
    /// Engine internals that only the monitor shows, read on a slow cadence.
    monitor: Option<MonitorReading>,
    /// Bindings from incoming MIDI to actions.
    mapping: Mapping,
    /// A connected controller, if one was opened. Held so it stays connected.
    control_input: Option<(
        shoop_engine::midir_driver::MidiCapture,
        shoop_engine::midir_driver::MidiCaptureConnection,
    )>,
    /// Names of the MIDI inputs offered, for the connections panel.
    control_ports: Vec<String>,
    /// Output devices offered, for the settings panel.
    audio_devices: Vec<String>,
    /// Whether the settings panel is open.
    settings_open: bool,
    /// The scripting engine, if an interpreter could be created.
    script: Option<Script>,
    /// Source being edited, kept here so it survives closing the panel.
    script_source: String,
    script_open: bool,
    status: String,
    /// Where save and load go. A fixed path rather than a file dialog, which would mean
    /// another dependency; shown in the UI so it is never a mystery where a session went.
    session_path: std::path::PathBuf,
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

impl App {
    /// Opens the audio device and builds the pre-configured session.
    ///
    /// A failure to open a device is kept rather than returned: the window should come up
    /// and say so, not fail to appear.
    pub fn new() -> Self {
        let running = Self::start();
        let status = match &running {
            Ok(r) => format!(
                "{} — {} Hz, {} out",
                r.driver.client_name(),
                r.driver.sample_rate(),
                r.driver.n_channels()
            ),
            Err(e) => format!("no audio device: {e}"),
        };
        Self {
            running,
            loops: Vec::new(),
            tracks: vec![TrackState::default(); N_TRACKS],
            held: HashSet::new(),
            octave: 4,
            solo_active: false,
            play_after_record: false,
            selection: Selection::default(),
            composite: Composite::default(),
            playback: Playback::default(),
            counter: CycleCounter::default(),
            script_counter: CycleCounter::default(),
            cycle: 0,
            click: ClickTrack::default(),
            click_held: None,
            bar_seconds: 2.0,
            waveforms: std::collections::HashMap::new(),
            waveform_width: 200,
            waveform_tick: 0,
            dsp_history: History::new(240),
            details_of: None,
            channel_details: None,
            retired: std::collections::HashSet::new(),
            monitor_open: false,
            monitor: None,
            mapping: Mapping::default_for_grid(N_TRACKS, N_LOOPS_PER_TRACK),
            control_input: None,
            control_ports: list_midi_inputs(),
            audio_devices: output_device_names(),
            settings_open: false,
            script: Script::new().ok(),
            script_source: DEFAULT_SCRIPT.to_string(),
            script_open: false,
            status,
            session_path: std::path::PathBuf::from("shoop-session.json"),
        }
    }

    /// Reads the whole session back, for saving.
    ///
    /// One round trip for everything rather than one per loop: sixteen blocking waits
    /// would be noticeable, and a session captured across sixteen moments could be
    /// internally inconsistent.
    fn capture(&mut self) -> Option<SavedSession> {
        let tracks: Vec<SavedTrack> = self
            .tracks
            .iter()
            .map(|t| SavedTrack {
                gain: t.gain,
                muted: t.muted,
            })
            .collect();
        let r = self.running.as_mut().ok()?;
        let grid: Vec<(usize, usize, usize)> = r
            .layout
            .loops
            .iter()
            .map(|l| (l.loop_idx, l.track, l.row))
            .collect();
        let instrument = SavedInstrument {
            waveform: waveform_name(r.settings.waveform()).to_string(),
            gain: r.settings.gain(),
        };
        let sync = r.layout.sync_loop;

        let captured = r
            .driver
            .handle()
            .send_and_wait(
                move |s: &mut Session| {
                    let loops: Vec<SavedLoop> = grid
                        .iter()
                        .map(|&(idx, track, row)| {
                            let (length, samples) = s
                                .loop_(idx)
                                .and_then(|l| l.audio_channel(0))
                                .map(|c| (c.length() as u32, c.data()))
                                .unwrap_or((0, Vec::new()));
                            SavedLoop {
                                track,
                                row,
                                length,
                                samples,
                            }
                        })
                        .collect();
                    let sync_length = s.loop_(sync).map(|l| l.length()).unwrap_or(0);
                    (loops, sync_length, s.sample_rate())
                },
                std::time::Duration::from_secs(5),
            )
            .ok()?;

        let (loops, sync_length, sample_rate) = captured;
        Some(SavedSession {
            version: FORMAT_VERSION,
            sample_rate,
            sync_length,
            tracks,
            loops,
            instrument,
        })
    }

    /// Puts a saved session back.
    ///
    /// Loops are matched by grid position, so a file written by a build with a different
    /// creation order still lands in the right cells. Anything the grid does not have a
    /// cell for is ignored rather than refused.
    fn restore(&mut self, saved: SavedSession) {
        for (i, t) in saved.tracks.iter().enumerate() {
            if let Some(slot) = self.tracks.get_mut(i) {
                slot.gain = t.gain;
                slot.muted = t.muted;
            }
        }
        for i in 0..self.tracks.len() {
            self.push_track(i);
        }

        if let Ok(r) = &mut self.running {
            r.settings
                .set_waveform(waveform_from_name(&saved.instrument.waveform));
            r.settings.set_gain(saved.instrument.gain);

            let mut work: Vec<(usize, u32, Vec<f32>)> = Vec::new();
            for l in &saved.loops {
                if let Some(target) = r.layout.loop_at(l.track, l.row) {
                    work.push((target.loop_idx, l.length, l.samples.clone()));
                }
            }
            let sync = r.layout.sync_loop;
            let sync_length = saved.sync_length;

            let _ = r.driver.handle().send(Box::new(move |s: &mut Session| {
                for (idx, length, samples) in &work {
                    // Stopped first: loading underneath a playing loop would jump the
                    // position into material that is no longer there.
                    let _ = s.set_loop_mode(*idx, LoopMode::Stopped);
                    if let Some(c) = s.loop_mut(*idx).and_then(|l| l.audio_channel_mut(0)) {
                        c.load_data(samples);
                    }
                    if let Some(l) = s.loop_mut(*idx) {
                        l.set_length(*length);
                    }
                }
                if sync_length > 0 {
                    if let Some(l) = s.loop_mut(sync) {
                        l.set_length(sync_length);
                    }
                }
            }));
        }
    }

    fn start() -> Result<Running, String> {
        Self::start_on(None)
    }

    /// Starts on a named device, or the default when given none.
    fn start_on(device: Option<String>) -> Result<Running, String> {
        let (driver, built) =
            start_output_on_device(Session::default(), 256, device, |session, outs| {
                let layout = layout::configure(session, outs);
                let (keys, mut voice, settings, n_voices) =
                    instrument::split(layout.instrument_port, session.sample_rate());
                voice.set_sample_rate(session.sample_rate());

                // A second voice for the metronome, on its own port: sharing the instrument's put
                // the click on the port the loops record from, so every take captured it.
                let (click_keys, mut click_voice, click_settings, _) =
                    instrument::split(layout.click_port, session.sample_rate());
                click_voice.set_sample_rate(session.sample_rate());
                // A square carries through a mix where a sine disappears, which is the whole job of
                // a metronome. Set once: the click's level is the beat velocity, not this gain.
                click_settings.set_waveform(Waveform::Square);
                click_settings.set_gain(0.3);

                let hook: CycleHook = Box::new(move |s: &mut Session, n: usize| {
                    voice.render_into(s, n);
                    click_voice.render_into(s, n);
                });
                Ok(((keys, settings, n_voices, layout, click_keys), hook))
            })
            .map_err(|e| e.to_string())?;

        let (keys, settings, n_voices, layout, click_keys) = built;
        Ok(Running {
            driver,
            keys,
            settings,
            n_voices,
            layout,
            click_keys,
        })
    }

    /// Queues work against one loop.
    fn with_loop(&mut self, loop_idx: usize, f: impl FnMut(&mut Session) + Send + 'static) {
        let _ = loop_idx;
        if let Ok(r) = &mut self.running {
            let mut f = f;
            let _ = r
                .driver
                .handle()
                .send(Box::new(move |s: &mut Session| f(s)));
        }
    }

    /// Records into a loop by replacing its bar in place.
    ///
    /// `Replacing`, not `Recording`: a loop is always exactly one bar long, and playing into it
    /// overwrites those frames at the shared bar position rather than starting a new take that
    /// grows to whatever length the player held the key down for. Two consequences worth stating,
    /// because they are the point of the model: nothing can come out longer than a bar, and silence
    /// overwrites too -- so a second pass over a bar erases what was there instead of layering on
    /// top of it.
    ///
    /// With sync on the change lands on the bar boundary, which is what keeps takes aligned; with
    /// it off it happens now and the take starts mid-bar.
    fn record(&mut self, loop_idx: usize) {
        self.with_loop(loop_idx, move |s: &mut Session| {
            if let Some(l) = s.loop_mut(loop_idx) {
                // `to_sync_cycle = Some(0)`: start now, at the position the bar is already at.
                l.plan_transition(LoopMode::Replacing, None, Some(0));
            }
        });
    }

    /// Plays a loop from the bar's shared position, so it is in time with whatever else is playing.
    fn play(&mut self, loop_idx: usize) {
        let solo = self.solo_active;
        let others: Vec<usize> = if solo {
            self.loop_indices()
                .into_iter()
                .filter(|i| *i != loop_idx)
                .collect()
        } else {
            Vec::new()
        };
        self.with_loop(loop_idx, move |s: &mut Session| {
            // Solo stops everything else first, so only one take is heard.
            for other in &others {
                let _ = s.set_loop_mode(*other, LoopMode::Stopped);
            }
            if let Some(l) = s.loop_mut(loop_idx) {
                l.plan_transition(LoopMode::Playing, None, Some(0));
            }
        });
    }

    /// Stops a loop at once, keeping whatever was written.
    ///
    /// Never planned: a loop's length is a bar by construction, so waiting for the boundary cannot
    /// improve the alignment and only makes the button look broken. Anything still queued is dropped
    /// so nothing restarts afterwards.
    ///
    /// With `play after record` on, stopping a take plays it instead of stopping dead -- which is
    /// what the toggle's name says, and it stays in time because leaving `Replacing` for `Playing`
    /// does not move the position.
    fn stop(&mut self, loop_idx: usize) {
        let after = self.play_after_record;
        self.with_loop(loop_idx, move |s: &mut Session| {
            let was_recording = s.loop_(loop_idx).map(|l| l.mode()) == Some(LoopMode::Replacing);
            if let Some(l) = s.loop_mut(loop_idx) {
                l.clear_planned_transitions();
            }
            let next = if was_recording && after {
                LoopMode::Playing
            } else {
                LoopMode::Stopped
            };
            let _ = s.set_loop_mode(loop_idx, next);
        });
    }

    /// Silences a loop's bar, keeping its length.
    ///
    /// Not `clear(0)`: a loop is always one bar long, and a zero-length one could not be played
    /// into again. So clearing means filling the bar with silence, which is also what a fresh loop
    /// holds.
    fn clear(&mut self, loop_idx: usize) {
        let bar = self.bar_frames();
        self.with_loop(loop_idx, move |s: &mut Session| {
            let _ = s.resize_loop(loop_idx, bar);
        });
        // Dropped now rather than at the next poll, so the tile updates on the click that caused it.
        self.waveforms.remove(&loop_idx);
    }

    fn stop_all(&mut self) {
        for idx in self.loop_indices() {
            self.stop(idx);
        }
    }

    /// Loop indices for the current selection, in grid order.
    fn selected_loops(&self) -> Vec<usize> {
        let Ok(r) = &self.running else {
            return Vec::new();
        };
        self.selection
            .cells()
            .filter_map(|c| r.layout.loop_at(c.track, c.row))
            .map(|l| l.loop_idx)
            .collect()
    }

    /// Turns the selection into an arrangement, one member per cycle in grid order.
    ///
    /// A sequence rather than a chord: playing them all at once is what the group action
    /// already does, so a composite built from a selection is most useful as a sequence.
    fn build_composite_from_selection(&mut self) {
        let cells: Vec<Cell> = self.selection.cells().copied().collect();
        self.composite = Composite {
            name: format!("{} loops", cells.len()),
            entries: cells
                .into_iter()
                .enumerate()
                .map(|(i, cell)| Entry::new(cell, i as u32, 1))
                .collect(),
        };
        self.playback = Playback::default();
    }

    /// Issues the play and stop commands a composite's cycle calls for.
    fn apply_composite(&mut self, starts: Vec<Cell>, stops: Vec<Cell>) {
        for cell in stops {
            if let Some(idx) = self.loop_at_cell(cell) {
                self.stop(idx);
            }
        }
        for cell in starts {
            if let Some(idx) = self.loop_at_cell(cell) {
                // Immediate, not planned: the composite is already aligned to the sync
                // loop's boundary, and planning again would delay it by another cycle.
                self.with_loop(idx, move |s: &mut Session| {
                    let _ = s.set_loop_mode(idx, LoopMode::Playing);
                });
            }
        }
    }

    fn loop_at_cell(&self, cell: Cell) -> Option<usize> {
        self.running
            .as_ref()
            .ok()?
            .layout
            .loop_at(cell.track, cell.row)
            .map(|l| l.loop_idx)
    }

    /// Restarts the engine on a different audio device.
    ///
    /// The session is rebuilt rather than moved across: ports belong to the device, so a
    /// new device means new ports and a new graph. Loop contents are carried over, since
    /// losing a take to a device change would be unforgivable.
    fn switch_device(&mut self, device: String) {
        let carried = self.capture();

        // Dropped before the new one opens, so the old device is released first.
        self.running = Err("restarting".to_string());
        self.held.clear();

        match Self::start_on(Some(device.clone())) {
            Ok(r) => {
                self.running = Ok(r);
                self.status = format!("switched to {device}");
                if let Some(session) = carried {
                    self.restore(session);
                }
            }
            Err(e) => {
                self.status = format!("could not open {device}: {e}");
                self.running = Err(e);
            }
        }
        self.counter.reset();
    }

    /// Opens a MIDI input by name, replacing whatever was connected.
    fn connect_control(&mut self, pattern: &str) {
        // Dropped first, so a controller can be reconnected without restarting.
        self.control_input = None;
        match shoop_engine::midir_driver::open_input("shoop-control", "control", pattern) {
            Ok((capture, conn)) => {
                self.control_input = Some((capture, conn));
                self.status = format!("control: {pattern}");
            }
            Err(e) => self.status = format!("could not open {pattern}: {e}"),
        }
    }

    /// Applies whatever a connected controller sent.
    ///
    /// Drained here rather than in the audio callback: these are commands, not notes, so a
    /// buffer of latency is irrelevant and doing it on this thread keeps the callback free
    /// of the mapping.
    fn poll_control(&mut self) {
        let mut fired: Vec<crate::midi_control::Fired> = Vec::new();
        if let Some((capture, _)) = self.control_input.as_mut() {
            // Borrowed through a scratch port, because `MidiCapture` hands messages over by
            // staging them into one.
            let mut scratch = shoop_engine::external_midi_port::ExternalMidiPort::new(
                "control-scratch",
                shoop_engine::port::PortDirection::Input,
            );
            capture.drain_into(&mut scratch);
            scratch.prepare(0);
            for e in scratch.visible_events() {
                fired.extend(self.mapping.resolve(e.data()));
            }
        }

        for f in fired {
            match f.action {
                ControlAction::Record(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.record(i);
                    }
                }
                ControlAction::Play(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.play(i);
                    }
                }
                ControlAction::Stop(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.stop(i);
                    }
                }
                ControlAction::Clear(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.clear(i);
                    }
                }
                ControlAction::ToggleTrackMute(track) => {
                    if let Some(t) = self.tracks.get_mut(track) {
                        t.muted = !t.muted;
                    }
                    self.push_track(track);
                }
                ControlAction::SetTrackGain(track) => {
                    if let (Some(t), Some(v)) = (self.tracks.get_mut(track), f.value) {
                        // Scaled to the slider's range, so a controller reaches the same
                        // maximum the UI offers.
                        t.gain = v * 2.0;
                    }
                    self.push_track(track);
                }
                ControlAction::StopAll => self.stop_all(),
                ControlAction::RunComposite => {
                    self.counter.reset();
                    let composite = self.composite.clone();
                    let (starts, stops) = self.playback.start(&composite);
                    self.apply_composite(starts, stops);
                }
                ControlAction::HaltComposite => {
                    let composite = self.composite.clone();
                    let stops = self.playback.stop(&composite);
                    self.apply_composite(Vec::new(), stops);
                }
            }
        }
    }

    /// Runs everything a script asked for.
    ///
    /// The same actions MIDI produces, applied by the same code: a script can ask for what a
    /// user or a controller could ask for, and nothing else.
    fn apply_actions(&mut self, actions: Vec<ControlAction>) {
        for action in actions {
            match action {
                ControlAction::Record(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.record(i);
                    }
                }
                ControlAction::Play(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.play(i);
                    }
                }
                ControlAction::Stop(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.stop(i);
                    }
                }
                ControlAction::Clear(cell) => {
                    if let Some(i) = self.loop_at_cell(cell) {
                        self.clear(i);
                    }
                }
                ControlAction::ToggleTrackMute(track) => {
                    if let Some(t) = self.tracks.get_mut(track) {
                        t.muted = !t.muted;
                    }
                    self.push_track(track);
                }
                ControlAction::SetTrackGain(_) => {
                    // A script sets a level through its own call, which carries the value;
                    // this variant exists for MIDI, where the value is in the message.
                }
                ControlAction::StopAll => self.stop_all(),
                ControlAction::RunComposite => {
                    self.counter.reset();
                    let composite = self.composite.clone();
                    let (starts, stops) = self.playback.start(&composite);
                    self.apply_composite(starts, stops);
                }
                ControlAction::HaltComposite => {
                    let composite = self.composite.clone();
                    let stops = self.playback.stop(&composite);
                    self.apply_composite(Vec::new(), stops);
                }
            }
        }
    }

    /// Advances the composite when the sync loop wraps.
    fn drive_composite(&mut self) {
        if !self.playback.is_running() {
            return;
        }
        let Some(sync) = self.running.as_ref().ok().map(|r| r.layout.sync_loop) else {
            return;
        };
        let Some(position) = self.loops.get(sync).map(|l| l.position) else {
            return;
        };
        if self.counter.update(position) {
            self.cycle += 1;
            let composite = self.composite.clone();
            let (starts, stops) = self.playback.advance(&composite);
            self.apply_composite(starts, stops);
        }
    }

    /// Adds a track, with as many loops as the widest track already has.
    ///
    /// Matching the existing width rather than a fixed number, so a new track lines up with the
    /// grid instead of leaving a ragged column.
    fn add_track(&mut self) {
        let rows = self
            .running
            .as_ref()
            .ok()
            .map(|r| r.layout.max_rows().max(1))
            .unwrap_or(1);
        // The layout is the UI's copy, so it is updated here and the session work is queued.
        let Ok(r) = &mut self.running else { return };
        let mut layout = r.layout.clone();
        let _ = r
            .driver
            .handle()
            .send_and_wait(
                move |s: &mut Session| {
                    layout::add_track(s, &mut layout, rows);
                    layout
                },
                std::time::Duration::from_millis(500),
            )
            .map(|updated| r.layout = updated);
        self.tracks.push(TrackState::default());
    }

    /// Adds a loop to the end of a track.
    fn add_loop(&mut self, track: usize) {
        let Ok(r) = &mut self.running else { return };
        let mut layout = r.layout.clone();
        let _ = r
            .driver
            .handle()
            .send_and_wait(
                move |s: &mut Session| {
                    layout::add_loop_to_track(s, &mut layout, track);
                    layout
                },
                std::time::Duration::from_millis(500),
            )
            .map(|updated| r.layout = updated);
    }

    /// Retires a loop: made inert in the engine, and hidden here.
    ///
    /// The engine keeps the slot so no other index moves, so the UI is what remembers the cell is
    /// gone. Retiring also drops it from the selection and from any arrangement, since acting on a
    /// hidden loop would be a surprise.
    fn retire(&mut self, cell: Cell, loop_idx: usize) {
        self.with_loop(loop_idx, move |s: &mut Session| {
            if s.remove_loop(loop_idx).is_ok() {
                let _ = s.apply_graph_changes();
            }
        });
        self.retired.insert(cell);
        self.selection.click(cell, Click::Toggle);
        if self.selection.contains(cell) {
            // The toggle added it rather than removing it, so undo that.
            self.selection.click(cell, Click::Toggle);
        }
        self.composite.entries.retain(|e| e.cell != cell);
        self.waveforms.remove(&loop_idx);
        if self.details_of == Some(loop_idx) {
            self.details_of = None;
        }
    }

    /// Brings every retired cell back, since the engine never discarded them.
    fn restore_retired(&mut self) {
        let cells: Vec<Cell> = self.retired.drain().collect();
        for cell in cells {
            if let Some(idx) = self.loop_at_cell(cell) {
                // Re-enabled by putting its channel back into Direct mode; removal only disabled
                // it, so nothing has to be rebuilt.
                self.with_loop(idx, move |s: &mut Session| {
                    if let Some(c) = s.loop_mut(idx).and_then(|l| l.audio_channel_mut(0)) {
                        c.set_mode(ChannelMode::Direct);
                    }
                });
            }
        }
    }

    /// Reads the engine's internal counters.
    fn read_monitor(&mut self) {
        let Ok(r) = &mut self.running else { return };
        self.monitor = r
            .driver
            .handle()
            .send_and_wait(
                |s: &mut Session| MonitorReading {
                    sub_blocks_last_cycle: s.n_sub_blocks_last_cycle(),
                    stuck_cycles: s.n_stuck_cycles(),
                    n_ports: s.n_ports(),
                    n_loops: s.n_loops(),
                    n_channels: s.n_channels(),
                    graph_up_to_date: s.graph_up_to_date(),
                },
                std::time::Duration::from_millis(200),
            )
            .ok();
    }

    /// The monitoring window: what the engine is doing, rather than what the music is doing.
    fn monitor_window(&mut self, ctx: &egui::Context) {
        let mut open = self.monitor_open;
        egui::Window::new("engine monitor")
            .open(&mut open)
            .default_pos([560.0, 110.0])
            .default_width(340.0)
            .show(ctx, |ui| {
                let Ok(r) = &self.running else {
                    ui.weak("not running");
                    return;
                };
                let st = driver_state(&r.driver);

                egui::Grid::new("monitor-grid")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let mut row = |label: &str, value: String| {
                            ui.label(label);
                            ui.monospace(value);
                            ui.end_row();
                        };
                        row("driver", st.instance_name.clone());
                        row("sample rate", format!("{} Hz", st.sample_rate));
                        row("buffer size", format!("{} frames", st.buffer_size));
                        row("cycles", st.last_processed.to_string());
                        row("dsp load", format!("{:.1} %", st.dsp_load_percent));
                        row("xruns", st.xruns.to_string());
                    });

                ui.separator();
                let stats = r.driver.stats();
                egui::Grid::new("monitor-engine")
                    .num_columns(2)
                    .show(ui, |ui| {
                        let mut row = |label: &str, value: String| {
                            ui.label(label);
                            ui.monospace(value);
                            ui.end_row();
                        };
                        row(
                            "commands applied",
                            stats.commands_applied.load(Ordering::Relaxed).to_string(),
                        );
                        // A refused cycle means the graph was stale when the callback ran, which
                        // sounds like silence and is otherwise invisible.
                        row(
                            "refused cycles",
                            stats.refused_cycles.load(Ordering::Relaxed).to_string(),
                        );
                        row(
                            "capture underruns",
                            stats.capture_underruns.load(Ordering::Relaxed).to_string(),
                        );
                        row(
                            "capture overruns",
                            stats.capture_overruns.load(Ordering::Relaxed).to_string(),
                        );
                    });

                if let Some(m) = &self.monitor {
                    ui.separator();
                    egui::Grid::new("monitor-session")
                        .num_columns(2)
                        .show(ui, |ui| {
                            let mut row = |label: &str, value: String| {
                                ui.label(label);
                                ui.monospace(value);
                                ui.end_row();
                            };
                            // How finely the last cycle had to be split. Rising with the number of
                            // loops is expected; rising without cause means points of interest are
                            // landing oddly.
                            row("sub-blocks last cycle", m.sub_blocks_last_cycle.to_string());
                            row("stuck cycles", m.stuck_cycles.to_string());
                            row("ports", m.n_ports.to_string());
                            row("loops", m.n_loops.to_string());
                            row("channels", m.n_channels.to_string());
                            row(
                                "graph",
                                if m.graph_up_to_date {
                                    "current".into()
                                } else {
                                    "stale".to_string()
                                },
                            );
                        });

                    if m.stuck_cycles > 0 {
                        // Worth shouting about: a stuck cycle is silence, and nothing else in the
                        // UI would show why.
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 80, 80),
                            "cycles have given up: a loop is shorter than the cycle can be split",
                        );
                    }
                }
            });
        self.monitor_open = open;
    }

    /// Draws the per-channel settings of one loop, and applies edits.
    ///
    /// Read once when the pane opens rather than polled: these are settings a user changes, not
    /// state that moves, and re-reading every frame would fight the widgets.
    fn details_pane(&mut self, ctx: &egui::Context, loop_idx: usize) {
        let mut open = true;
        let mut edits: Vec<ChannelEdit> = Vec::new();
        let current = self.channel_details.clone();

        egui::Window::new(format!("loop {loop_idx} details"))
            .open(&mut open)
            .default_width(320.0)
            .show(ctx, |ui| match &current {
                None => {
                    ui.weak("reading...");
                }
                Some(d) => {
                    ui.label(format!("length {} frames", d.length));
                    ui.separator();

                    let mut gain = d.gain;
                    if ui
                        .add(egui::Slider::new(&mut gain, 0.0f32..=2.0f32).text("channel gain"))
                        .changed()
                    {
                        edits.push(ChannelEdit::Gain(gain));
                    }

                    let mut offset = d.start_offset;
                    if ui
                        .add(
                            egui::DragValue::new(&mut offset)
                                .prefix("start offset ")
                                .speed(16.0),
                        )
                        .changed()
                    {
                        edits.push(ChannelEdit::StartOffset(offset));
                    }

                    let mut preplay = d.n_preplay_samples;
                    if ui
                        .add(
                            egui::DragValue::new(&mut preplay)
                                .prefix("pre-play ")
                                .speed(16.0),
                        )
                        .changed()
                    {
                        edits.push(ChannelEdit::PrePlay(preplay));
                    }

                    ui.separator();
                    ui.label("mode");
                    for mode in [
                        ChannelMode::Direct,
                        ChannelMode::Dry,
                        ChannelMode::Wet,
                        ChannelMode::Disabled,
                    ] {
                        if ui
                            .selectable_label(d.mode == mode, format!("{mode:?}"))
                            .clicked()
                        {
                            edits.push(ChannelEdit::Mode(mode));
                        }
                    }
                    ui.separator();
                    ui.weak(format!("output peak {:.3}", d.output_peak));
                }
            });

        for edit in edits {
            self.apply_channel_edit(loop_idx, edit);
        }
        if !open {
            self.details_of = None;
            self.channel_details = None;
        }
    }

    /// Reads the selected loop's channel settings.
    fn read_channel_details(&mut self, loop_idx: usize) {
        let Ok(r) = &mut self.running else { return };
        self.channel_details = r
            .driver
            .handle()
            .send_and_wait(
                move |s: &mut Session| {
                    s.loop_(loop_idx)
                        .and_then(|l| l.audio_channel(0))
                        .map(|c| ChannelDetails {
                            gain: c.gain(),
                            start_offset: c.start_offset(),
                            n_preplay_samples: c.pre_play_samples(),
                            mode: c.mode(),
                            length: c.length(),
                            output_peak: c.output_peak(),
                        })
                },
                std::time::Duration::from_millis(200),
            )
            .ok()
            .flatten();
    }

    fn apply_channel_edit(&mut self, loop_idx: usize, edit: ChannelEdit) {
        // Applied to the cached copy as well, so the widget does not snap back before the next
        // read.
        if let Some(d) = self.channel_details.as_mut() {
            match edit {
                ChannelEdit::Gain(v) => d.gain = v,
                ChannelEdit::StartOffset(v) => d.start_offset = v,
                ChannelEdit::PrePlay(v) => d.n_preplay_samples = v,
                ChannelEdit::Mode(m) => d.mode = m,
            }
        }
        self.with_loop(loop_idx, move |s: &mut Session| {
            if let Some(c) = s.loop_mut(loop_idx).and_then(|l| l.audio_channel_mut(0)) {
                match edit {
                    ChannelEdit::Gain(v) => c.set_gain(v),
                    ChannelEdit::StartOffset(v) => c.set_start_offset(v),
                    ChannelEdit::PrePlay(v) => c.set_pre_play_samples(v),
                    ChannelEdit::Mode(m) => c.set_mode(m),
                }
            }
        });
    }

    /// Fetches and reduces any loop whose contents have changed.
    ///
    /// One blocking round trip covering every stale loop, rather than one each: the samples have
    /// to come from the audio thread's side, and asking sixteen times would be sixteen waits.
    fn refresh_waveforms(&mut self) {
        let Ok(r) = &mut self.running else { return };
        let cells: Vec<usize> = r.layout.loops.iter().map(|l| l.loop_idx).collect();
        let known: Vec<(usize, u32)> = cells
            .iter()
            .map(|&i| (i, self.waveforms.get(&i).map(|(rev, _)| *rev).unwrap_or(0)))
            .collect();

        let width = self.waveform_width;
        let fetched = r.driver.handle().send_and_wait(
            move |s: &mut Session| {
                let mut out: Vec<(usize, u32, Vec<f32>)> = Vec::new();
                for (idx, known_rev) in known {
                    if let Some(c) = s.loop_(idx).and_then(|l| l.audio_channel(0)) {
                        let rev = c.data_seq_nr();
                        if rev != known_rev {
                            // An emptied loop is reported as empty rather than skipped. Skipping it
                            // was a bug: clearing leaves length at zero, so the stale picture stayed
                            // on screen and the loop looked untouched.
                            let mut data = c.data();
                            data.truncate(c.length());
                            out.push((idx, rev, data));
                        }
                    }
                }
                out
            },
            std::time::Duration::from_millis(200),
        );

        if let Ok(fetched) = fetched {
            for (idx, rev, data) in fetched {
                if data.is_empty() {
                    // Nothing to draw, so the tile falls back to showing "empty".
                    self.waveforms.remove(&idx);
                } else {
                    self.waveforms
                        .insert(idx, (rev, waveform::reduce(&data, width)));
                }
            }
        }
    }

    /// Draws a loop's waveform, with a line marking where playback has reached.
    fn draw_waveform(&self, ui: &mut egui::Ui, loop_idx: usize, position: u32, length: u32) {
        let height = 34.0f32;
        let (rect, _) = ui.allocate_exact_size(
            egui::vec2(ui.available_width().min(210.0), height),
            egui::Sense::hover(),
        );
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));

        let Some((_, columns)) = self.waveforms.get(&loop_idx) else {
            return;
        };
        if columns.is_empty() {
            return;
        }
        // Normalised to its own peak, so a quiet take is still visible. Absolute levels are what
        // the track meter is for.
        let scale = waveform::peak(columns).unwrap_or(1.0);
        let mid = rect.center().y;
        let half = height / 2.0 - 2.0;
        let step = rect.width() / columns.len() as f32;

        for (i, c) in columns.iter().enumerate() {
            let x = rect.left() + i as f32 * step;
            let top = mid - (c.max / scale) * half;
            let bottom = mid - (c.min / scale) * half;
            painter.line_segment(
                [egui::pos2(x, top), egui::pos2(x, bottom)],
                egui::Stroke::new(1.0f32, egui::Color32::from_rgb(90, 160, 200)),
            );
        }

        if length > 0 {
            let fraction = (position as f32 / length as f32).clamp(0.0, 1.0);
            let x = rect.left() + rect.width() * fraction;
            painter.line_segment(
                [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
                egui::Stroke::new(1.0f32, egui::Color32::from_rgb(230, 200, 90)),
            );
        }
    }

    /// Where the bar is, as a beat count and a filling strip.
    ///
    /// Without this the bar is invisible: a planned change waits for a boundary the player cannot
    /// see, so the wait reads as the application ignoring the click. The click track is the audible
    /// half of the same problem and is on by default for the same reason.
    fn bar_cursor(&mut self, ui: &mut egui::Ui) {
        let (position, length) = self
            .running
            .as_ref()
            .ok()
            .map(|r| r.layout.sync_loop)
            .and_then(|sync| self.loops.get(sync).map(|l| (l.position, l.length)))
            .unwrap_or((0, 0));

        let fraction = if length > 0 {
            position as f32 / length as f32
        } else {
            0.0
        };
        let beats = self.click.beats_per_bar.max(1);
        // 1-based, as a musician counts.
        let beat = (fraction * beats as f32).floor() as u32 + 1;

        ui.label(format!("bar {beat}/{beats}"));
        ui.add(
            egui::ProgressBar::new(fraction)
                .desired_width(120.0)
                .show_percentage(),
        )
        .on_hover_text("Position within the bar. Every loop shares it.");
    }

    /// The bar in frames, which is every loop's length.
    fn bar_frames(&self) -> u32 {
        let Ok(r) = &self.running else { return 0 };
        self.loops
            .get(r.layout.sync_loop)
            .map(|l| l.length)
            .unwrap_or(0)
    }

    /// Sets the bar everything aligns to.
    ///
    /// Applied to the sync loop's length, which is what "one cycle" means everywhere else -- so this
    /// is the tempo control, and the click track divides it into beats.
    fn push_bar_length(&mut self) {
        let seconds = self.bar_seconds.clamp(0.25, layout::MAX_BAR_SECONDS as f32);
        let Ok(r) = &mut self.running else { return };
        let sync = r.layout.sync_loop;
        let rate = r.driver.sample_rate();
        let frames = ((seconds * rate as f32) as u32).max(1);
        let loops: Vec<usize> = r.layout.loops.iter().map(|l| l.loop_idx).collect();
        let _ = r.driver.handle().send(Box::new(move |s: &mut Session| {
            if let Some(l) = s.loop_mut(sync) {
                l.set_length(frames);
            }
            // Every loop is one bar long by definition, so a new bar resizes them all. It also
            // empties them: the old contents no longer fit the grid they were played to.
            for &idx in &loops {
                let _ = s.resize_loop(idx, frames);
            }
        }));
        self.click.reset();
    }

    /// Sounds the metronome when the sync loop crosses a beat.
    ///
    /// Played through the instrument, so it mixes with everything else and needs no port of its
    /// own. A short note, released immediately, because a click is a transient.
    fn drive_click(&mut self) {
        // Last frame's click, released now rather than when it was sent. Sending both together gave
        // them the same timestamp, so the instrument released the voice before its attack had
        // produced anything and the metronome was silent -- see
        // `a_note_released_at_the_same_timestamp_makes_no_sound` in `wave_generator.rs`. One frame is
        // roughly 16 ms, which is a click.
        if let Some(note) = self.click_held.take() {
            if let Ok(r) = &mut self.running {
                r.click_keys.send(&midi::note_off(0, note, 64));
            }
        }

        let Some((sync, length)) = self
            .running
            .as_ref()
            .ok()
            .map(|r| r.layout.sync_loop)
            .and_then(|sync| self.loops.get(sync).map(|l| (l.position, l.length)))
        else {
            return;
        };
        let Some(click) = self.click.poll(sync, length) else {
            return;
        };
        let note = self.click.note_for(click);
        let velocity = self.click.velocity;
        if let Ok(r) = &mut self.running {
            r.click_keys.send(&midi::note_on(0, note, velocity));
            self.click_held = Some(note);
        }
    }

    /// Ticks the script on a sync-loop wrap, and applies what it asked for.
    ///
    /// Separate from `drive_composite` because a script should run whether or not an
    /// arrangement is playing, and the two must not interfere.
    fn drive_script(&mut self) {
        let Some(sync) = self.running.as_ref().ok().map(|r| r.layout.sync_loop) else {
            return;
        };
        let Some(position) = self.loops.get(sync).map(|l| l.position) else {
            return;
        };
        // Its own counter, so ticking the script does not depend on a composite running.
        if !self.script_counter.update(position) {
            return;
        }
        let cycle = self.cycle;
        let actions = match self.script.as_mut() {
            Some(s) => {
                s.tick(cycle);
                s.drain()
            }
            None => Vec::new(),
        };
        self.apply_actions(actions);
    }

    /// Applies one of the transport actions to every selected loop.
    ///
    /// Sent as separate commands rather than one: they are applied in order within a
    /// cycle, so the group still lands together, and one oversized command is harder to
    /// bound than several small ones.
    fn apply_to_selection(&mut self, action: Action) {
        for idx in self.selected_loops() {
            match action {
                Action::Record => self.record(idx),
                Action::Play => self.play(idx),
                Action::Stop => self.stop(idx),
                Action::Clear => self.clear(idx),
            }
        }
    }

    /// Applies a track's effect choice to its port.
    ///
    /// The chain is created on the audio thread's side but by a queued command, which runs between
    /// cycles rather than during one, so allocating a delay line there is safe.
    fn push_track_fx(&mut self, track: usize) {
        let Some(state) = self.tracks.get(track).cloned() else {
            return;
        };
        let Ok(r) = &mut self.running else { return };
        let Some(&port) = r.layout.track_ports.get(track) else {
            return;
        };
        let rate = r.driver.sample_rate();
        let _ = r.driver.handle().send(Box::new(move |s: &mut Session| {
            let Some(a) = s.port_mut(port).and_then(|p| p.audio_mut()) else {
                return;
            };
            if state.fx == EffectKind::None {
                a.set_fx(None);
                return;
            }
            if a.fx().is_none() {
                let mut chain = FxChain::default();
                chain.configure(rate);
                a.set_fx(Some(chain));
            }
            if let Some(chain) = a.fx_mut() {
                chain.set_kind(state.fx);
                chain.set_wet(state.fx_wet);
                chain.set_bypassed(state.fx_bypassed);
            }
        }));
    }

    /// Applies a track's gain and muting to its port.
    fn push_track(&mut self, track: usize) {
        let Some(state) = self.tracks.get(track).cloned() else {
            return;
        };
        let Ok(r) = &mut self.running else { return };
        let Some(&port) = r.layout.track_ports.get(track) else {
            return;
        };
        let _ = r.driver.handle().send(Box::new(move |s: &mut Session| {
            if let Some(a) = s.port_mut(port).and_then(|p| p.audio_mut()) {
                a.set_gain(state.gain);
                a.set_muted(state.muted);
            }
        }));
    }

    /// Reads each track's level and resets it, so the meter shows this interval's peak.
    ///
    /// Blocking once per frame rather than per track: one round trip is cheap, sixteen
    /// would not be.
    fn poll_meters(&mut self) {
        let Ok(r) = &mut self.running else { return };
        let ports = r.layout.track_ports.clone();
        let peaks = r.driver.handle().send_and_wait(
            move |s: &mut Session| {
                ports
                    .iter()
                    .map(|&p| {
                        let peak = s
                            .port(p)
                            .and_then(|p| p.audio())
                            .map(|a| a.output_peak())
                            .unwrap_or(0.0);
                        // Reset, so the next reading is the next interval rather than the
                        // loudest moment since the application started.
                        if let Some(a) = s.port_mut(p).and_then(|p| p.audio_mut()) {
                            a.reset_output_peak();
                        }
                        peak
                    })
                    .collect::<Vec<f32>>()
            },
            std::time::Duration::from_millis(100),
        );
        if let Ok(peaks) = peaks {
            for (t, peak) in peaks.into_iter().enumerate() {
                if let Some(s) = self.tracks.get_mut(t) {
                    // Peak hold with decay, so a meter is readable rather than flickering.
                    s.peak = peak.max(s.peak * 0.8);
                }
            }
        }
    }

    fn loop_indices(&self) -> Vec<usize> {
        match &self.running {
            Ok(r) => r.layout.loops.iter().map(|l| l.loop_idx).collect(),
            Err(_) => Vec::new(),
        }
    }

    fn snapshot_of(&self, loop_idx: usize) -> Option<&LoopSnapshot> {
        self.loops.get(loop_idx)
    }

    /// Turns this frame's key events into notes.
    ///
    /// Driven by events rather than by "is the key down", so a press and its release are
    /// each acted on exactly once even if a frame is dropped.
    fn handle_keys(&mut self, ctx: &egui::Context) {
        let mut pressed: Vec<u8> = Vec::new();
        let mut released: Vec<u8> = Vec::new();
        let mut octave_delta = 0;
        let mut actions: Vec<keyboard::Action> = Vec::new();

        ctx.input(|i| {
            for event in &i.events {
                if let egui::Event::Key {
                    key,
                    pressed: down,
                    repeat,
                    ..
                } = event
                {
                    if *repeat {
                        // Auto-repeat would retrigger a held note; ignore it.
                        continue;
                    }
                    // Transport first: a key is one or the other, which `keyboard`'s tests enforce.
                    if let Some(action) = keyboard::key_to_action(*key) {
                        if *down {
                            actions.push(action);
                        }
                    } else if let Some(note) = keyboard::key_to_note(*key, self.octave) {
                        if *down {
                            pressed.push(note);
                        } else {
                            released.push(note);
                        }
                    }
                }
            }
        });

        for action in actions {
            match action {
                keyboard::Action::OctaveDown => octave_delta -= 1,
                keyboard::Action::OctaveUp => octave_delta += 1,
                // Stopping is the one action that needs no selection: it is the panic button.
                keyboard::Action::StopAll => self.stop_all(),
                // Both toggle against the mode the engine last reported, so one key starts and
                // stops rather than only starting.
                keyboard::Action::RecordOrStop => {
                    for idx in self.selected_loops() {
                        if self.mode_of(idx) == Some(LoopMode::Replacing) {
                            self.stop(idx);
                        } else {
                            self.record(idx);
                        }
                    }
                }
                keyboard::Action::PlayOrStop => {
                    for idx in self.selected_loops() {
                        if self.mode_of(idx) == Some(LoopMode::Playing) {
                            self.stop(idx);
                        } else {
                            self.play(idx);
                        }
                    }
                }
                keyboard::Action::Clear => {
                    for idx in self.selected_loops() {
                        self.clear(idx);
                    }
                }
            }
        }

        if octave_delta != 0 {
            self.octave =
                (self.octave + octave_delta).clamp(keyboard::MIN_OCTAVE, keyboard::MAX_OCTAVE);
        }

        if let Ok(r) = &mut self.running {
            for note in pressed {
                // Only if not already held, so a repeat cannot double-trigger.
                if self.held.insert(note) {
                    r.keys.send(&midi::note_on(0, note, 100));
                }
            }
            for note in released {
                if self.held.remove(&note) {
                    r.keys.send(&midi::note_off(0, note, 64));
                }
            }
        }
    }

    /// The mode the engine last reported for a loop.
    ///
    /// From the published snapshot, so it lags the engine by up to a frame. Good enough for a
    /// toggle: the alternative is a blocking read per key press.
    fn mode_of(&self, loop_idx: usize) -> Option<LoopMode> {
        self.loops.get(loop_idx).map(|l| l.mode)
    }

    /// Releases everything held, for when focus is lost mid-chord.
    fn release_all(&mut self) {
        if let Ok(r) = &mut self.running {
            for note in self.held.drain() {
                r.keys.send(&midi::note_off(0, note, 64));
            }
        } else {
            self.held.clear();
        }
    }
}

/// What a group action does, so the toolbar and the per-loop buttons share one path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Record,
    Play,
    Stop,
    Clear,
}

/// A script that does something audible, so the panel is not an empty box.
const DEFAULT_SCRIPT: &str = "\
-- Called once per sync-loop cycle. Coordinates are 1-based, as shown in the grid.
function on_cycle(cycle)
  -- Play the first loop of a different track every cycle, round-robin.
  local track = (cycle % 4) + 1
  shoop.stop_all()
  shoop.play(track, 1)
end
";

/// MIDI inputs the system offers. Empty rather than an error when MIDI is unavailable.
fn list_midi_inputs() -> Vec<String> {
    match midir::MidiInput::new("shoop-list") {
        Ok(input) => input
            .ports()
            .iter()
            .filter_map(|p| input.port_name(p).ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

fn mode_label(mode: LoopMode) -> &'static str {
    match mode {
        LoopMode::Stopped => "stopped",
        LoopMode::Playing => "playing",
        LoopMode::Recording => "recording",
        LoopMode::Replacing => "replacing",
        LoopMode::PlayingDryThroughWet => "play dry",
        LoopMode::RecordingDryIntoWet => "re-record",
        LoopMode::Unknown => "?",
    }
}

fn mode_colour(mode: LoopMode) -> egui::Color32 {
    match mode {
        LoopMode::Recording | LoopMode::RecordingDryIntoWet => egui::Color32::from_rgb(180, 40, 40),
        LoopMode::Playing | LoopMode::PlayingDryThroughWet => egui::Color32::from_rgb(40, 140, 60),
        LoopMode::Replacing => egui::Color32::from_rgb(160, 110, 30),
        _ => egui::Color32::from_gray(60),
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.draw(ui);
    }
}

impl App {
    // Accessors for tests driving the interface, which need to see that a click latched rather than
    // only that it was accepted.
    pub fn solo_active(&self) -> bool {
        self.solo_active
    }
    pub fn play_after_record(&self) -> bool {
        self.play_after_record
    }
    pub fn settings_open(&self) -> bool {
        self.settings_open
    }
    pub fn script_open(&self) -> bool {
        self.script_open
    }
    pub fn monitor_open(&self) -> bool {
        self.monitor_open
    }
    pub fn n_selected(&self) -> usize {
        self.selection.len()
    }
    /// True when no audio device could be opened, which is the headless case.
    pub fn audio_unavailable(&self) -> bool {
        self.running.is_err()
    }

    /// The whole window.
    ///
    /// Split out from `eframe::App::ui` so a test harness can drive it: a harness has no
    /// `eframe::Frame` to hand over, and three of the four bugs found by using the application were
    /// UI-level and invisible to tests that could not run the UI at all.
    pub fn draw(&mut self, ui: &mut egui::Ui) {
        // Panels nest inside the given `Ui`; free-floating windows still need the context, and
        // holding it separately keeps `ui` available to borrow mutably.
        let ctx = &ui.ctx().clone();
        // The engine runs on the device's clock, so the UI has to ask rather than wait.
        ctx.request_repaint_after(std::time::Duration::from_millis(16));

        if let Ok(r) = &mut self.running {
            if let Some(snap) = r.driver.handle().poll() {
                self.loops.clear();
                self.loops.extend(snap.loops.iter().cloned());
            }
        }
        self.poll_meters();
        self.drive_composite();
        self.poll_control();
        self.drive_script();
        self.drive_click();
        // Every few frames: contents change when a recording stops, not continuously, so polling
        // at frame rate would be waiting for nothing most of the time.
        self.waveform_tick = self.waveform_tick.wrapping_add(1);
        if self.waveform_tick.is_multiple_of(15) {
            self.refresh_waveforms();
        }
        if let Ok(r) = &self.running {
            // The cpal driver publishes load from its own callback, so this only reads it.
            self.dsp_history
                .push(driver_state(&r.driver).dsp_load_percent);
        }
        if let Some(idx) = self.details_of {
            self.details_pane(ctx, idx);
        }
        if self.monitor_open {
            // Slow cadence: these are diagnostics, and a blocking read per frame would cost more
            // than the information is worth.
            if self.waveform_tick.is_multiple_of(20) {
                self.read_monitor();
            }
            self.monitor_window(ctx);
        }

        if !ctx.input(|i| i.focused) {
            // Losing focus mid-chord would leave notes stuck on.
            self.release_all();
        } else if ctx.egui_wants_keyboard_input() {
            // A text field has the keyboard: typing in the script editor must not play notes, and
            // must certainly not hit Space and start recording.
            self.release_all();
        } else {
            self.handle_keys(ctx);
        }

        egui::Panel::top("toolbar").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.heading("ShoopDaLoop");
                ui.separator();
                self.bar_cursor(ui);
                ui.separator();
                ui.toggle_value(&mut self.solo_active, "solo")
                    .on_hover_text("Playing a loop stops the others");
                ui.toggle_value(&mut self.play_after_record, "play after record")
                    .on_hover_text("A recording starts playing when it finishes");
                ui.separator();
                if ui.button("stop all").clicked() {
                    self.stop_all();
                }
                ui.separator();
                let n = self.selection.len();
                ui.label(if n == 0 {
                    "no selection".to_string()
                } else {
                    format!("{n} selected")
                });
                ui.add_enabled_ui(n > 0, |ui| {
                    if ui.button("rec sel").clicked() {
                        self.apply_to_selection(Action::Record);
                    }
                    if ui.button("play sel").clicked() {
                        self.apply_to_selection(Action::Play);
                    }
                    if ui.button("stop sel").clicked() {
                        self.apply_to_selection(Action::Stop);
                    }
                    if ui.button("clear sel").clicked() {
                        self.apply_to_selection(Action::Clear);
                    }
                });
                if !self.retired.is_empty()
                    && ui
                        .button(format!("restore {} retired", self.retired.len()))
                        .clicked()
                {
                    self.restore_retired();
                }
                if ui.button("+ track").clicked() {
                    self.add_track();
                }
                if ui.button("select all").clicked() {
                    self.selection.select_all(N_TRACKS, N_LOOPS_PER_TRACK);
                }
                if ui.button("deselect").clicked() {
                    self.selection.clear();
                }
                ui.separator();
                ui.toggle_value(&mut self.settings_open, "settings");
                ui.toggle_value(&mut self.script_open, "script");
                ui.toggle_value(&mut self.monitor_open, "monitor");
                ui.separator();
                if ui.button("save").clicked() {
                    match self.capture() {
                        Some(session) => {
                            let path = self.session_path.clone();
                            self.status = match session.save(&path) {
                                Ok(()) => format!("saved to {}", path.display()),
                                Err(e) => format!("could not save: {e}"),
                            };
                        }
                        None => self.status = "nothing to save".to_string(),
                    }
                }
                if ui.button("load").clicked() {
                    let path = self.session_path.clone();
                    match SavedSession::load(&path) {
                        Ok(session) => {
                            self.restore(session);
                            self.status = format!("loaded {}", path.display());
                        }
                        Err(e) => self.status = format!("could not load: {e}"),
                    }
                }
                ui.separator();

                match &self.running {
                    Ok(r) => {
                        let st = driver_state(&r.driver);
                        ui.label(format!("{:.1}% dsp", st.dsp_load_percent));
                        // A history as well as the number: a load that spikes periodically looks
                        // the same as a steady one if only the latest value is shown.
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(90.0, 18.0), egui::Sense::hover());
                        let painter = ui.painter_at(rect);
                        painter.rect_filled(rect, 2.0, egui::Color32::from_gray(24));
                        let scale = self.dsp_history.peak().unwrap_or(1.0).max(1.0);
                        let n = self.dsp_history.len().max(1);
                        let step = rect.width() / n as f32;
                        for (i, v) in self.dsp_history.iter().enumerate() {
                            let h = (v / scale).clamp(0.0, 1.0) * rect.height();
                            let x = rect.left() + i as f32 * step;
                            painter.line_segment(
                                [
                                    egui::pos2(x, rect.bottom()),
                                    egui::pos2(x, rect.bottom() - h),
                                ],
                                egui::Stroke::new(1.0f32, egui::Color32::from_rgb(120, 170, 110)),
                            );
                        }
                        ui.label(format!("{} xruns", st.xruns));
                        ui.label(format!("{} cycles", st.last_processed));
                        ui.label(format!("{} voices", r.n_voices.load(Ordering::Relaxed)));
                    }
                    Err(_) => {
                        ui.colored_label(egui::Color32::from_rgb(200, 80, 80), &self.status);
                    }
                }
            });
        });

        if self.script_open {
            let mut open = self.script_open;
            egui::Window::new("lua script")
                .open(&mut open)
                .default_pos([300.0, 110.0])
                .default_width(520.0)
                .show(ctx, |ui| {
                    match self.script.as_ref() {
                        None => {
                            ui.colored_label(
                                egui::Color32::from_rgb(200, 80, 80),
                                "no interpreter available",
                            );
                        }
                        Some(s) => {
                            ui.horizontal(|ui| {
                                if s.has_cycle_hook() {
                                    ui.colored_label(
                                        egui::Color32::from_rgb(60, 150, 90),
                                        "running per cycle",
                                    );
                                } else {
                                    ui.weak("no on_cycle defined");
                                }
                                ui.label(format!("cycle {}", self.cycle));
                            });
                            if let Some(e) = s.last_error() {
                                // Shown rather than swallowed: a hook that raised has been
                                // stopped, and the user needs to know why it went quiet.
                                ui.colored_label(egui::Color32::from_rgb(200, 120, 60), e);
                            }
                        }
                    }
                    ui.separator();

                    ui.add(
                        egui::TextEdit::multiline(&mut self.script_source)
                            .code_editor()
                            .desired_rows(14)
                            .desired_width(f32::INFINITY),
                    );

                    ui.horizontal(|ui| {
                        if ui.button("load script").clicked() {
                            let source = self.script_source.clone();
                            if let Some(sc) = self.script.as_mut() {
                                sc.clear_error();
                                match sc.load(&source) {
                                    Ok(()) => {
                                        // Reset so the first wrap after loading is cycle one
                                        // rather than being taken as one already elapsed.
                                        self.script_counter.reset();
                                        self.status = "script loaded".to_string();
                                    }
                                    Err(e) => self.status = format!("script error: {e}"),
                                }
                                let pending = sc.drain();
                                self.apply_actions(pending);
                            }
                        }
                        if ui.button("stop").clicked() {
                            // Reloading an empty script is how a hook is removed.
                            if let Some(sc) = self.script.as_mut() {
                                let _ = sc.load("");
                                sc.clear_error();
                            }
                            self.status = "script stopped".to_string();
                        }
                        ui.weak("shoop.play/record/stop/clear(track,row) · stop_all · toggle_mute(track) · run_composite");
                    });
                });
            self.script_open = open;
        }

        if self.settings_open {
            let mut open = self.settings_open;
            egui::Window::new("audio & session")
                .open(&mut open)
                .default_pos([24.0, 110.0])
                .resizable(false)
                .show(ctx, |ui| {
                    ui.strong("audio output");
                    let current = match &self.running {
                        Ok(r) => r.driver.client_name().to_string(),
                        Err(_) => default_output_device_name().unwrap_or_default(),
                    };
                    ui.label(format!("current: {current}"));
                    ui.separator();

                    if self.audio_devices.is_empty() {
                        ui.weak("no output devices");
                    }
                    let devices = self.audio_devices.clone();
                    for name in devices {
                        let is_current = name == current;
                        // Switching to the device already in use would restart for nothing.
                        if ui
                            .add_enabled_ui(!is_current, |ui| {
                                ui.selectable_label(is_current, &name)
                            })
                            .inner
                            .clicked()
                        {
                            self.switch_device(name);
                        }
                    }
                    if ui.small_button("rescan devices").clicked() {
                        self.audio_devices = output_device_names();
                    }

                    ui.separator();
                    ui.strong("session");
                    ui.label(format!("file: {}", self.session_path.display()));
                    ui.weak("changing the device keeps the loops, and rebuilds the graph");
                });
            self.settings_open = open;
        }

        egui::Panel::bottom("connections").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("control:");
                if self.control_ports.is_empty() {
                    ui.weak("no MIDI inputs");
                } else {
                    let names = self.control_ports.clone();
                    for name in names {
                        if ui.button(&name).clicked() {
                            self.connect_control(&name);
                        }
                    }
                }
                if ui.small_button("rescan").clicked() {
                    self.control_ports = list_midi_inputs();
                }
                if self.control_input.is_some() && ui.small_button("disconnect").clicked() {
                    self.control_input = None;
                    self.status = "control disconnected".to_string();
                }
                ui.separator();
                ui.label(format!("{} bindings", self.mapping.bindings.len()));
                ui.weak("pads from note 36 · levels on CC 1+ · CC 123 stops all");
            });
        });

        egui::Panel::bottom("composite").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("arrangement:");
                if self.composite.is_empty() {
                    ui.weak("none");
                } else {
                    ui.label(format!(
                        "{} · {} cycles",
                        self.composite.name,
                        self.composite.total_cycles()
                    ));
                }
                ui.add_enabled_ui(!self.selection.is_empty(), |ui| {
                    if ui
                        .button("build from selection")
                        .on_hover_text("One selected loop per cycle, in grid order")
                        .clicked()
                    {
                        self.build_composite_from_selection();
                    }
                });
                ui.add_enabled_ui(!self.composite.is_empty(), |ui| {
                    if ui.button("run").clicked() {
                        // Counting starts fresh, so the first wrap after this advances to
                        // cycle one rather than being taken as a cycle already elapsed.
                        self.counter.reset();
                        let composite = self.composite.clone();
                        let (starts, stops) = self.playback.start(&composite);
                        self.apply_composite(starts, stops);
                    }
                    if ui.button("halt").clicked() {
                        let composite = self.composite.clone();
                        let stops = self.playback.stop(&composite);
                        self.apply_composite(Vec::new(), stops);
                    }
                });
                let mut looping = self.playback.looping();
                if ui.toggle_value(&mut looping, "repeat").changed() {
                    self.playback.set_looping(looping);
                }
                if self.playback.is_running() {
                    ui.label(format!(
                        "cycle {} of {}",
                        self.playback.cycle() + 1,
                        self.composite.total_cycles()
                    ));
                }
            });
        });

        egui::Panel::bottom("instrument").show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label("instrument:");
                if let Ok(r) = &self.running {
                    let mut w = r.settings.waveform();
                    egui::ComboBox::from_id_salt("waveform")
                        .selected_text(format!("{w:?}"))
                        .show_ui(ui, |ui| {
                            for option in [
                                Waveform::Sine,
                                Waveform::Square,
                                Waveform::Saw,
                                Waveform::Triangle,
                            ] {
                                ui.selectable_value(&mut w, option, format!("{option:?}"));
                            }
                        });
                    r.settings.set_waveform(w);

                    let mut gain = r.settings.gain();
                    if ui
                        .add(egui::Slider::new(&mut gain, 0.0f32..=1.0f32).text("level"))
                        .changed()
                    {
                        r.settings.set_gain(gain);
                    }
                }
                ui.separator();
                let mut bar = self.bar_seconds;
                if ui
                    .add(
                        egui::DragValue::new(&mut bar)
                            .range(0.25..=layout::MAX_BAR_SECONDS as f64)
                            .speed(0.05)
                            .prefix("bar ")
                            .suffix(" s"),
                    )
                    .on_hover_text("Length everything syncs to; the click divides it into beats")
                    .changed()
                {
                    self.bar_seconds = bar;
                    self.push_bar_length();
                }
                ui.separator();
                ui.toggle_value(&mut self.click.enabled, "click")
                    .on_hover_text("Metronome, divided from the sync loop's length");
                if self.click.enabled {
                    let mut beats = self.click.beats_per_bar;
                    if ui
                        .add(
                            egui::DragValue::new(&mut beats)
                                .range(1..=16)
                                .prefix("beats "),
                        )
                        .changed()
                    {
                        self.click.beats_per_bar = beats.max(1);
                        // Forgotten, so changing the division clicks the new beat rather than
                        // waiting for the next bar.
                        self.click.reset();
                    }
                }
                ui.separator();
                ui.label(format!("octave {}", self.octave));
                if ui.small_button("−").clicked() {
                    self.octave = (self.octave - 1).max(keyboard::MIN_OCTAVE);
                }
                if ui.small_button("+").clicked() {
                    self.octave = (self.octave + 1).min(keyboard::MAX_OCTAVE);
                }
                ui.separator();
                // Built from the mapping, so the hint cannot drift from what the keys do.
                ui.label(keyboard::hint());
                ui.separator();
                ui.label(keyboard::action_hint())
                    .on_hover_text("Record, play and clear act on the selected loops");
                if let Ok(r) = &self.running {
                    let dropped = r.keys.n_dropped();
                    if dropped > 0 {
                        // Worth showing: a dropped note-off is a stuck note.
                        ui.colored_label(
                            egui::Color32::from_rgb(200, 120, 60),
                            format!("{dropped} notes dropped"),
                        );
                    }
                }
            });
        });

        egui::CentralPanel::default().show(ui, |ui| {
            if self.running.is_err() {
                ui.colored_label(egui::Color32::from_rgb(200, 80, 80), &self.status);
                ui.label("The grid needs an audio device to run.");
                return;
            }
            if let Ok(r) = &self.running {
                ui.label(format!(
                    "{} · {} output port(s) · sync loop {}",
                    self.status,
                    r.layout.output_ports.len(),
                    r.layout.sync_loop
                ));
            }
            ui.add_space(4.0);

            egui::ScrollArea::both().show(ui, |ui| {
                let (n_tracks, max_rows) = match &self.running {
                    Ok(r) => (r.layout.n_tracks(), r.layout.max_rows()),
                    Err(_) => (0, 0),
                };
                ui.horizontal_top(|ui| {
                    for track in 0..n_tracks {
                        ui.vertical(|ui| {
                            // Fixed, not a minimum: egui widgets expand to the space offered, and
                            // in the first column that is the whole panel.
                            ui.set_width(230.0);
                            self.track_header(ui, track);
                            ui.separator();
                            for row in 0..max_rows {
                                let Some(l) = self
                                    .running
                                    .as_ref()
                                    .ok()
                                    .and_then(|r| r.layout.loop_at(track, row))
                                    .copied()
                                else {
                                    continue;
                                };
                                if self.retired.contains(&Cell { track, row }) {
                                    continue;
                                }
                                self.loop_widget(ui, l.loop_idx, track, row);
                            }
                            if ui.small_button("+ loop").clicked() {
                                self.add_loop(track);
                            }
                        });
                        ui.separator();
                    }
                });
            });
        });
    }
}

impl App {
    /// A track's name, level, muting and meter.
    fn track_header(&mut self, ui: &mut egui::Ui, track: usize) {
        ui.horizontal(|ui| {
            ui.strong(format!("track {}", track + 1));
            let mut muted = self.tracks.get(track).map(|t| t.muted).unwrap_or(false);
            if ui.toggle_value(&mut muted, "mute").changed() {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.muted = muted;
                }
                self.push_track(track);
            }
        });

        let mut gain = self.tracks.get(track).map(|t| t.gain).unwrap_or(1.0);
        if ui
            .add(
                egui::Slider::new(&mut gain, 0.0f32..=2.0f32)
                    .text("gain")
                    .clamping(egui::SliderClamping::Always),
            )
            .changed()
        {
            if let Some(t) = self.tracks.get_mut(track) {
                t.gain = gain;
            }
            self.push_track(track);
        }

        ui.horizontal(|ui| {
            let mut fx = self
                .tracks
                .get(track)
                .map(|t| t.fx)
                .unwrap_or(EffectKind::None);
            egui::ComboBox::from_id_salt(("fx", track))
                .selected_text(match fx {
                    EffectKind::None => "no fx",
                    EffectKind::LowPass => "low pass",
                    EffectKind::Delay => "delay",
                })
                .width(80.0)
                .show_ui(ui, |ui| {
                    for option in [EffectKind::None, EffectKind::LowPass, EffectKind::Delay] {
                        let label = match option {
                            EffectKind::None => "no fx",
                            EffectKind::LowPass => "low pass",
                            EffectKind::Delay => "delay",
                        };
                        ui.selectable_value(&mut fx, option, label);
                    }
                });
            if self.tracks.get(track).map(|t| t.fx) != Some(fx) {
                if let Some(t) = self.tracks.get_mut(track) {
                    t.fx = fx;
                }
                self.push_track_fx(track);
            }

            if fx != EffectKind::None {
                let mut bypassed = self
                    .tracks
                    .get(track)
                    .map(|t| t.fx_bypassed)
                    .unwrap_or(false);
                if ui.toggle_value(&mut bypassed, "byp").changed() {
                    if let Some(t) = self.tracks.get_mut(track) {
                        t.fx_bypassed = bypassed;
                    }
                    self.push_track_fx(track);
                }
                let mut wet = self.tracks.get(track).map(|t| t.fx_wet).unwrap_or(1.0);
                if ui
                    .add(
                        egui::DragValue::new(&mut wet)
                            .range(0.0..=1.0)
                            .speed(0.01)
                            .prefix("wet "),
                    )
                    .changed()
                {
                    if let Some(t) = self.tracks.get_mut(track) {
                        t.fx_wet = wet;
                    }
                    self.push_track_fx(track);
                }
            }
        });

        // Meter. Clamped rather than scaled, so clipping reads as full rather than as a
        // bar that keeps growing.
        let peak = self.tracks.get(track).map(|t| t.peak).unwrap_or(0.0);
        let filled = peak.clamp(0.0, 1.0);
        ui.add(
            egui::ProgressBar::new(filled)
                .desired_width(210.0)
                .desired_height(8.0)
                .fill(if peak >= 1.0 {
                    egui::Color32::from_rgb(200, 60, 50)
                } else {
                    egui::Color32::from_rgb(60, 150, 90)
                }),
        );
    }

    /// One loop: its state, its position within its length, and its controls.
    fn loop_widget(&mut self, ui: &mut egui::Ui, loop_idx: usize, track: usize, row: usize) {
        let snap = self.snapshot_of(loop_idx).cloned();
        let cell = Cell { track, row };
        let selected = self.selection.contains(cell);

        let frame = egui::Frame::group(ui.style()).stroke(if selected {
            egui::Stroke::new(2.0f32, egui::Color32::from_rgb(90, 140, 220))
        } else {
            ui.style().visuals.widgets.noninteractive.bg_stroke
        });

        frame.show(ui, |ui| {
            let (mode, length, position) = match &snap {
                Some(s) => (s.mode, s.length, s.position),
                None => (LoopMode::Stopped, 0, 0),
            };

            ui.horizontal(|ui| {
                // The number is the selection handle, so clicking a loop's controls does
                // not also change what is selected.
                let label = ui.selectable_label(selected, format!("{}", row + 1));
                if label.clicked() {
                    let how = ui.input(|i| {
                        if i.modifiers.shift {
                            Click::Range
                        } else if i.modifiers.command || i.modifiers.ctrl {
                            Click::Toggle
                        } else {
                            Click::Plain
                        }
                    });
                    self.selection.click(cell, how);
                }
                ui.colored_label(mode_colour(mode), mode_label(mode));
                if let Some(s) = &snap {
                    if let Some(next) = s.next_mode {
                        // A planned change is worth showing: with sync on, nothing appears
                        // to happen until the boundary arrives.
                        ui.weak(format!("→ {}", mode_label(next)));
                    }
                }
            });

            // Progress through the loop, which is the clearest signal it is running.
            let fraction = if length > 0 {
                (position as f32 / length as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            if self.waveforms.contains_key(&loop_idx) {
                // The waveform carries the position line, so the bar would be redundant.
                self.draw_waveform(ui, loop_idx, position, length);
            } else {
                ui.add(
                    egui::ProgressBar::new(fraction)
                        .desired_width(200.0)
                        .text(if length > 0 {
                            format!("{position} / {length}")
                        } else {
                            "empty".to_string()
                        }),
                );
            }

            ui.horizontal(|ui| {
                if ui.button("rec").clicked() {
                    self.record(loop_idx);
                }
                if ui.button("play").clicked() {
                    self.play(loop_idx);
                }
                if ui.button("stop").clicked() {
                    self.stop(loop_idx);
                }
                if ui.button("clear").clicked() {
                    self.clear(loop_idx);
                }
                if ui
                    .small_button("...")
                    .on_hover_text("channel details")
                    .clicked()
                {
                    self.details_of = Some(loop_idx);
                    self.read_channel_details(loop_idx);
                }
                if ui
                    .small_button("x")
                    .on_hover_text("retire this loop")
                    .clicked()
                {
                    self.retire(cell, loop_idx);
                }
            });
        });
    }
}
