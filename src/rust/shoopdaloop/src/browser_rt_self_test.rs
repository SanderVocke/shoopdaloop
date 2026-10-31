//! The core workflow on the new engine backend in a real browser, for the browser smoke.
//! It drives the application with the intents the UI sends and reports its progress as
//! `data-new-engine-self-test` on `#runtime_status`: the current step, `passed` or
//! `failed: <step>: <reason>`.

use std::time::Duration;

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
const NOTE_HOLD: Duration = Duration::from_millis(100);
/// Recorded or monitored capture is louder than this when a microphone is connected.
const SIGNAL_DB: f32 = -80.0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Step {
    Disabled,
    Startup,
    NewSession,
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
    Waveform,
    MidiDetail,
    ImmediateStop,
    Cleared,
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
    step: Step,
    since: Instant,
    ids: Ids,
    links: Vec<(PortId, HostPortId)>,
    routes: Vec<(PortId, BusChannelId)>,
    sync_length: u64,
    position: Option<f32>,
}

impl NewEngineSelfTest {
    pub fn new(enabled: bool) -> Self {
        let step = if enabled {
            Step::Startup
        } else {
            Step::Disabled
        };
        report(&format!("{step:?}"));
        Self {
            step,
            since: Instant::now(),
            ids: Ids::default(),
            links: Vec::new(),
            routes: Vec::new(),
            sync_length: 0,
            position: None,
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
            Ok(None) if waiting < STEP_TIMEOUT => {}
            Ok(None) => self.fail("timed out".to_owned()),
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
