//! The pre-configured session: what exists before the user touches anything.
//!
//! One instrument feeding a grid of loops, each able to record it and play back to the
//! device. Deliberately opinionated -- the point is that starting the application gives
//! something playable, not a blank canvas.

use shoop_engine::channel_mode::ChannelMode;
use shoop_engine::external_audio_port::ExternalAudioPort;
use shoop_engine::internal_audio_port::InternalAudioPort;
use shoop_engine::loop_mode::LoopMode;
use shoop_engine::port::{PortConnectability, PortDirection};
use shoop_engine::session::{Port, Session};

/// Tracks across, loops down. Matches the existing GUI's default shape closely enough to
/// be recognisable.
pub const N_TRACKS: usize = 4;
pub const N_LOOPS_PER_TRACK: usize = 4;

/// Chunk size for a loop's audio storage. A second at 48 kHz, so ordinary recording does
/// not keep reaching for another chunk.
const CHUNK: usize = 48000;

/// Length of the sync loop, which sets the bar everything aligns to.
const SYNC_SECONDS: u32 = 2;

/// Longest bar the tempo control allows.
///
/// A ceiling rather than a preference: every loop's storage is sized for this once, off the audio
/// thread, so later bar changes only ever shrink within storage that already exists. Growing a
/// channel from the audio thread allocates -- `resizing_a_loop_from_the_audio_thread_does_not_allocate`
/// in `shoop_engine/tests/no_alloc.rs` is what holds that line. Raising this costs
/// `MAX_BAR_SECONDS * sample_rate * 4` bytes per loop.
pub const MAX_BAR_SECONDS: u32 = 8;

/// Rate to assume when the session does not know its own yet.
///
/// A driver sets the real rate before this runs, but a session built without one reports
/// zero, and deriving a length from that gives a one-frame loop. That is not merely
/// wrong: a loop shorter than the buffer wraps many times per cycle, and the engine caps
/// how many sub-blocks one cycle may be split into, so it stalls instead of playing.
const ASSUMED_SAMPLE_RATE: u32 = 48000;

/// One loop as the UI thinks of it.
#[derive(Debug, Clone, Copy)]
pub struct LoopRef {
    pub loop_idx: usize,
    pub track: usize,
    pub row: usize,
}

/// What `configure` built, so the UI can address it.
#[derive(Debug, Clone)]
pub struct Layout {
    pub instrument_port: usize,
    /// Carries the metronome to the device only, never to a loop's input.
    pub click_port: usize,
    pub output_ports: Vec<usize>,
    /// One port per track, which its loops feed and which carries the track's gain,
    /// muting and meters. The existing GUI gives every track its own level control, and
    /// it cannot be done per loop: they share the track.
    pub track_ports: Vec<usize>,
    pub loops: Vec<LoopRef>,
    /// The loop everything else syncs to. The existing GUI keeps a dedicated one.
    pub sync_loop: usize,
}

impl Layout {
    pub fn loop_at(&self, track: usize, row: usize) -> Option<&LoopRef> {
        self.loops.iter().find(|l| l.track == track && l.row == row)
    }

    pub fn n_tracks(&self) -> usize {
        self.track_ports.len()
    }

    /// Rows present in a track, which may differ between tracks once loops are added.
    pub fn n_rows(&self, track: usize) -> usize {
        self.loops
            .iter()
            .filter(|l| l.track == track)
            .map(|l| l.row + 1)
            .max()
            .unwrap_or(0)
    }

    /// The widest track, which is how tall the grid has to be drawn.
    pub fn max_rows(&self) -> usize {
        (0..self.n_tracks())
            .map(|t| self.n_rows(t))
            .max()
            .unwrap_or(0)
    }
}

/// Adds a track with `n_rows` loops, wired like the others.
///
/// Returns its index. Everything it needs -- a port, loops, channels, sync -- is created here so a
/// track added at runtime is indistinguishable from one built at startup.
pub fn add_track(session: &mut Session, layout: &mut Layout, n_rows: usize) -> usize {
    let track = layout.track_ports.len();
    let port = session.add_port(Port::Internal(InternalAudioPort::new(
        format!("track_{}", track + 1),
        0,
        PortConnectability::INTERNAL,
        PortConnectability::INTERNAL,
        0,
    )));
    for &out in &layout.output_ports {
        let _ = session.connect_ports_internal(port, out);
    }
    layout.track_ports.push(port);

    for row in 0..n_rows {
        add_loop(session, layout, track, row);
    }
    let _ = session.apply_graph_changes();
    track
}

