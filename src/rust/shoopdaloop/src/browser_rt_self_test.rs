//! The core workflow on the new engine backend in a real browser, for the browser smoke.
//! It drives the application with the intents the UI sends and reports its progress as
//! `data-new-engine-self-test` on `#runtime_status`: the current step, `passed` or
//! `failed: <step>: <reason>`.

use std::{sync::Arc, time::Duration};

use shoop_egui::{
    AppIntent, AppSnapshot, ApplicationPortOwner, BusAction, BusChannelId, DirectTrackSpec,
    GlobalControlAction, HostPortId, IoTaskStatus, LoopAction, LoopMode, LoopState, MidiNote,
    NotificationLevel, PianoAction, PortDataType, PortId, PortRole, SelectionModifiers,
    StructuralState, TrackAction, TrackId, TrackState,
};
use web_time::Instant;

use crate::Runtime;

const STEP_TIMEOUT: Duration = Duration::from_secs(30);
const SYNC_RECORDING: Duration = Duration::from_millis(600);
const LARGE_FRAMES: usize = 33 * 1024 * 1024 / 4;
const NOTE_HOLD: Duration = Duration::from_millis(100);
/// Recorded or monitored capture is louder than this when a microphone is connected.
const SIGNAL_DB: f32 = -80.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Disabled,
    Startup,
    NewSession,
    Buses,
    BusZero,
    BusZeroSaving,
    BusZeroCleared,
    BusZeroLoaded,
    BusAddedAgain,
    BusNewSession,
    Tracks,
    Connections,
    SyncRecording,
    SyncRecorded,
    SyncPlayback,
    PlannedRecording,
    SyncedRecording,
    SyncedPlayback,
    ImmediateMidiRecording,
    NotePressed,
    NoteReleased,
    ImmediateMidiPlayback,
    Controls,
    Progress,
    Burst,
    Waveform,
    MidiDetail,
    ImmediateStop,
    TimingScalar,
    TimingChannels,
    TimingPickup,
    TimingPlaying,
    CancelSaving,
    Saving,
    Cleared,
    Loaded,
    Resaving,
    RestoredPlayback,
    LargeMapping,
    LargeImport,
    LargeSaving,
    LargeLoaded,
    LargeResaving,
    LargePlayback,
    PreviewReady,
    PreviewDenied,
    PreviewSystem,
    PreviewMaster,
    PreviewSwitchSystem,
    PreviewStopped,
    PreviewCompleted,
    Passed,
    Failed,
}

#[derive(Clone, Copy, Debug, Default)]
struct Ids {
    sync: Option<(TrackId, shoop_egui::LoopId)>,
    audio: Option<(TrackId, shoop_egui::LoopId)>,
    keys: Option<(TrackId, shoop_egui::LoopId)>,
    master: Option<shoop_egui::BusId>,
}

pub struct NewEngineSelfTest {
    preview_only: bool,
    preview_denied: bool,
    stress: bool,
    large: bool,
    step: Step,
    since: Instant,
    ids: Ids,
    links: Vec<(PortId, HostPortId)>,
    routes: Vec<(PortId, BusChannelId)>,
    sync_length: u64,
    position: Option<f32>,
    saved_file: Option<Arc<Vec<u8>>>,
    saved_bundle: Option<shoop_session::SessionBundle>,
    resaved_bundle: Option<shoop_session::SessionBundle>,
}

impl NewEngineSelfTest {
    pub fn new(enabled: bool, stress: bool, large: bool) -> Self {
        let step = if enabled {
            Step::Startup
        } else {
            Step::Disabled
        };
        report(&format!("{step:?}"));
        let query = web_sys::window()
            .and_then(|w| w.location().search().ok())
            .unwrap_or_default();
        Self {
            preview_only: query.contains("preview-only=1"),
            preview_denied: query.contains("preview-denied=1"),
            stress,
            large,
            step,
            since: Instant::now(),
            ids: Ids::default(),
            links: Vec::new(),
            routes: Vec::new(),
            sync_length: 0,
            position: None,
            saved_file: None,
            saved_bundle: None,
            resaved_bundle: None,
        }
    }

    pub fn file_output(&mut self, output: &crate::application::ApplicationFileOutput) {
        if !matches!(
            self.step,
            Step::Saving
                | Step::Resaving
                | Step::LargeSaving
                | Step::LargeResaving
                | Step::BusZeroSaving
        ) {
            return;
        }
        match shoop_session::decode_session(&output.bytes) {
            Ok(bundle)
                if matches!(
                    self.step,
                    Step::Saving | Step::LargeSaving | Step::BusZeroSaving
                ) =>
            {
                self.saved_file = Some(Arc::clone(&output.bytes));
                self.saved_bundle = Some(bundle);
            }
            Ok(bundle) => self.resaved_bundle = Some(bundle),
            Err(error) => self.fail(format!("saved file could not be decoded: {error}")),
        }
    }

