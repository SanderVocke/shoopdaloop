use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use shoop_engine::app_backend::{
    AudioChannel, AudioDriver, AudioDriverSettings, AudioPort, BackendSession,
    DummyAudioDriverSettings, Loop as EngineLoop, MidiChannel, MidiPort,
};
use shoop_engine::{AudioDriverType, ChannelMode, LoopMode, PortDirection};

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendLoopId(u64);

impl BackendLoopId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct BackendTrackId(u64);

impl BackendTrackId {
    pub const fn from_raw(value: u64) -> Self {
        Self(value)
    }

    pub const fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DirectTrackRequest {
    pub port_name_base: String,
    pub audio_channels: u8,
    pub midi: bool,
    pub initial_loops: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackendTrackCreation {
    pub track_id: BackendTrackId,
    pub loops: Vec<BackendLoopId>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BackendTrackControl {
    OutputGainDb(f32),
    OutputBalance(f32),
    OutputMute(bool),
    InputGainDb(f32),
    InputBalance(f32),
    InputMonitoring(bool),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendTrackState {
    pub audio_channels: u8,
    pub midi: bool,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub input_monitoring: bool,
    pub input_peaks: Vec<f32>,
    pub output_peaks: Vec<f32>,
    pub input_midi_activity: bool,
    pub output_midi_activity: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackendStatus {
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendLoopMode {
    #[default]
    Unknown,
    Stopped,
    Playing,
    Recording,
    Replacing,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendLoopState {
    pub mode: BackendLoopMode,
    pub length: u32,
    pub position: u32,
    pub next_mode: Option<BackendLoopMode>,
    pub next_transition_delay: Option<u32>,
    pub stereo: bool,
    pub gain: f32,
    pub audio_peaks: Vec<f32>,
    pub midi_activity: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct BackendSnapshot {
    pub status: BackendStatus,
    pub tracks: BTreeMap<BackendTrackId, BackendTrackState>,
    pub loops: BTreeMap<BackendLoopId, BackendLoopState>,
}

pub trait Backend: Send {
    fn create_loop(&mut self) -> Result<BackendLoopId>;
    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation>;
    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId>;
    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()>;
    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()>;
    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Vec<Arc<[f32]>>>;
    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()>;
    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()>;
    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()>;
    fn poll(&mut self) -> Result<BackendSnapshot>;
    fn wait_idle(&mut self);
}

pub struct EngineBackend {
    session: BackendSession,
    driver: AudioDriver,
    loops: BTreeMap<BackendLoopId, EngineLoop>,
    loop_channels: BTreeMap<BackendLoopId, EngineLoopChannels>,
    tracks: BTreeMap<BackendTrackId, EngineTrack>,
    next_loop_id: u64,
    next_track_id: u64,
}

struct EngineLoopChannels {
    audio: Vec<AudioChannel>,
    midi: Vec<MidiChannel>,
}

struct EngineTrack {
    audio_inputs: Vec<AudioPort>,
    audio_outputs: Vec<AudioPort>,
    midi_input: Option<MidiPort>,
    midi_output: Option<MidiPort>,
    loops: Vec<BackendLoopId>,
    output_gain_db: f32,
    output_balance: f32,
    output_muted: bool,
    input_gain_db: f32,
    input_balance: f32,
    input_monitoring: bool,
}

impl EngineBackend {
    pub fn new_dummy(sample_rate: u32, buffer_size: u32) -> Result<Self> {
        let driver = AudioDriver::new(AudioDriverType::Dummy, None)?;
        driver.start(&AudioDriverSettings::Dummy(DummyAudioDriverSettings {
            client_name: "ShoopDaLoop-egui".to_owned(),
            sample_rate,
            buffer_size,
        }))?;
        let session = BackendSession::new()?;
        session.set_audio_driver(&driver)?;
        driver.wait_process();
        Ok(Self {
            session,
            driver,
            loops: BTreeMap::new(),
            loop_channels: BTreeMap::new(),
            tracks: BTreeMap::new(),
            next_loop_id: 1,
            next_track_id: 1,
        })
    }

    fn engine_loop(&self, id: BackendLoopId) -> Result<&EngineLoop> {
        self.loops
            .get(&id)
            .ok_or_else(|| anyhow!("unknown backend loop {id:?}"))
    }

    fn create_track_loop(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let (audio_inputs, audio_outputs, midi_input, midi_output) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
            (
                track.audio_inputs.clone(),
                track.audio_outputs.clone(),
                track.midi_input.clone(),
                track.midi_output.clone(),
            )
        };
        let loop_id = self.create_loop()?;
        let engine_loop = self.engine_loop(loop_id)?.clone();
        let mut audio = Vec::with_capacity(audio_inputs.len());
        for (input, output) in audio_inputs.iter().zip(&audio_outputs) {
            let channel = engine_loop.add_audio_channel(ChannelMode::Direct)?;
            channel.connect_input(input)?;
            channel.connect_output(output)?;
            audio.push(channel);
        }
        let mut midi = Vec::new();
        if let (Some(input), Some(output)) = (midi_input.as_ref(), midi_output.as_ref()) {
            let channel = engine_loop.add_midi_channel(ChannelMode::Direct)?;
            channel.connect_input(input)?;
            channel.connect_output(output)?;
            midi.push(channel);
        }
        self.loop_channels
            .insert(loop_id, EngineLoopChannels { audio, midi });
        self.tracks
            .get_mut(&track_id)
            .expect("track was validated before loop construction")
            .loops
            .push(loop_id);
        Ok(loop_id)
    }
}

fn balance_factors(balance: f32) -> (f32, f32) {
    let balance = balance.clamp(-1.0, 1.0);
    if balance < 0.0 {
        (1.0, 1.0 + balance)
    } else {
        (1.0 - balance, 1.0)
    }
}

fn db_gain(db: f32) -> f32 {
    10.0_f32.powf(db / 20.0)
}

fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude > 0.0 {
        20.0 * amplitude.log10()
    } else {
        -200.0
    }
}

impl Backend for EngineBackend {
    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let engine_loop = self.session.create_loop()?;
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(id, engine_loop);
        Ok(id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        if request.audio_channels > 10 {
            return Err(anyhow!("direct track audio channel count exceeds 10"));
        }
        let ring = self.driver.get_sample_rate().saturating_mul(30);
        let mut audio_inputs = Vec::with_capacity(request.audio_channels as usize);
        let mut audio_outputs = Vec::with_capacity(request.audio_channels as usize);
        for index in 0..request.audio_channels {
            let suffix = if request.audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            let input = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &format!("{}_direct_in{suffix}", request.port_name_base),
                &PortDirection::Input,
                ring,
            )?;
            let output = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &format!("{}_direct_out{suffix}", request.port_name_base),
                &PortDirection::Output,
                0,
            )?;
            input.connect_internal(&output)?;
            input.set_passthrough_muted(true)?;
            audio_inputs.push(input);
            audio_outputs.push(output);
        }
        let (midi_input, midi_output) = if request.midi {
            let input = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &format!("{}_direct_midi_in", request.port_name_base),
                &PortDirection::Input,
                ring,
            )?;
            let output = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &format!("{}_direct_midi_out", request.port_name_base),
                &PortDirection::Output,
                0,
            )?;
            input.connect_internal(&output)?;
            input.set_passthrough_muted(true)?;
            (Some(input), Some(output))
        } else {
            (None, None)
        };
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            EngineTrack {
                audio_inputs,
                audio_outputs,
                midi_input,
                midi_output,
                loops: Vec::new(),
                output_gain_db: 0.0,
                output_balance: 0.0,
                output_muted: false,
                input_gain_db: 0.0,
                input_balance: 0.0,
                input_monitoring: false,
            },
        );
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.create_track_loop(track_id)?);
        }
        self.driver.wait_process();
        Ok(BackendTrackCreation { track_id, loops })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let loop_id = self.create_track_loop(track_id)?;
        self.driver.wait_process();
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown backend track {track_id:?}"))?;
        match control {
            BackendTrackControl::OutputGainDb(value) => track.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => {
                track.output_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::OutputMute(value) => {
                track.output_muted = value;
                for port in &track.audio_outputs {
                    port.set_muted(value)?;
                }
                if let Some(port) = &track.midi_output {
                    port.set_muted(value)?;
                }
            }
            BackendTrackControl::InputGainDb(value) => track.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => {
                track.input_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::InputMonitoring(value) => {
                track.input_monitoring = value;
                for port in &track.audio_inputs {
                    port.set_passthrough_muted(!value)?;
                }
                if let Some(port) = &track.midi_input {
                    port.set_passthrough_muted(!value)?;
                }
            }
        }
        let (left, right) = balance_factors(track.output_balance);
        let base = db_gain(track.output_gain_db);
        for (index, port) in track.audio_outputs.iter().enumerate() {
            let factor = if track.audio_outputs.len() == 2 {
                if index == 0 {
                    left
                } else {
                    right
                }
            } else {
                1.0
            };
            port.set_gain(base * factor)?;
        }
        let (left, right) = balance_factors(track.input_balance);
        let base = db_gain(track.input_gain_db);
        for (index, port) in track.audio_inputs.iter().enumerate() {
            let factor = if track.audio_inputs.len() == 2 {
                if index == 0 {
                    left
                } else {
                    right
                }
            } else {
                1.0
            };
            port.set_gain(base * factor)?;
        }
        Ok(())
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        for channel in &channels.audio {
            channel.set_gain(gain.clamp(0.0, 1.0))?;
        }
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Vec<Arc<[f32]>>> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        Ok(channels
            .audio
            .iter()
            .map(|channel| Arc::from(channel.get_data()))
            .collect())
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        let target = self.engine_loop(loop_id)?;
        let source = source.map(|id| self.engine_loop(id)).transpose()?;
        target.set_sync_source(source)?;
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let delay = cycles_delay
            .map(|delay| i32::try_from(delay).unwrap_or(i32::MAX))
            .unwrap_or(-1);
        self.engine_loop(loop_id)?
            .transition(to_engine_mode(mode), delay, -1)?;
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.engine_loop(loop_id)?.clear(0)?;
        Ok(())
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let driver = self.driver.get_state();
        let mut tracks = BTreeMap::new();
        for (id, track) in &self.tracks {
            let input_peaks = track
                .audio_inputs
                .iter()
                .filter_map(AudioPort::poll_state)
                .map(|state| amplitude_db(state.input_peak))
                .collect();
            let output_peaks = track
                .audio_outputs
                .iter()
                .filter_map(AudioPort::poll_state)
                .map(|state| amplitude_db(state.output_peak))
                .collect();
            let input_midi_activity = track
                .midi_input
                .as_ref()
                .and_then(MidiPort::poll_state)
                .is_some_and(|state| state.n_input_events > 0 || state.n_input_notes_active > 0);
            let output_midi_activity = track
                .midi_output
                .as_ref()
                .and_then(MidiPort::poll_state)
                .is_some_and(|state| state.n_output_events > 0 || state.n_output_notes_active > 0);
            tracks.insert(
                *id,
                BackendTrackState {
                    audio_channels: track.audio_inputs.len() as u8,
                    midi: track.midi_input.is_some(),
                    output_gain_db: track.output_gain_db,
                    output_balance: track.output_balance,
                    output_muted: track.output_muted,
                    input_gain_db: track.input_gain_db,
                    input_balance: track.input_balance,
                    input_monitoring: track.input_monitoring,
                    input_peaks,
                    output_peaks,
                    input_midi_activity,
                    output_midi_activity,
                },
            );
        }
        let mut loops = BTreeMap::new();
        for (id, engine_loop) in &self.loops {
            if let Some(state) = engine_loop.poll_state() {
                let channels = self.loop_channels.get(id);
                let audio_states: Vec<_> = channels
                    .into_iter()
                    .flat_map(|channels| &channels.audio)
                    .filter_map(AudioChannel::poll_state)
                    .collect();
                let midi_activity = channels
                    .into_iter()
                    .flat_map(|channels| &channels.midi)
                    .filter_map(MidiChannel::poll_state)
                    .any(|state| state.n_events_triggered > 0 || state.n_notes_active > 0);
                loops.insert(
                    *id,
                    BackendLoopState {
                        mode: from_engine_mode(state.mode),
                        length: state.length,
                        position: state.position,
                        next_mode: state.maybe_next_mode.map(from_engine_mode),
                        next_transition_delay: state.maybe_next_mode_delay,
                        stereo: audio_states.len() == 2,
                        gain: audio_states.first().map(|state| state.gain).unwrap_or(1.0),
                        audio_peaks: audio_states
                            .iter()
                            .map(|state| amplitude_db(state.output_peak))
                            .collect(),
                        midi_activity,
                    },
                );
            }
        }
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: driver.dsp_load_percent,
                xruns: driver.xruns_since_last,
                buffer_size: driver.buffer_size,
                sample_rate: driver.sample_rate,
            },
            tracks,
            loops,
        })
    }

    fn wait_idle(&mut self) {
        self.driver.wait_process();
    }
}