/// Adds a loop to the end of a track. Returns its row, or `None` for an unknown track.
pub fn add_loop_to_track(
    session: &mut Session,
    layout: &mut Layout,
    track: usize,
) -> Option<usize> {
    if track >= layout.track_ports.len() {
        return None;
    }
    let row = layout.n_rows(track);
    add_loop(session, layout, track, row);
    let _ = session.apply_graph_changes();
    Some(row)
}

/// The wiring one loop needs, shared by startup and by additions so they cannot drift apart.
fn add_loop(session: &mut Session, layout: &mut Layout, track: usize, row: usize) {
    let loop_idx = session.create_loop();
    if let Ok(c) = session.add_audio_channel(loop_idx, CHUNK, ChannelMode::Direct) {
        let _ = session.connect_channel_input(c, layout.instrument_port);
        if let Some(&port) = layout.track_ports.get(track) {
            let _ = session.connect_channel_output(c, port);
        }
    }
    let _ = session.set_loop_sync_source(loop_idx, Some(layout.sync_loop));
    // Fixed-bar model: a loop is always exactly one bar long, from the moment it exists. Playing
    // into it replaces those frames rather than growing a new take, so the bar of silence has to be
    // there first -- `Replacing` refuses to write past a channel's recorded length.
    let bar = session
        .loop_(layout.sync_loop)
        .map(|l| l.length())
        .unwrap_or(0);
    // Sized for the longest bar first, then cut down to the current one: see `MAX_BAR_SECONDS`.
    let rate = match session.sample_rate() {
        0 => ASSUMED_SAMPLE_RATE,
        r => r,
    };
    let _ = session.resize_loop(loop_idx, rate * MAX_BAR_SECONDS);
    let _ = session.resize_loop(loop_idx, bar);
    layout.loops.push(LoopRef {
        loop_idx,
        track,
        row,
    });
}

/// Builds the instrument port, the loop grid, and the wiring between them.
///
/// `output_ports` are the device's, already registered by the driver.
pub fn configure(session: &mut Session, output_ports: &[usize]) -> Layout {
    let rate = match session.sample_rate() {
        0 => ASSUMED_SAMPLE_RATE,
        r => r,
    };
    configure_with_bar(session, output_ports, rate * SYNC_SECONDS)
}