    pub fn update(&mut self, runtime: &mut Runtime, snapshot: &AppSnapshot) {
        if matches!(self.step, Step::Disabled | Step::Passed | Step::Failed) {
            return;
        }
        let waiting = self.since.elapsed();
        let result = self.advance(runtime, snapshot, waiting);
        match result {
            Ok(Some(next)) => {
                tracing::info!(step = ?next, "frontend.new_engine.self_test_step");
                self.step = next;
                self.since = Instant::now();
                report(&format!("{next:?}"));
                if next == Step::Passed {
                    report("passed");
                }
            }
            Ok(None)
                if waiting
                    < if matches!(
                        self.step,
                        Step::LargeMapping
                            | Step::LargeImport
                            | Step::LargeSaving
                            | Step::LargeLoaded
                            | Step::LargeResaving
                            | Step::LargePlayback
                    ) || (self.large
                        && matches!(self.step, Step::BusZeroSaving | Step::BusZeroLoaded))
                    {
                        Duration::from_secs(120)
                    } else {
                        STEP_TIMEOUT
                    } => {}
            Ok(None) => self.fail(format!("timed out; file task: {:?}", snapshot.io_task)),
            Err(reason) => self.fail(reason),
        }
    }

    fn fail(&mut self, reason: String) {
        tracing::error!(step = ?self.step, reason = %reason, "frontend.new_engine.self_test_failed");
        report(&format!("failed: {:?}: {reason}", self.step));
        self.step = Step::Failed;
    }