fn from_engine_mode(mode: LoopMode) -> BackendLoopMode {
    match mode {
        LoopMode::Unknown => BackendLoopMode::Unknown,
        LoopMode::Stopped => BackendLoopMode::Stopped,
        LoopMode::Playing => BackendLoopMode::Playing,
        LoopMode::Recording => BackendLoopMode::Recording,
        LoopMode::Replacing => BackendLoopMode::Replacing,
        LoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        LoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}

fn to_engine_mode(mode: BackendLoopMode) -> LoopMode {
    match mode {
        BackendLoopMode::Unknown => LoopMode::Unknown,
        BackendLoopMode::Stopped => LoopMode::Stopped,
        BackendLoopMode::Playing => LoopMode::Playing,
        BackendLoopMode::Recording => LoopMode::Recording,
        BackendLoopMode::Replacing => LoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => LoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => LoopMode::RecordingDryIntoWet,
    }
}

#[derive(Debug)]
pub struct FakeBackend {
    status: BackendStatus,
    tracks: BTreeMap<BackendTrackId, FakeTrack>,
    loops: BTreeMap<BackendLoopId, BackendLoopState>,
    sync_sources: BTreeMap<BackendLoopId, Option<BackendLoopId>>,
    next_loop_id: u64,
    next_track_id: u64,
    fail_track_creation_after: Option<usize>,
    operations: Vec<FakeOperation>,
}

#[derive(Debug)]
struct FakeTrack {
    state: BackendTrackState,
    loops: Vec<BackendLoopId>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FakeOperation {
    CreateLoop(BackendLoopId),
    CreateTrack(BackendTrackId),
    AddTrackLoop(BackendTrackId, BackendLoopId),
    SetTrackControl(BackendTrackId, BackendTrackControl),
    SetLoopGain(BackendLoopId, f32),
    SetSyncSource(BackendLoopId, Option<BackendLoopId>),
    Transition(BackendLoopId, BackendLoopMode, Option<u32>),
    Clear(BackendLoopId),
}

impl Default for FakeBackend {
    fn default() -> Self {
        Self {
            status: BackendStatus {
                buffer_size: 256,
                sample_rate: 48_000,
                ..Default::default()
            },
            tracks: BTreeMap::new(),
            loops: BTreeMap::new(),
            sync_sources: BTreeMap::new(),
            next_loop_id: 1,
            next_track_id: 1,
            fail_track_creation_after: None,
            operations: Vec::new(),
        }
    }
}

impl FakeBackend {
    pub fn operations(&self) -> &[FakeOperation] {
        &self.operations
    }

    pub fn fail_track_creation_after(&mut self, successful_creations: usize) {
        self.fail_track_creation_after = Some(successful_creations);
    }

    fn require_loop(&self, id: BackendLoopId) -> Result<()> {
        self.loops
            .contains_key(&id)
            .then_some(())
            .ok_or_else(|| anyhow!("unknown fake loop {id:?}"))
    }
}

impl Backend for FakeBackend {
    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            BackendLoopState {
                mode: BackendLoopMode::Stopped,
                ..Default::default()
            },
        );
        self.sync_sources.insert(id, None);
        self.operations.push(FakeOperation::CreateLoop(id));
        Ok(id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        if let Some(remaining) = self.fail_track_creation_after.as_mut() {
            if *remaining == 0 {
                self.fail_track_creation_after = None;
                return Err(anyhow!("injected track creation failure"));
            }
            *remaining -= 1;
        }
        if request.audio_channels > 10 {
            return Err(anyhow!("direct track audio channel count exceeds 10"));
        }
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            FakeTrack {
                state: BackendTrackState {
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                    ..Default::default()
                },
                loops: Vec::new(),
            },
        );
        self.operations.push(FakeOperation::CreateTrack(track_id));
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.add_loop_to_track(track_id)?);
        }
        Ok(BackendTrackCreation { track_id, loops })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        if !self.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown fake track {track_id:?}"));
        }
        let loop_id = self.create_loop()?;
        let track = self.tracks.get_mut(&track_id).expect("track was checked");
        track.loops.push(loop_id);
        if let Some(state) = self.loops.get_mut(&loop_id) {
            state.stereo = track.state.audio_channels == 2;
            state.gain = 1.0;
            state.audio_peaks = vec![-200.0; track.state.audio_channels as usize];
        }
        self.operations
            .push(FakeOperation::AddTrackLoop(track_id, loop_id));
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown fake track {track_id:?}"))?;
        match control {
            BackendTrackControl::OutputGainDb(value) => track.state.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => track.state.output_balance = value,
            BackendTrackControl::OutputMute(value) => track.state.output_muted = value,
            BackendTrackControl::InputGainDb(value) => track.state.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => track.state.input_balance = value,
            BackendTrackControl::InputMonitoring(value) => track.state.input_monitoring = value,
        }
        self.operations
            .push(FakeOperation::SetTrackControl(track_id, control));
        Ok(())
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.gain = gain.clamp(0.0, 1.0);
        self.operations
            .push(FakeOperation::SetLoopGain(loop_id, state.gain));
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Vec<Arc<[f32]>>> {
        self.require_loop(loop_id)?;
        let n_channels = self
            .tracks
            .values()
            .find(|track| track.loops.contains(&loop_id))
            .map(|track| track.state.audio_channels)
            .unwrap_or(0);
        Ok((0..n_channels)
            .map(|_| Arc::from(Vec::<f32>::new()))
            .collect())
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.require_loop(loop_id)?;
        if let Some(source) = source {
            self.require_loop(source)?;
        }
        self.sync_sources.insert(loop_id, source);
        self.operations
            .push(FakeOperation::SetSyncSource(loop_id, source));
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        state.mode = mode;
        state.next_mode = None;
        state.next_transition_delay = None;
        self.operations
            .push(FakeOperation::Transition(loop_id, mode, cycles_delay));
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let state = self
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown fake loop {loop_id:?}"))?;
        *state = BackendLoopState {
            mode: BackendLoopMode::Stopped,
            ..Default::default()
        };
        self.operations.push(FakeOperation::Clear(loop_id));
        Ok(())
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        Ok(BackendSnapshot {
            status: self.status,
            tracks: self
                .tracks
                .iter()
                .map(|(id, track)| (*id, track.state.clone()))
                .collect(),
            loops: self.loops.clone(),
        })
    }

    fn wait_idle(&mut self) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    fn backend_contract(backend: &mut dyn Backend) {
        let sync = backend.create_loop().unwrap();
        let follower = backend.create_loop().unwrap();
        backend.wait_idle();
        backend
            .transition_loop(follower, BackendLoopMode::Playing, None)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.loops.contains_key(&sync));
        assert_eq!(
            snapshot.loops.get(&follower).unwrap().mode,
            BackendLoopMode::Playing
        );
        backend.set_loop_sync_source(follower, Some(sync)).unwrap();
        backend.wait_idle();
    }

    fn direct_track_contract(backend: &mut dyn Backend) {
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "contract".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::OutputGainDb(-6.0))
            .unwrap();
        backend.set_loop_gain(created.loops[0], 0.5).unwrap();
        let third = backend.add_loop_to_track(created.track_id).unwrap();
        backend.wait_idle();
        let snapshot = backend.poll().unwrap();
        let track = &snapshot.tracks[&created.track_id];
        assert_eq!(track.audio_channels, 2);
        assert!(track.midi);
        assert_eq!(track.output_gain_db, -6.0);
        assert!(snapshot.loops[&created.loops[0]].stereo);
        assert!(snapshot.loops.contains_key(&third));
        assert_eq!(backend.loop_audio_data(created.loops[0]).unwrap().len(), 2);
    }

    #[test]
    fn fake_backend_satisfies_contracts() {
        let mut backend = FakeBackend::default();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
    }

    #[test]
    fn engine_dummy_backend_satisfies_contracts() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend_contract(&mut backend);
        direct_track_contract(&mut backend);
    }
}