/// As [`configure`], with the bar given in frames.
///
/// Separate so a test can ask for a bar it can drive by hand, and so it gets the same loops
/// production does -- sizing them itself would test the test rather than the layout.
pub fn configure_with_bar(session: &mut Session, output_ports: &[usize], bar: u32) -> Layout {
    let instrument_port = session.add_port(Port::External(ExternalAudioPort::new(
        "instrument",
        PortDirection::Input,
        // A capture ring, so a loop can be grabbed retroactively later.
        48000,
    )));

    // The sync loop carries no audio; it exists to give everything else a common cycle,
    // which is how the existing GUI keeps tracks aligned.
    //
    // It has to be running, and this is easy to miss: a follower's planned transition
    // fires when its sync source wraps, so a stopped sync loop means nothing planned ever
    // lands and the sync toggle silently does nothing.
    let sync_loop = session.create_loop();
    if let Some(l) = session.loop_mut(sync_loop) {
        l.set_length(bar.max(1));
    }
    let _ = session.set_loop_mode(sync_loop, LoopMode::Playing);

    // The instrument has to reach the device directly as well as through the loops, or nothing is
    // heard while playing: the loops record *from* this port, so without this path a key press is
    // only audible once a recording is played back.
    for &out in output_ports {
        let _ = session.connect_ports_internal(instrument_port, out);
    }

    // The metronome gets its own port and its own voice. Sending the click through the instrument
    // put it on the port the loops record from, so every take captured the metronome. This port
    // reaches the device and nothing else.
    let click_port = session.add_port(Port::External(ExternalAudioPort::new(
        "click",
        PortDirection::Input,
        0,
    )));
    for &out in output_ports {
        let _ = session.connect_ports_internal(click_port, out);
    }

    let mut layout = Layout {
        instrument_port,
        click_port,
        output_ports: output_ports.to_vec(),
        track_ports: Vec::new(),
        loops: Vec::new(),
        sync_loop,
    };

    // Built through the same helper additions use, so a track made at startup and one made later
    // cannot drift apart.
    for _ in 0..N_TRACKS {
        add_track(session, &mut layout, N_LOOPS_PER_TRACK);
    }

    let _ = session.apply_graph_changes();
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configured() -> (Session, Layout) {
        let mut s = Session::default();
        let out = s.add_port(Port::External(ExternalAudioPort::new(
            "out_0",
            PortDirection::Output,
            0,
        )));
        let layout = configure(&mut s, &[out]);
        (s, layout)
    }

    #[test]
    fn the_grid_is_built_and_schedulable() {
        let (s, layout) = configured();
        assert_eq!(layout.loops.len(), N_TRACKS * N_LOOPS_PER_TRACK);
        // Plus the sync loop.
        assert_eq!(s.n_loops(), N_TRACKS * N_LOOPS_PER_TRACK + 1);
        assert!(
            s.graph_up_to_date(),
            "the session cannot run: the graph was left stale"
        );
    }

    #[test]
    fn every_loop_has_a_channel_wired_to_the_instrument() {
        let (s, layout) = configured();
        for l in &layout.loops {
            let lp = s.loop_(l.loop_idx).expect("loop");
            assert_eq!(
                lp.n_audio_channels(),
                1,
                "loop {} has no channel",
                l.loop_idx
            );
        }
    }

    #[test]
    fn loops_are_addressable_by_grid_position() {
        let (_s, layout) = configured();
        for track in 0..N_TRACKS {
            for row in 0..N_LOOPS_PER_TRACK {
                assert!(
                    layout.loop_at(track, row).is_some(),
                    "{track},{row} missing"
                );
            }
        }
        assert!(layout.loop_at(N_TRACKS, 0).is_none());
    }

    #[test]
    fn every_loop_follows_the_sync_loop() {
        let (s, layout) = configured();
        for l in &layout.loops {
            assert_eq!(
                s.sync_source_of(l.loop_idx),
                Some(layout.sync_loop),
                "loop {} is not synced",
                l.loop_idx
            );
        }
    }

    /// A sync loop derived from a zero sample rate would be one frame long, which stalls
    /// the cycle rather than playing fast. Worth a test of its own: the symptom is silence,
    /// not an error.
    /// Playing the instrument must be audible without recording anything first.
    #[test]
    fn the_instrument_reaches_the_device_directly() {
        let (mut s, layout) = configured();

        // Signal staged on the instrument port, as the synth does each cycle.
        if let Some(p) = s
            .port_mut(layout.instrument_port)
            .and_then(shoop_engine::session::Port::as_external_mut)
        {
            p.stage_input(&vec![0.5f32; 256]);
        }
        s.process(256).expect("cycle");

        let peak = s
            .port(layout.output_ports[0])
            .and_then(|p| p.audio())
            .map(|a| a.output_peak())
            .expect("output port");
        assert!(
            peak > 0.0,
            "the instrument was inaudible: nothing routes it to the device"
        );
    }

    #[test]
    fn every_track_has_a_port_between_its_loops_and_the_device() {
        let (s, layout) = configured();
        assert_eq!(layout.track_ports.len(), N_TRACKS);
        for &p in &layout.track_ports {
            // Internal, so it carries gain and meters rather than reaching the device
            // directly.
            assert!(s.port(p).and_then(|p| p.audio()).is_some());
        }
    }

    #[test]
    fn a_loop_reaches_the_device_through_its_track() {
        let (mut s, layout) = configured();
        let first = layout.loop_at(0, 0).expect("loop").loop_idx;

        // A recorded loop playing back should show up as level on its own track port.
        if let Some(c) = s.loop_mut(first).and_then(|l| l.audio_channel_mut(0)) {
            c.load_data(&vec![0.5f32; 512]);
        }
        s.loop_mut(first).expect("loop").set_length(512);
        s.set_loop_mode(first, LoopMode::Playing).expect("mode");
        s.process(256).expect("cycle");

        let peak = s
            .port(layout.track_ports[0])
            .and_then(|p| p.audio())
            .map(|a| a.output_peak())
            .expect("track port");
        assert!(peak > 0.0, "the track port saw no signal");
    }

    #[test]
    fn the_sync_loop_is_long_enough_to_be_playable() {
        let (s, layout) = configured();
        let len = s.loop_(layout.sync_loop).expect("sync loop").length();
        assert_eq!(len, ASSUMED_SAMPLE_RATE * SYNC_SECONDS);
    }

    #[test]
    fn a_known_sample_rate_is_used_when_there_is_one() {
        let mut s = Session::default();
        s.set_sample_rate(44100);
        let out = s.add_port(Port::External(ExternalAudioPort::new(
            "out_0",
            PortDirection::Output,
            0,
        )));
        let layout = configure(&mut s, &[out]);
        let len = s.loop_(layout.sync_loop).expect("sync loop").length();
        assert_eq!(len, 44100 * SYNC_SECONDS);
    }

    #[test]
    fn the_sync_loop_is_running() {
        let (s, layout) = configured();
        let sync = s.loop_(layout.sync_loop).expect("sync loop");
        // A stopped sync loop would make every planned transition wait forever.
        assert_eq!(sync.mode(), LoopMode::Playing);
        assert!(sync.length() > 0, "a zero-length sync loop never wraps");
    }

    #[test]
    fn the_sync_loop_advances_when_the_session_runs() {
        let (mut s, layout) = configured();
        s.process(256).expect("cycle");
        let sync = s.loop_(layout.sync_loop).expect("sync loop");
        assert_eq!(
            s.n_stuck_cycles(),
            0,
            "the cycle got stuck after {} sub-blocks",
            s.n_sub_blocks_last_cycle()
        );
        assert_eq!(sync.position(), 256);
    }

    #[test]
    fn a_session_can_run_a_cycle_as_configured() {
        let (mut s, _layout) = configured();
        assert!(
            s.process(256).is_ok(),
            "a configured session must be runnable"
        );
    }
}