    fn advance(
        &mut self,
        runtime: &mut Runtime,
        snapshot: &AppSnapshot,
        waiting: Duration,
    ) -> Result<Option<Step>, String> {
        let errors = snapshot
            .notifications
            .iter()
            .filter(|notification| notification.level == NotificationLevel::Error)
            .map(|notification| notification.message.as_str())
            .collect::<Vec<_>>();
        if !errors.is_empty() && self.step != Step::Startup {
            return Err(format!("error notifications: {errors:?}"));
        }
        if let Some(task) = snapshot
            .io_task
            .as_ref()
            .filter(|task| task.status == IoTaskStatus::Failed)
        {
            return Err(format!("file task failed: {}", task.message));
        }
        let running = runtime.audio_running();
        let signal = runtime.new_engine_microphone();
        let mut send = |intent: AppIntent| {
            runtime
                .dispatch(intent)
                .map_err(|error| format!("dispatch failed: {error}"))
        };
        let ids = self.ids;
        let next = match self.step {
            Step::Disabled | Step::Passed | Step::Failed => return Ok(None),
            Step::Startup => {
                if !(running
                    && snapshot.connections.backend_available
                    && snapshot.tracks.len() == 1)
                {
                    return Ok(None);
                }
                send(AppIntent::RequestNewSession)?;
                Step::NewSession
            }
            Step::NewSession => {
                let done = snapshot
                    .io_task
                    .as_ref()
                    .is_some_and(|task| task.status == IoTaskStatus::Completed);
                if !done {
                    return Ok(None);
                }
                if snapshot.tracks.len() != 1 || !snapshot.tracks[0].is_sync {
                    return Err("a new session has only the sync track".to_owned());
                }
                if self.preview_only {
                    let audio = &snapshot.tracks[1].loops[0];
                    if audio.length_frames != self.sync_length - 10240
                        || audio.channel_timings.len() != 2
                        || audio.channel_timings[0].start_offset != 8192
                        || audio.channel_timings[1].start_offset != 8320
                        || audio.channel_timings[0].preplay_samples != 8192
                        || audio.channel_timings[1].preplay_samples != 4096
                        || snapshot.tracks[2].loops[0].channel_timings[0].preplay_samples != 256
                    {
                        return Err("save/load changed timing windows".into());
                    }
                    self.ids.sync = Some((snapshot.tracks[0].id, snapshot.tracks[0].loops[0].id));
                    self.ids.master = snapshot.buses.first().map(|b| b.id);
                    return Ok(Some(Step::PreviewReady));
                }
                for (name, channel_count) in [("Cue", 1), ("Wide", 8)] {
                    send(AppIntent::AddBus(shoop_egui::BusSpec {
                        name: name.into(),
                        channel_count,
                        fx: None,
                        creation_request_id: None,
                    }))?;
                }
                Step::Buses
            }
            Step::Buses => {
                if snapshot.buses.len() != 3
                    || snapshot
                        .buses
                        .iter()
                        .any(|bus| bus.structural_state != StructuralState::Confirmed)
                {
                    return Ok(None);
                }
                send(AppIntent::Bus {
                    bus_id: snapshot.buses[2].id,
                    action: BusAction::MoveBefore(Some(snapshot.buses[1].id)),
                })?;
                let sync = &snapshot.tracks[0];
                self.ids.sync = Some((sync.id, sync.loops[0].id));
                self.ids.master = snapshot.buses.first().map(|bus| bus.id);
                send(AppIntent::Global(GlobalControlAction::SetSync(true)))?;
                for (name, audio_channels, midi) in [("Audio", 2, false), ("Keys", 0, true)] {
                    send(AppIntent::AddTrack(DirectTrackSpec {
                        name: name.to_owned(),
                        audio_channels,
                        midi,
                    }))?;
                }
                Step::Tracks
            }
            Step::Tracks => {
                let confirmed = snapshot.tracks.len() == 3
                    && snapshot.tracks.iter().all(|track| {
                        track.structural_state == StructuralState::Confirmed
                            && track
                                .loops
                                .iter()
                                .all(|state| state.structural_state == StructuralState::Confirmed)
                    });
                if !confirmed {
                    return Ok(None);
                }
                let audio = &snapshot.tracks[1];
                let keys = &snapshot.tracks[2];
                self.ids.audio = Some((audio.id, audio.loops[0].id));
                self.ids.keys = Some((keys.id, keys.loops[0].id));
                let master = snapshot
                    .buses
                    .first()
                    .ok_or_else(|| "Master is missing".to_owned())?;
                let captures = host_ports(snapshot, PortDataType::Audio, "webaudio:capture");
                let midi_sources = host_ports(snapshot, PortDataType::Midi, "webmidi:")
                    .into_iter()
                    .filter(|port| port.as_str().ends_with(" in"))
                    .collect::<Vec<_>>();
                if captures.is_empty() {
                    return Err("no capture host ports".to_owned());
                }
                let audio_inputs = ports(snapshot, audio.id, PortRole::AudioInput);
                let audio_outputs = ports(snapshot, audio.id, PortRole::AudioOutput);
                self.links = audio_inputs
                    .iter()
                    .enumerate()
                    .map(|(index, port)| (*port, captures[index.min(captures.len() - 1)].clone()))
                    .chain(
                        ports(snapshot, keys.id, PortRole::MidiInput)
                            .into_iter()
                            .zip(midi_sources),
                    )
                    .collect();
                self.routes = audio_outputs
                    .iter()
                    .zip(master.channels.iter())
                    .map(|(port, channel)| (*port, channel.id))
                    .collect();
                for bus in snapshot.buses.iter().skip(1) {
                    for channel in bus.channels.iter() {
                        self.routes.push((audio_outputs[0], channel.id));
                    }
                }
                for (port_id, host_port_id) in &self.links {
                    send(AppIntent::SetPortConnected {
                        port_id: *port_id,
                        host_port_id: host_port_id.clone(),
                        connected: true,
                    })?;
                }
                for (source_port_id, destination_channel_id) in &self.routes {
                    send(AppIntent::SetMixerRouteConnected {
                        source_port_id: *source_port_id,
                        destination_channel_id: *destination_channel_id,
                        connected: true,
                    })?;
                }
                Step::Connections
            }
            Step::Connections => {
                let connections = &snapshot.connections;
                let linked = self.links.iter().all(|(port, host)| {
                    connections
                        .confirmed_links
                        .iter()
                        .any(|link| link.application_port_id == *port && link.host_port_id == *host)
                });
                let routed = self.routes.iter().all(|(port, channel)| {
                    connections.mixer_links.iter().any(|route| {
                        route.source_port_id == *port && route.destination_channel_id == *channel
                    })
                });
                if !(linked && routed) {
                    return Ok(None);
                }
                send(loop_intent(ids.sync, LoopAction::RecordClicked)?)?;
                Step::SyncRecording
            }
            Step::SyncRecording => {
                if first_loop(snapshot, ids.sync)?.mode != LoopMode::Recording {
                    return Ok(None);
                }
                Step::SyncRecorded
            }
            Step::SyncRecorded => {
                if waiting < SYNC_RECORDING {
                    return Ok(None);
                }
                send(loop_intent(ids.sync, LoopAction::PlayClicked)?)?;
                Step::SyncPlayback
            }
            Step::SyncPlayback => {
                let sync = first_loop(snapshot, ids.sync)?;
                if sync.mode != LoopMode::Playing {
                    return Ok(None);
                }
                self.sync_length = sync.length_frames;
                if self.sync_length == 0 {
                    return Err("the sync loop is empty".to_owned());
                }
                send(loop_intent(ids.audio, LoopAction::RecordClicked)?)?;
                Step::PlannedRecording
            }
            Step::PlannedRecording => {
                let audio = first_loop(snapshot, ids.audio)?;
                if audio.mode == LoopMode::Recording {
                    return Err("the synced recording started without being planned".to_owned());
                }
                if !(audio.mode == LoopMode::Stopped && audio.next_mode == LoopMode::Recording) {
                    return Ok(None);
                }
                Step::SyncedRecording
            }
            Step::SyncedRecording => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Recording {
                    return Ok(None);
                }
                send(loop_intent(ids.audio, LoopAction::PlayClicked)?)?;
                Step::SyncedPlayback
            }
            Step::SyncedPlayback => {
                let audio = first_loop(snapshot, ids.audio)?;
                if audio.mode != LoopMode::Playing {
                    return Ok(None);
                }
                if audio.length_frames != self.sync_length {
                    return Err(format!(
                        "the synced loop has {} frames, the sync loop {}",
                        audio.length_frames, self.sync_length
                    ));
                }
                send(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
                send(track_intent(
                    ids.keys,
                    TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                )?)?;
                send(loop_intent(ids.keys, LoopAction::RecordClicked)?)?;
                Step::ImmediateMidiRecording
            }
            Step::ImmediateMidiRecording => {
                let keys = track(snapshot, ids.keys)?;
                if !(keys.loops[0].mode == LoopMode::Recording && keys.controls.input_monitoring) {
                    return Ok(None);
                }
                send(AppIntent::Piano(PianoAction::Press(note())))?;
                Step::NotePressed
            }
            Step::NotePressed => {
                if waiting < NOTE_HOLD {
                    return Ok(None);
                }
                send(AppIntent::Piano(PianoAction::Release(note())))?;
                Step::NoteReleased
            }
            Step::NoteReleased => {
                if waiting < NOTE_HOLD {
                    return Ok(None);
                }
                send(loop_intent(ids.keys, LoopAction::PlayClicked)?)?;
                Step::ImmediateMidiPlayback
            }
            Step::ImmediateMidiPlayback => {
                let keys = first_loop(snapshot, ids.keys)?;
                if !(keys.mode == LoopMode::Playing && keys.length_frames > 0) {
                    return Ok(None);
                }
                for action in [
                    TrackAction::OutputGainChanged(-6.0),
                    TrackAction::OutputBalanceChanged(0.5),
                    TrackAction::InputMonitoringChanged {
                        enabled: true,
                        respect_auto_mute: false,
                    },
                ] {
                    send(track_intent(ids.audio, action)?)?;
                }
                let master = ids.master.ok_or_else(|| "Master is missing".to_owned())?;
                for action in [
                    BusAction::GainChanged(-3.0),
                    BusAction::BalanceChanged(-0.25),
                ] {
                    send(AppIntent::Bus {
                        bus_id: master,
                        action,
                    })?;
                }
                Step::Controls
            }
            Step::Controls => {
                let controls = &track(snapshot, ids.audio)?.controls;
                let Some(master) = snapshot.buses.first() else {
                    return Err("Master is missing".to_owned());
                };
                let confirmed = controls.output_gain_db == -6.0
                    && controls.output_balance == 0.5
                    && controls.input_monitoring
                    && master.gain_db == -3.0
                    && master.balance == -0.25
                    && !master.control_pending;
                let metered = !signal
                    || (controls.input_peak_left_db > SIGNAL_DB
                        && master.output_peaks_db.iter().any(|peak| *peak > SIGNAL_DB));
                if !(confirmed && metered) {
                    return Ok(None);
                }
                self.position = Some(first_loop(snapshot, ids.audio)?.position);
                Step::Progress
            }
            Step::Progress => {
                let position = first_loop(snapshot, ids.audio)?.position;
                if Some(position) == self.position {
                    return Ok(None);
                }
                if self.stress {
                    for index in 0..64 {
                        let gain = if index == 63 {
                            -7.25
                        } else if index % 2 == 0 {
                            -9.0
                        } else {
                            -6.5
                        };
                        send(track_intent(
                            ids.audio,
                            TrackAction::OutputGainChanged(gain),
                        )?)?;
                    }
                    return Ok(Some(Step::Burst));
                }
                send(loop_intent(
                    ids.audio,
                    LoopAction::IconClicked(SelectionModifiers::default()),
                )?)?;
                Step::Waveform
            }
            Step::Burst => {
                if track(snapshot, ids.audio)?.controls.output_gain_db != -7.25 {
                    return Ok(None);
                }
                if let Some(element) = web_sys::window()
                    .and_then(|window| window.document())
                    .and_then(|document| document.get_element_by_id("runtime_status"))
                {
                    let _ = element
                        .set_attribute("data-control-burst-ms", &waiting.as_millis().to_string());
                }
                send(loop_intent(
                    ids.audio,
                    LoopAction::IconClicked(SelectionModifiers::default()),
                )?)?;
                Step::Waveform
            }
            Step::Waveform => {
                let (_, loop_id) = ids.audio.ok_or_else(|| "no audio loop".to_owned())?;
                let Some(details) = snapshot.details.as_ref().filter(|details| {
                    details.loop_id == loop_id
                        && details.channels.len() == 2
                        && details
                            .channels
                            .iter()
                            .all(|channel| channel.samples.len() as u64 == self.sync_length)
                }) else {
                    return Ok(None);
                };
                if signal
                    && !details.channels[0]
                        .samples
                        .iter()
                        .any(|sample| sample.abs() > 1e-4)
                {
                    return Err("the recorded waveform is silent".to_owned());
                }
                send(loop_intent(
                    ids.keys,
                    LoopAction::IconClicked(SelectionModifiers::default()),
                )?)?;
                Step::MidiDetail
            }
            Step::MidiDetail => {
                let (_, loop_id) = ids.keys.ok_or_else(|| "no MIDI loop".to_owned())?;
                let Some(events) = snapshot
                    .details
                    .as_ref()
                    .filter(|details| details.loop_id == loop_id)
                    .and_then(|details| details.midi_channels.first())
                    .map(|channel| &channel.events)
                    .filter(|events| events.len() == 2)
                else {
                    return Ok(None);
                };
                if events[0].frame >= events[1].frame {
                    return Err("the recorded note ends before it starts".to_owned());
                }
                send(loop_intent(ids.audio, LoopAction::StopClicked)?)?;
                Step::ImmediateStop
            }
            Step::ImmediateStop => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Stopped {
                    return Ok(None);
                }
                let audio = first_loop(snapshot, ids.audio)?;
                send(AppIntent::SetLoopTimeline {
                    loop_id: audio.id,
                    start_offset: Some(8192),
                    preplay_samples: Some(8192),
                    loop_length: Some(self.sync_length - 10240),
                })?;
                Step::TimingScalar
            }
            Step::TimingScalar => {
                let audio = first_loop(snapshot, ids.audio)?;
                if audio.length_frames != self.sync_length - 10240
                    || audio.channel_timings.iter().any(|channel| {
                        channel.start_offset != 8192 || channel.preplay_samples != 8192
                    })
                {
                    return Ok(None);
                }
                send(AppIntent::SetLoopChannelTimings {
                    loop_id: audio.id,
                    updates: audio
                        .channel_timings
                        .iter()
                        .enumerate()
                        .map(|(index, channel)| shoop_egui::ChannelTimelineUpdate {
                            channel_id: channel.id,
                            start_offset: Some(8192 + index as i64 * 128),
                            preplay_samples: Some(if index == 0 { 8192 } else { 4096 }),
                        })
                        .collect(),
                })?;
                send(AppIntent::SetLoopTimeline {
                    loop_id: ids.keys.unwrap().1,
                    start_offset: Some(128),
                    preplay_samples: Some(256),
                    loop_length: None,
                })?;
                send(AppIntent::Track {
                    track_id: ids.audio.unwrap().0,
                    action: TrackAction::InputMonitoringChanged {
                        enabled: false,
                        respect_auto_mute: false,
                    },
                })?;
                Step::TimingChannels
            }
            Step::TimingChannels => {
                let audio = first_loop(snapshot, ids.audio)?;
                let keys = first_loop(snapshot, ids.keys)?;
                if audio.channel_timings.len() != 2
                    || audio.channel_timings[1].start_offset != 8320
                    || audio.channel_timings[1].preplay_samples != 4096
                    || keys.channel_timings.first().is_none_or(|channel| {
                        channel.start_offset != 128 || channel.preplay_samples != 256
                    })
                {
                    return Ok(None);
                }
                if signal && audio.peak_left_db > SIGNAL_DB {
                    return Ok(None);
                }
                send(AppIntent::Global(GlobalControlAction::SetSync(true)))?;
                send(loop_intent(ids.audio, LoopAction::PlayClicked)?)?;
                Step::TimingPickup
            }
            Step::TimingPickup => {
                let audio = first_loop(snapshot, ids.audio)?;
                if audio.mode != LoopMode::Stopped || (signal && audio.peak_left_db < SIGNAL_DB) {
                    if waiting > Duration::from_secs(3) {
                        return Err(format!(
                            "pickup missing: target {audio:?}, source {:?}",
                            first_loop(snapshot, ids.sync)?
                        ));
                    }
                    return Ok(None);
                }
                Step::TimingPlaying
            }
            Step::TimingPlaying => {
                if first_loop(snapshot, ids.audio)?.mode != LoopMode::Playing {
                    return Ok(None);
                }
                send(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
                send(loop_intent(ids.audio, LoopAction::StopClicked)?)?;
                send(AppIntent::RequestSaveSession)?;
                Step::CancelSaving
            }
            Step::CancelSaving => {
                if !snapshot
                    .io_task
                    .as_ref()
                    .is_some_and(|task| task.status == IoTaskStatus::Cancelled)
                {
                    return Ok(None);
                }
                send(AppIntent::RequestSaveSession)?;
                Step::Saving
            }
            Step::Saving => {
                if self.saved_file.is_none() || !output_completed(snapshot)? {
                    return Ok(None);
                }
                send(AppIntent::Global(GlobalControlAction::ClearRecordings {
                    include_sync: false,
                }))?;
                Step::Cleared
            }
            Step::Cleared => {
                let cleared = [ids.audio, ids.keys].into_iter().all(|ids| {
                    first_loop(snapshot, ids).is_ok_and(|state| {
                        state.mode == LoopMode::Stopped && state.length_frames == 0
                    })
                });
                if !cleared {
                    return Ok(None);
                }
                if first_loop(snapshot, ids.sync)?.mode != LoopMode::Playing {
                    return Err("clearing stopped the sync loop".to_owned());
                }
                send(AppIntent::LoadSessionBytes {
                    name: "browser-roundtrip.shoop".into(),
                    bytes: Arc::clone(self.saved_file.as_ref().unwrap()),
                })?;
                Step::Loaded
            }
            Step::Loaded => {
                if !snapshot.io_task.as_ref().is_some_and(|task| {
                    task.kind == shoop_egui::IoTaskKind::LoadSession
                        && task.status == IoTaskStatus::Completed
                }) {
                    return Ok(None);
                }
                if snapshot.tracks.len() != 3
                    || snapshot.tracks[1].id == ids.audio.unwrap().0
                    || snapshot.tracks[1].loops[0].id == ids.audio.unwrap().1
                    || snapshot
                        .tracks
                        .iter()
                        .flat_map(|track| &track.loops)
                        .any(|entry| entry.mode != LoopMode::Stopped)
                    || snapshot.tracks[1].controls.output_gain_db != -6.0
                    || snapshot.buses[0].gain_db != -3.0
                    || snapshot
                        .buses
                        .iter()
                        .map(|b| b.name.as_str())
                        .collect::<Vec<_>>()
                        != ["Master", "Wide", "Cue"]
                    || snapshot.buses[1].channels.len() != 8
                    || snapshot
                        .capabilities
                        .supports(shoop_egui::BackendCapability::BusFx)
                    || snapshot.tracks[0].loops[0].length_frames != self.sync_length
                {
                    return Err(
                        "loaded session did not restore controls, lengths and fresh stopped owners"
                            .into(),
                    );
                }
                let audio = &snapshot.tracks[1].loops[0];
                if audio.length_frames != self.sync_length - 10240
                    || audio.channel_timings.len() != 2
                    || audio.channel_timings[0].start_offset != 8192
                    || audio.channel_timings[1].start_offset != 8320
                    || audio.channel_timings[0].preplay_samples != 8192
                    || audio.channel_timings[1].preplay_samples != 4096
                    || snapshot.tracks[2].loops[0].channel_timings[0].preplay_samples != 256
                {
                    return Err("save/load changed timing windows".into());
                }
                self.ids.sync = Some((snapshot.tracks[0].id, snapshot.tracks[0].loops[0].id));
                self.ids.audio = Some((snapshot.tracks[1].id, snapshot.tracks[1].loops[0].id));
                self.ids.keys = Some((snapshot.tracks[2].id, snapshot.tracks[2].loops[0].id));
                send(AppIntent::RequestSaveSession)?;
                Step::Resaving
            }
            Step::Resaving => {
                let Some(after) = self.resaved_bundle.as_ref() else {
                    return Ok(None);
                };
                if !output_completed(snapshot)? {
                    return Ok(None);
                }
                let before = self.saved_bundle.as_ref().unwrap();
                if before.media.values().collect::<Vec<_>>()
                    != after.media.values().collect::<Vec<_>>()
                {
                    return Err("save/load/save changed recorded PCM or exact MIDI".into());
                }
                send(AppIntent::Global(GlobalControlAction::SetSync(false)))?;
                send(loop_intent(ids.audio, LoopAction::PlayClicked)?)?;
                self.position = None;
                Step::RestoredPlayback
            }
            Step::RestoredPlayback => {
                let entry = first_loop(snapshot, ids.audio)?;
                if entry.mode != LoopMode::Playing || entry.length_frames == 0 {
                    return Ok(None);
                }
                match self.position {
                    None => {
                        self.position = Some(entry.position);
                        return Ok(None);
                    }
                    Some(position) if position == entry.position => return Ok(None),
                    _ => {}
                }
                if !self.large {
                    for bus in snapshot.buses.iter() {
                        send(AppIntent::Bus {
                            bus_id: bus.id,
                            action: BusAction::Remove,
                        })?;
                    }
                    return Ok(Some(Step::BusZero));
                }
                self.saved_file = None;
                self.saved_bundle = None;
                self.resaved_bundle = None;
                let audio = shoop_session::LoopAudio {
                    sample_rate: snapshot.status.sample_rate,
                    channels: vec![shoop_session::LoopAudioChannel {
                        label: "Large mono".into(),
                        role: "direct".into(),
                        samples: (0..LARGE_FRAMES)
                            .map(|i| if i % 2 == 0 { 0.25 } else { -0.5 })
                            .collect(),
                    }],
                };
                let bytes = shoop_session::encode_float_wav(&audio).map_err(|e| e.to_string())?;
                if bytes.len() <= 32 * 1024 * 1024 {
                    return Err("large browser fixture is too small".into());
                }
                send(AppIntent::ImportLoopAudioBytes {
                    loop_id: ids.audio.unwrap().1,
                    name: "large.wav".into(),
                    bytes: bytes.into(),
                    update_loop_length: true,
                })?;
                Step::LargeMapping
            }
            Step::LargeMapping => {
                let Some(task) = snapshot
                    .io_task
                    .as_ref()
                    .filter(|t| t.status == IoTaskStatus::AwaitingChannelMapping)
                else {
                    return Ok(None);
                };
                send(AppIntent::ConfirmAudioChannelMapping {
                    task_id: task.id,
                    source_for_destination: vec![0, 0],
                })?;
                Step::LargeImport
            }
            Step::LargeImport => {
                if !snapshot.io_task.as_ref().is_some_and(|t| {
                    t.kind == shoop_egui::IoTaskKind::ImportLoopAudio
                        && t.status == IoTaskStatus::Completed
                }) {
                    return Ok(None);
                }
                if first_loop(snapshot, ids.audio)?.length_frames != LARGE_FRAMES as u64 {
                    return Err("large import length changed".into());
                }
                send(AppIntent::RequestSaveSession)?;
                Step::LargeSaving
            }
            Step::LargeSaving => {
                let Some(bundle) = self.saved_bundle.as_ref() else {
                    return Ok(None);
                };
                if !output_completed(snapshot)? {
                    return Ok(None);
                }
                let large = bundle
                    .media
                    .values()
                    .filter_map(|media| match media {
                        shoop_session::MediaPayload::Audio(audio)
                            if audio.samples.len() == LARGE_FRAMES =>
                        {
                            Some(audio)
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if large.len() != 2
                    || large.iter().any(|audio| {
                        audio
                            .samples
                            .iter()
                            .enumerate()
                            .any(|(i, sample)| *sample != if i % 2 == 0 { 0.25 } else { -0.5 })
                    })
                {
                    return Err("large browser save changed mapped PCM".into());
                }
                send(AppIntent::LoadSessionBytes {
                    name: "large.shoop".into(),
                    bytes: Arc::clone(self.saved_file.as_ref().unwrap()),
                })?;
                Step::LargeLoaded
            }
            Step::LargeLoaded => {
                if !snapshot.io_task.as_ref().is_some_and(|t| {
                    t.kind == shoop_egui::IoTaskKind::LoadSession
                        && t.status == IoTaskStatus::Completed
                }) {
                    return Ok(None);
                }
                if snapshot.tracks[1].loops[0].id == ids.audio.unwrap().1
                    || snapshot
                        .tracks
                        .iter()
                        .flat_map(|t| &t.loops)
                        .any(|l| l.mode != LoopMode::Stopped)
                    || snapshot.tracks[1].loops[0].length_frames != LARGE_FRAMES as u64
                {
                    return Err("large load did not restore fresh stopped content".into());
                }
                self.ids.audio = Some((snapshot.tracks[1].id, snapshot.tracks[1].loops[0].id));
                send(AppIntent::RequestSaveSession)?;
                self.resaved_bundle = None;
                Step::LargeResaving
            }
            Step::LargeResaving => {
                let Some(after) = self.resaved_bundle.as_ref() else {
                    return Ok(None);
                };
                if !output_completed(snapshot)? {
                    return Ok(None);
                }
                if self
                    .saved_bundle
                    .as_ref()
                    .unwrap()
                    .media
                    .values()
                    .collect::<Vec<_>>()
                    != after.media.values().collect::<Vec<_>>()
                {
                    return Err("large browser roundtrip changed PCM or MIDI".into());
                }
                send(loop_intent(ids.audio, LoopAction::PlayClicked)?)?;
                self.position = None;
                Step::LargePlayback
            }
            Step::LargePlayback => {
                let entry = first_loop(snapshot, ids.audio)?;
                if entry.mode != LoopMode::Playing {
                    return Ok(None);
                }
                match self.position {
                    None => {
                        self.position = Some(entry.position);
                        return Ok(None);
                    }
                    Some(position) if position == entry.position => return Ok(None),
                    _ => {}
                }
                for bus in snapshot.buses.iter() {
                    send(AppIntent::Bus {
                        bus_id: bus.id,
                        action: BusAction::Remove,
                    })?;
                }
                Step::BusZero
            }
            Step::BusZero => {
                if !snapshot.buses.is_empty() {
                    return Ok(None);
                }
                self.saved_file = None;
                self.saved_bundle = None;
                send(AppIntent::RequestSaveSession)?;
                Step::BusZeroSaving
            }
            Step::BusZeroSaving => {
                if !snapshot.io_task.as_ref().is_some_and(|t| {
                    t.kind == shoop_egui::IoTaskKind::SaveSession
                        && t.status == IoTaskStatus::Completed
                }) {
                    return Ok(None);
                }
                let Some(bundle) = &self.saved_bundle else {
                    return Ok(None);
                };
                if !bundle.document.buses.is_empty()
                    || !bundle.document.bus_display_order.is_empty()
                    || !bundle.document.mixer_routes.is_empty()
                {
                    return Err("zero-bus save retained bus state".into());
                }
                send(AppIntent::RequestNewSession)?;
                Step::BusZeroCleared
            }
            Step::BusZeroCleared => {
                if snapshot.buses.len() != 1
                    || !snapshot.io_task.as_ref().is_some_and(|t| {
                        t.kind == shoop_egui::IoTaskKind::LoadSession
                            && t.status == IoTaskStatus::Completed
                    })
                {
                    return Ok(None);
                }
                send(AppIntent::LoadSessionBytes {
                    name: "zero-buses.shoop".into(),
                    bytes: Arc::clone(self.saved_file.as_ref().unwrap()),
                })?;
                Step::BusZeroLoaded
            }
            Step::BusZeroLoaded => {
                if !snapshot.buses.is_empty()
                    || !snapshot.io_task.as_ref().is_some_and(|t| {
                        t.kind == shoop_egui::IoTaskKind::LoadSession
                            && t.status == IoTaskStatus::Completed
                    })
                {
                    return Ok(None);
                }
                send(AppIntent::AddBus(shoop_egui::BusSpec {
                    name: "After zero".into(),
                    channel_count: 1,
                    fx: None,
                    creation_request_id: None,
                }))?;
                Step::BusAddedAgain
            }
            Step::BusAddedAgain => {
                if snapshot.buses.len() != 1
                    || snapshot.buses[0].name != "After zero"
                    || snapshot.buses[0].structural_state != StructuralState::Confirmed
                {
                    return Ok(None);
                }
                send(AppIntent::RequestNewSession)?;
                Step::BusNewSession
            }
            Step::PreviewReady => {
                if waiting < Duration::from_secs(1) {
                    return Ok(None);
                }
                send(preview_intent(
                    ids.sync,
                    shoop_egui::ClickTrackPreviewOutput::System,
                    16,
                )?)?;
                if self.preview_denied {
                    Step::PreviewDenied
                } else {
                    Step::PreviewSystem
                }
            }
            Step::PreviewDenied => {
                if snapshot.click_track.preview_status
                    == shoop_egui::ClickTrackPreviewStatus::Playing
                {
                    return Err("suspended System preview falsely reported Playing".into());
                }
                if snapshot.click_track.preview_status
                    != shoop_egui::ClickTrackPreviewStatus::Failed
                {
                    return Ok(None);
                }
                if !(snapshot.click_track.preview_message.contains("resume")
                    || snapshot.click_track.preview_message.contains("suspended"))
                {
                    return Err(format!(
                        "unexpected preview error: {:?}",
                        snapshot.click_track
                    ));
                }
                send(preview_intent(
                    ids.sync,
                    shoop_egui::ClickTrackPreviewOutput::System,
                    16,
                )?)?;
                Step::PreviewSystem
            }
            Step::PreviewSystem | Step::PreviewSwitchSystem => {
                if snapshot.click_track.preview_status
                    == shoop_egui::ClickTrackPreviewStatus::Failed
                {
                    return Err(format!("System preview failed: {:?}", snapshot.click_track));
                }
                if snapshot.click_track.preview_status
                    != shoop_egui::ClickTrackPreviewStatus::Playing
                    || waiting < Duration::from_millis(400)
                {
                    return Ok(None);
                }
                if self.step == Step::PreviewSystem {
                    send(preview_intent(
                        ids.sync,
                        shoop_egui::ClickTrackPreviewOutput::MasterBus,
                        16,
                    )?)?;
                    Step::PreviewMaster
                } else {
                    send(AppIntent::StopClickTrackPreview)?;
                    Step::PreviewStopped
                }
            }
            Step::PreviewMaster => {
                if snapshot.click_track.preview_status
                    == shoop_egui::ClickTrackPreviewStatus::Failed
                {
                    return Err(format!("Master preview failed: {:?}", snapshot.click_track));
                }
                if snapshot.click_track.preview_status
                    != shoop_egui::ClickTrackPreviewStatus::Playing
                    || !snapshot
                        .buses
                        .iter()
                        .any(|b| b.output_peaks_db.iter().any(|p| *p > SIGNAL_DB))
                {
                    return Ok(None);
                }
                send(preview_intent(
                    ids.sync,
                    shoop_egui::ClickTrackPreviewOutput::System,
                    16,
                )?)?;
                Step::PreviewSwitchSystem
            }
            Step::PreviewStopped => {
                if snapshot.click_track.preview_status
                    != shoop_egui::ClickTrackPreviewStatus::Cancelled
                    || waiting < Duration::from_millis(500)
                {
                    return Ok(None);
                }
                send(preview_intent(
                    ids.sync,
                    shoop_egui::ClickTrackPreviewOutput::System,
                    2,
                )?)?;
                Step::PreviewCompleted
            }
            Step::PreviewCompleted => {
                if snapshot.click_track.preview_status
                    != shoop_egui::ClickTrackPreviewStatus::Completed
                {
                    return Ok(None);
                }
                if snapshot
                    .tracks
                    .iter()
                    .flat_map(|t| &t.loops)
                    .any(|l| l.length_frames != 0 || l.mode != LoopMode::Stopped)
                {
                    return Err("preview changed loop media or transport".into());
                }
                Step::Passed
            }
            Step::BusNewSession => {
                if snapshot.buses.len() != 1
                    || snapshot.buses[0].name != "Master"
                    || !snapshot.io_task.as_ref().is_some_and(|t| {
                        t.kind == shoop_egui::IoTaskKind::LoadSession
                            && t.status == IoTaskStatus::Completed
                    })
                {
                    return Ok(None);
                }
                Step::Passed
            }
        };
        Ok(Some(next))
    }
}

fn report(status: &str) {
    if let Some(element) = web_sys::window()
        .and_then(|window| window.document())
        .and_then(|document| document.get_element_by_id("runtime_status"))
    {
        let _ = element.set_attribute("data-new-engine-self-test", status);
    }
}

fn note() -> MidiNote {
    MidiNote::new(60).expect("middle C")
}

fn track(
    snapshot: &AppSnapshot,
    ids: Option<(TrackId, shoop_egui::LoopId)>,
) -> Result<&TrackState, String> {
    let (track_id, _) = ids.ok_or_else(|| "the track is not known yet".to_owned())?;
    snapshot
        .tracks
        .iter()
        .find(|track| track.id == track_id)
        .ok_or_else(|| format!("track {track_id:?} disappeared"))
}

fn first_loop(
    snapshot: &AppSnapshot,
    ids: Option<(TrackId, shoop_egui::LoopId)>,
) -> Result<&LoopState, String> {
    track(snapshot, ids)?
        .loops
        .first()
        .ok_or_else(|| "the track has no loop".to_owned())
}

fn loop_intent(
    ids: Option<(TrackId, shoop_egui::LoopId)>,
    action: LoopAction,
) -> Result<AppIntent, String> {
    let (track_id, loop_id) = ids.ok_or_else(|| "the loop is not known yet".to_owned())?;
    Ok(AppIntent::Loop {
        track_id,
        loop_id,
        action,
    })
}

fn track_intent(
    ids: Option<(TrackId, shoop_egui::LoopId)>,
    action: TrackAction,
) -> Result<AppIntent, String> {
    let (track_id, _) = ids.ok_or_else(|| "the track is not known yet".to_owned())?;
    Ok(AppIntent::Track { track_id, action })
}

fn ports(snapshot: &AppSnapshot, track_id: TrackId, role: PortRole) -> Vec<PortId> {
    snapshot
        .connections
        .application_ports
        .iter()
        .filter(|port| {
            matches!(port.owner, ApplicationPortOwner::Track { track_id: owner, .. } if owner == track_id)
                && port.role == role
        })
        .map(|port| port.id)
        .collect()
}

fn host_ports(snapshot: &AppSnapshot, data_type: PortDataType, prefix: &str) -> Vec<HostPortId> {
    let mut ports = snapshot
        .connections
        .host_ports
        .iter()
        .filter(|port| port.data_type == data_type && port.id.as_str().starts_with(prefix))
        .map(|port| port.id.clone())
        .collect::<Vec<_>>();
    ports.sort_by(|a, b| a.as_str().cmp(b.as_str()));
    ports
}

fn output_completed(snapshot: &AppSnapshot) -> Result<bool, String> {
    let Some(task) = snapshot.io_task.as_ref() else {
        return Ok(false);
    };
    if task.status == IoTaskStatus::Failed || task.status == IoTaskStatus::Cancelled {
        return Err(format!("platform output failed: {}", task.message));
    }
    if task.status != IoTaskStatus::Completed {
        return Ok(false);
    }
    if !task.message.contains("Download handed to the browser") {
        return Err(format!(
            "browser output claimed an unexpected completion: {}",
            task.message
        ));
    }
    Ok(true)
}

fn preview_intent(
    ids: Option<(TrackId, shoop_egui::LoopId)>,
    output: shoop_egui::ClickTrackPreviewOutput,
    click_count: u32,
) -> Result<AppIntent, String> {
    Ok(AppIntent::PreviewClickTrack {
        loop_id: ids.ok_or("missing preview target")?.1,
        request: shoop_egui::ClickTrackRequest {
            click_count,
            ..Default::default()
        },
        output,
    })
}