#[cfg(test)]
mod growth_tests {
    use super::*;

    fn configured() -> (Session, Layout) {
        let mut s = Session::default();
        s.set_sample_rate(48000);
        let out = s.add_port(Port::External(
            shoop_engine::external_audio_port::ExternalAudioPort::new(
                "out",
                PortDirection::Output,
                0,
            ),
        ));
        let layout = configure(&mut s, &[out]);
        (s, layout)
    }

    #[test]
    fn the_initial_grid_reports_its_shape() {
        let (_s, l) = configured();
        assert_eq!(l.n_tracks(), N_TRACKS);
        assert_eq!(l.max_rows(), N_LOOPS_PER_TRACK);
        for t in 0..N_TRACKS {
            assert_eq!(l.n_rows(t), N_LOOPS_PER_TRACK);
        }
    }

    #[test]
    fn a_track_can_be_added_and_is_wired_like_the_others() {
        let (mut s, mut l) = configured();
        let before = s.n_ports();

        let track = add_track(&mut s, &mut l, 2);

        assert_eq!(track, N_TRACKS);
        assert_eq!(l.n_tracks(), N_TRACKS + 1);
        assert_eq!(l.n_rows(track), 2);
        // Its own port, plus nothing else stolen from the existing tracks.
        assert_eq!(s.n_ports(), before + 1);
        assert!(s.graph_up_to_date());

        for row in 0..2 {
            let cell = l.loop_at(track, row).expect("new loop");
            // Synced like every other loop, or its planned transitions would never land.
            assert_eq!(s.sync_source_of(cell.loop_idx), Some(l.sync_loop));
            assert_eq!(s.loop_(cell.loop_idx).expect("loop").n_audio_channels(), 1);
        }
        assert!(s.process(256).is_ok());
    }

    #[test]
    fn a_loop_can_be_added_to_an_existing_track() {
        let (mut s, mut l) = configured();
        let row = add_loop_to_track(&mut s, &mut l, 1).expect("a row");

        // Appended past the existing rows.
        assert_eq!(row, N_LOOPS_PER_TRACK);
        assert_eq!(l.n_rows(1), N_LOOPS_PER_TRACK + 1);
        // Other tracks are untouched, so the grid is allowed to be ragged.
        assert_eq!(l.n_rows(0), N_LOOPS_PER_TRACK);
        assert_eq!(l.max_rows(), N_LOOPS_PER_TRACK + 1);
        assert!(s.process(256).is_ok());
    }

    #[test]
    fn adding_a_loop_to_a_track_that_is_not_there_is_refused() {
        let (mut s, mut l) = configured();
        assert_eq!(add_loop_to_track(&mut s, &mut l, 99), None);
        assert_eq!(l.max_rows(), N_LOOPS_PER_TRACK);
    }

    #[test]
    fn an_added_loop_plays_through_its_own_track() {
        let (mut s, mut l) = configured();
        let track = add_track(&mut s, &mut l, 1);
        let cell = *l.loop_at(track, 0).expect("new loop");

        if let Some(c) = s
            .loop_mut(cell.loop_idx)
            .and_then(|lp| lp.audio_channel_mut(0))
        {
            c.load_data(&vec![0.5f32; 512]);
        }
        s.loop_mut(cell.loop_idx).expect("loop").set_length(512);
        s.set_loop_mode(cell.loop_idx, LoopMode::Playing)
            .expect("mode");
        s.process(256).expect("cycle");

        let peak = s
            .port(l.track_ports[track])
            .and_then(|p| p.audio())
            .map(|a| a.output_peak())
            .expect("track port");
        assert!(peak > 0.0, "an added track carried no signal");
    }
}
