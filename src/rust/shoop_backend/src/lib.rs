use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use shoop_engine::dummy_midi_port::DummyMidiPort;
use shoop_engine::dummy_port::{DummyAudioPort, PortId};
use shoop_engine::external_audio_port::ExternalAudioPort;
use shoop_engine::session::{Port, Session};
use shoop_engine::{ChannelMode, LoopMode, PortDirection};

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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum BackendDriverState {
    #[default]
    Dummy,
    AwaitingGesture,
    RequestingPermission,
    Starting,
    Running,
    Suspended,
    Denied,
    Unsupported,
    Failed,
    Stopped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BackendStatus {
    pub dsp_load_percent: f32,
    pub xruns: u32,
    pub buffer_size: u32,
    pub sample_rate: u32,
    pub driver_state: BackendDriverState,
    pub callback_count: u64,
    pub processed_frames: u64,
    pub input_peak: f32,
    pub output_peak: f32,
    pub callback_budget_overruns: u32,
    pub render_discontinuities: u32,
    pub memory_growths: u32,
    pub command_overflows: u32,
    pub storage_low_channels: u32,
    pub storage_exhaustions: u32,
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

pub trait Backend {
    fn create_loop(&mut self) -> Result<BackendLoopId>;
    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation>;
    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId>;
    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()>;
    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()>;
    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>>;
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
    fn advance(&mut self, elapsed: Duration);
    fn poll(&mut self) -> Result<BackendSnapshot>;
    fn wait_idle(&mut self);
}

const NANOSECONDS_PER_SECOND: u128 = 1_000_000_000;
const MAX_CYCLES_PER_ADVANCE: u32 = 8;
pub const MAX_WEB_AUDIO_QUANTUM: u32 = 2048;
pub const RECORDING_CAPACITY_SECONDS: u32 = 10;
const RECORDING_CHUNK_SIZE: usize = 4096;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum EngineBackendMode {
    Dummy,
    Physical,
}

pub struct EngineBackend {
    session: Session,
    sample_rate: u32,
    buffer_size: u32,
    elapsed_frame_numerator: u128,
    processed_frames: u64,
    xruns: u32,
    loops: BTreeMap<BackendLoopId, usize>,
    loop_channels: BTreeMap<BackendLoopId, EngineLoopChannels>,
    tracks: BTreeMap<BackendTrackId, EngineTrack>,
    next_loop_id: u64,
    next_track_id: u64,
    next_port_id: u64,
    mode: EngineBackendMode,
    callback_count: u64,
    input_peak: f32,
    output_peak: f32,
    last_quantum: u32,
}

struct EngineLoopChannels {
    audio: Vec<usize>,
    midi: Vec<usize>,
}

struct EngineTrack {
    audio_inputs: Vec<usize>,
    audio_outputs: Vec<usize>,
    midi_input: Option<usize>,
    midi_output: Option<usize>,
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
        if sample_rate == 0 || buffer_size == 0 {
            return Err(anyhow!(
                "dummy sample rate and buffer size must be non-zero"
            ));
        }
        let mut session = Session::default();
        session.set_sample_rate(sample_rate);
        session.set_buffer_size(buffer_size);
        Ok(Self {
            session,
            sample_rate,
            buffer_size,
            elapsed_frame_numerator: 0,
            processed_frames: 0,
            xruns: 0,
            loops: BTreeMap::new(),
            loop_channels: BTreeMap::new(),
            tracks: BTreeMap::new(),
            next_loop_id: 1,
            next_track_id: 1,
            next_port_id: 1,
            mode: EngineBackendMode::Dummy,
            callback_count: 0,
            input_peak: 0.0,
            output_peak: 0.0,
            last_quantum: buffer_size,
        })
    }

    pub fn new_web_audio(sample_rate: u32, max_quantum: u32) -> Result<Self> {
        if sample_rate == 0 || max_quantum == 0 || max_quantum > MAX_WEB_AUDIO_QUANTUM {
            return Err(anyhow!(
                "Web Audio sample rate must be non-zero and quantum must be in 1..={MAX_WEB_AUDIO_QUANTUM}"
            ));
        }
        let mut backend = Self::new_dummy(sample_rate, max_quantum)?;
        backend.mode = EngineBackendMode::Physical;
        Ok(backend)
    }

    pub fn process_audio_quantum(
        &mut self,
        input: &[f32],
        input_channels: usize,
        output: &mut [f32],
        output_channels: usize,
        n_frames: usize,
    ) -> Result<()> {
        if self.mode != EngineBackendMode::Physical {
            return Err(anyhow!("audio quantum supplied to a non-physical backend"));
        }
        if n_frames == 0
            || n_frames > self.buffer_size as usize
            || input_channels.saturating_mul(n_frames) > input.len()
            || output_channels.saturating_mul(n_frames) > output.len()
        {
            return Err(anyhow!("invalid Web Audio channel or quantum shape"));
        }

        self.input_peak = 0.0;
        for track in self.tracks.values() {
            for (channel, port) in track.audio_inputs.iter().enumerate() {
                let source_channel = if input_channels == 0 {
                    None
                } else {
                    Some(channel.min(input_channels - 1))
                };
                let samples = source_channel
                    .map(|channel| &input[channel * n_frames..channel.saturating_add(1) * n_frames])
                    .unwrap_or(&[]);
                self.input_peak = samples
                    .iter()
                    .fold(self.input_peak, |peak, sample| peak.max(sample.abs()));
                self.session
                    .port_mut(*port)
                    .and_then(Port::as_external_mut)
                    .ok_or_else(|| anyhow!("missing physical audio input port"))?
                    .stage_input(samples);
            }
        }

        self.session.process(n_frames);
        output[..output_channels * n_frames].fill(0.0);
        self.output_peak = 0.0;
        for track in self.tracks.values() {
            for (channel, port) in track.audio_outputs.iter().enumerate() {
                let samples = self
                    .session
                    .port(*port)
                    .and_then(Port::as_external)
                    .ok_or_else(|| anyhow!("missing physical audio output port"))?
                    .output(n_frames);
                if track.audio_outputs.len() == 1 {
                    for destination in 0..output_channels {
                        for (target, sample) in output
                            [destination * n_frames..(destination + 1) * n_frames]
                            .iter_mut()
                            .zip(samples)
                        {
                            *target += *sample;
                        }
                    }
                } else if output_channels > 0 {
                    let destination = channel.min(output_channels - 1);
                    for (target, sample) in output
                        [destination * n_frames..(destination + 1) * n_frames]
                        .iter_mut()
                        .zip(samples)
                    {
                        *target += *sample;
                    }
                }
            }
        }
        for sample in &mut output[..output_channels * n_frames] {
            self.output_peak = self.output_peak.max(sample.abs());
            *sample = sample.clamp(-1.0, 1.0);
        }
        self.callback_count = self.callback_count.saturating_add(1);
        self.processed_frames = self.processed_frames.saturating_add(n_frames as u64);
        self.last_quantum = n_frames as u32;
        Ok(())
    }

    pub fn advance_frames(&mut self, mut frames: u32) {
        while frames > 0 {
            let chunk = frames.min(self.buffer_size);
            self.session.process(chunk as usize);
            self.processed_frames = self.processed_frames.saturating_add(chunk as u64);
            frames -= chunk;
        }
    }

    pub fn processed_frames(&self) -> u64 {
        self.processed_frames
    }

    fn next_port_id(&mut self) -> PortId {
        let id = PortId(self.next_port_id);
        self.next_port_id = self.next_port_id.saturating_add(1);
        id
    }

    fn engine_loop_index(&self, id: BackendLoopId) -> Result<usize> {
        self.loops
            .get(&id)
            .copied()
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
                track.midi_input,
                track.midi_output,
            )
        };
        let loop_id = self.create_loop()?;
        let engine_loop = self.engine_loop_index(loop_id)?;
        let mut audio = Vec::with_capacity(audio_inputs.len());
        for (input, output) in audio_inputs.iter().zip(&audio_outputs) {
            let channel = if self.mode == EngineBackendMode::Physical {
                self.session.add_audio_channel_with_bounded_capacity(
                    engine_loop,
                    RECORDING_CHUNK_SIZE,
                    self.sample_rate as usize * RECORDING_CAPACITY_SECONDS as usize,
                    ChannelMode::Direct,
                )?
            } else {
                self.session
                    .add_audio_channel(engine_loop, 64, ChannelMode::Direct)?
            };
            self.session.connect_channel_input(channel, *input)?;
            self.session.connect_channel_output(channel, *output)?;
            audio.push(channel);
        }
        let mut midi = Vec::new();
        if let (Some(input), Some(output)) = (midi_input, midi_output) {
            let channel = self
                .session
                .add_midi_channel(engine_loop, 1024, ChannelMode::Direct)?;
            self.session.connect_channel_input(channel, input)?;
            self.session.connect_channel_output(channel, output)?;
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

    fn apply_graph_changes(&mut self) -> Result<()> {
        self.session
            .apply_graph_changes()
            .map_err(|error| anyhow!("could not apply dummy engine graph: {error}"))
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
        let engine_loop = self.session.create_loop();
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(id, engine_loop);
        Ok(id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        if request.audio_channels > 10 {
            return Err(anyhow!("direct track audio channel count exceeds 10"));
        }
        let mut audio_inputs = Vec::with_capacity(request.audio_channels as usize);
        let mut audio_outputs = Vec::with_capacity(request.audio_channels as usize);
        for index in 0..request.audio_channels {
            let suffix = if request.audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            let input_name = format!("{}_direct_in{suffix}", request.port_name_base);
            let output_name = format!("{}_direct_out{suffix}", request.port_name_base);
            let (input, output) = if self.mode == EngineBackendMode::Physical {
                let mut input = ExternalAudioPort::new(
                    input_name,
                    PortDirection::Input,
                    self.buffer_size as usize,
                );
                input.audio_mut().set_passthrough_muted(true);
                let input = self.session.add_port(Port::External(input));
                let output = self.session.add_port(Port::External(ExternalAudioPort::new(
                    output_name,
                    PortDirection::Output,
                    self.buffer_size as usize,
                )));
                (input, output)
            } else {
                let mut input = DummyAudioPort::new(
                    self.next_port_id(),
                    input_name,
                    PortDirection::Input,
                    self.buffer_size as usize,
                );
                input.audio_mut().set_passthrough_muted(true);
                let input = self.session.add_port(Port::Dummy(input));
                let output_id = self.next_port_id();
                let output = self.session.add_port(Port::Dummy(DummyAudioPort::new(
                    output_id,
                    output_name,
                    PortDirection::Output,
                    1,
                )));
                (input, output)
            };
            self.session.connect_ports_internal(input, output)?;
            audio_inputs.push(input);
            audio_outputs.push(output);
        }
        let (midi_input, midi_output) = if request.midi {
            let mut input = DummyMidiPort::new(
                self.next_port_id(),
                format!("{}_direct_midi_in", request.port_name_base),
                PortDirection::Input,
            );
            input.midi_mut().set_passthrough_muted(true);
            let input = self.session.add_port(Port::DummyMidi(input));
            let output_id = self.next_port_id();
            let output = self.session.add_port(Port::DummyMidi(DummyMidiPort::new(
                output_id,
                format!("{}_direct_midi_out", request.port_name_base),
                PortDirection::Output,
            )));
            self.session.connect_ports_internal(input, output)?;
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
        self.apply_graph_changes()?;
        Ok(BackendTrackCreation { track_id, loops })
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let loop_id = self.create_track_loop(track_id)?;
        self.apply_graph_changes()?;
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
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .ok_or_else(|| anyhow!("missing audio output port"))?
                        .set_muted(value);
                }
                if let Some(port) = track.midi_output {
                    self.session
                        .port_mut(port)
                        .and_then(Port::midi_mut)
                        .ok_or_else(|| anyhow!("missing MIDI output port"))?
                        .set_muted(value);
                }
            }
            BackendTrackControl::InputGainDb(value) => track.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => {
                track.input_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::InputMonitoring(value) => {
                track.input_monitoring = value;
                for port in &track.audio_inputs {
                    self.session
                        .port_mut(*port)
                        .and_then(Port::audio_mut)
                        .ok_or_else(|| anyhow!("missing audio input port"))?
                        .set_passthrough_muted(!value);
                }
                if let Some(port) = track.midi_input {
                    self.session
                        .port_mut(port)
                        .and_then(Port::midi_mut)
                        .ok_or_else(|| anyhow!("missing MIDI input port"))?
                        .set_passthrough_muted(!value);
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
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing audio output port"))?
                .set_gain(base * factor);
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
            self.session
                .port_mut(*port)
                .and_then(Port::audio_mut)
                .ok_or_else(|| anyhow!("missing audio input port"))?
                .set_gain(base * factor);
        }
        Ok(())
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        for channel in &channels.audio {
            self.session
                .audio_channel_mut(*channel)
                .ok_or_else(|| anyhow!("missing audio loop channel"))?
                .set_gain(gain.clamp(0.0, 1.0));
        }
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        let channels = self
            .loop_channels
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown backend loop channels {loop_id:?}"))?;
        channels
            .audio
            .iter()
            .map(|channel| {
                self.session
                    .audio_channel(*channel)
                    .map(|channel| Arc::from(channel.data()))
                    .ok_or_else(|| anyhow!("missing audio loop channel"))
            })
            .collect::<Result<Vec<_>>>()
            .map(Some)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        let target = self.engine_loop_index(loop_id)?;
        let source = source.map(|id| self.engine_loop_index(id)).transpose()?;
        self.session.set_loop_sync_source(target, source)?;
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        if let Some(delay) = cycles_delay {
            self.session
                .loop_mut(engine_loop)
                .ok_or_else(|| anyhow!("missing engine loop"))?
                .plan_transition(to_engine_mode(mode), Some(delay), None);
        } else {
            self.session
                .set_loop_mode(engine_loop, to_engine_mode(mode))?;
        }
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let engine_loop = self.engine_loop_index(loop_id)?;
        self.session
            .loop_mut(engine_loop)
            .ok_or_else(|| anyhow!("missing engine loop"))?
            .clear(0);
        Ok(())
    }

    fn advance(&mut self, elapsed: Duration) {
        if self.mode == EngineBackendMode::Physical {
            return;
        }
        self.elapsed_frame_numerator = self
            .elapsed_frame_numerator
            .saturating_add(elapsed.as_nanos().saturating_mul(self.sample_rate as u128));
        let due = self.elapsed_frame_numerator / NANOSECONDS_PER_SECOND;
        let max_frames = self.buffer_size.saturating_mul(MAX_CYCLES_PER_ADVANCE) as u128;
        let processed = due.min(max_frames) as u32;
        self.elapsed_frame_numerator -= processed as u128 * NANOSECONDS_PER_SECOND;
        if due > max_frames {
            self.elapsed_frame_numerator = 0;
            self.xruns = self.xruns.saturating_add(1);
        }
        self.advance_frames(processed);
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let mut tracks = BTreeMap::new();
        for (id, track) in &self.tracks {
            let input_peaks = track
                .audio_inputs
                .iter()
                .map(|port| {
                    self.session
                        .port(*port)
                        .and_then(Port::audio)
                        .map(|port| amplitude_db(port.input_peak()))
                        .unwrap_or(-200.0)
                })
                .collect();
            let output_peaks = track
                .audio_outputs
                .iter()
                .map(|port| {
                    self.session
                        .port(*port)
                        .and_then(Port::audio)
                        .map(|port| amplitude_db(port.output_peak()))
                        .unwrap_or(-200.0)
                })
                .collect();
            let input_midi_activity = track.midi_input.is_some_and(|port| {
                self.session
                    .port(port)
                    .and_then(Port::midi)
                    .is_some_and(|port| port.n_input_events() > 0 || port.n_notes_active() > 0)
            });
            let output_midi_activity = track.midi_output.is_some_and(|port| {
                self.session
                    .port(port)
                    .and_then(Port::midi)
                    .is_some_and(|port| port.n_output_events() > 0 || port.n_notes_active() > 0)
            });
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
            let Some(state) = self.session.loop_(*engine_loop) else {
                continue;
            };
            let channels = self.loop_channels.get(id);
            let audio: Vec<_> = channels
                .into_iter()
                .flat_map(|channels| &channels.audio)
                .filter_map(|channel| self.session.audio_channel(*channel))
                .collect();
            let midi_activity = channels
                .into_iter()
                .flat_map(|channels| &channels.midi)
                .filter_map(|channel| self.session.midi_channel(*channel))
                .any(|channel| channel.n_events_triggered() > 0 || channel.n_notes_active() > 0);
            loops.insert(
                *id,
                BackendLoopState {
                    mode: from_engine_mode(state.mode()),
                    length: state.length(),
                    position: state.position(),
                    next_mode: state
                        .first_planned_transition()
                        .map(|(mode, _)| from_engine_mode(mode)),
                    next_transition_delay: state.first_planned_transition().map(|(_, delay)| delay),
                    stereo: audio.len() == 2,
                    gain: audio.first().map(|channel| channel.gain()).unwrap_or(1.0),
                    audio_peaks: audio
                        .iter()
                        .map(|channel| amplitude_db(channel.output_peak()))
                        .collect(),
                    midi_activity,
                },
            );
        }
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: 0.0,
                xruns: self.xruns,
                buffer_size: if self.mode == EngineBackendMode::Physical {
                    self.last_quantum
                } else {
                    self.buffer_size
                },
                sample_rate: self.sample_rate,
                driver_state: if self.mode == EngineBackendMode::Physical {
                    BackendDriverState::Running
                } else {
                    BackendDriverState::Dummy
                },
                callback_count: self.callback_count,
                processed_frames: self.processed_frames,
                input_peak: self.input_peak,
                output_peak: self.output_peak,
                callback_budget_overruns: 0,
                render_discontinuities: 0,
                memory_growths: 0,
                command_overflows: 0,
                storage_low_channels: self
                    .loop_channels
                    .values()
                    .flat_map(|channels| &channels.audio)
                    .filter_map(|channel| self.session.audio_channel(*channel))
                    .filter(|channel| {
                        channel
                            .storage_remaining()
                            .is_some_and(|remaining| remaining <= self.sample_rate as usize)
                    })
                    .count()
                    .min(u32::MAX as usize) as u32,
                storage_exhaustions: self
                    .loop_channels
                    .values()
                    .flat_map(|channels| &channels.audio)
                    .filter_map(|channel| self.session.audio_channel(*channel))
                    .map(|channel| channel.storage_exhaustions())
                    .sum(),
            },
            tracks,
            loops,
        })
    }

    fn wait_idle(&mut self) {
        let _ = self.apply_graph_changes();
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

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        self.require_loop(loop_id)?;
        let n_channels = self
            .tracks
            .values()
            .find(|track| track.loops.contains(&loop_id))
            .map(|track| track.state.audio_channels)
            .unwrap_or(0);
        Ok(Some(
            (0..n_channels)
                .map(|_| Arc::from(Vec::<f32>::new()))
                .collect(),
        ))
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

    fn advance(&mut self, _elapsed: Duration) {}

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
            .transition_loop(follower, BackendLoopMode::Recording, None)
            .unwrap();
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.loops.contains_key(&sync));
        assert_eq!(
            snapshot.loops.get(&follower).unwrap().mode,
            BackendLoopMode::Recording
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
        assert_eq!(
            backend
                .loop_audio_data(created.loops[0])
                .unwrap()
                .unwrap()
                .len(),
            2
        );
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

    #[test]
    fn cooperative_dummy_records_and_plays_real_engine_frames() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "cooperative".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let loop_id = track.loops[0];
        backend
            .transition_loop(loop_id, BackendLoopMode::Recording, None)
            .unwrap();
        backend.advance_frames(512);
        let recording = backend.poll().unwrap().loops[&loop_id].clone();
        assert_eq!(recording.mode, BackendLoopMode::Recording);
        assert_eq!(recording.length, 512);

        backend
            .transition_loop(loop_id, BackendLoopMode::Playing, None)
            .unwrap();
        backend.advance_frames(256);
        let playing = backend.poll().unwrap().loops[&loop_id].clone();
        assert_eq!(playing.mode, BackendLoopMode::Playing);
        assert_eq!(playing.position, 256);
        assert_eq!(
            backend.loop_audio_data(loop_id).unwrap().unwrap()[0].len(),
            512
        );
    }

    #[test]
    fn web_audio_backend_records_monitors_and_plays_non_zero_full_duplex_audio() {
        let mut backend = EngineBackend::new_web_audio(48_000, 128).unwrap();
        let track = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "web".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        let input = vec![0.25; 128];
        let mut output = vec![0.0; 256];
        assert_no_alloc::assert_no_alloc(|| {
            backend
                .process_audio_quantum(&input, 1, &mut output, 2, 128)
                .unwrap();
        });
        assert!(output[..128].iter().all(|sample| *sample == 0.25));
        assert!(output[128..].iter().all(|sample| *sample == 0.25));
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        let recorded = backend.loop_audio_data(track.loops[0]).unwrap().unwrap();
        assert_eq!(recorded[0].len(), 128);
        assert!(recorded[0].iter().all(|sample| *sample == 0.25));
        backend
            .set_track_control(track.track_id, BackendTrackControl::InputMonitoring(false))
            .unwrap();
        backend
            .transition_loop(track.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        output.fill(0.0);
        backend
            .process_audio_quantum(&vec![0.0; 128], 1, &mut output, 2, 128)
            .unwrap();
        assert!(output.iter().any(|sample| *sample != 0.0));
        let status = backend.poll().unwrap().status;
        assert_eq!(status.callback_count, 2);
        assert_eq!(status.processed_frames, 256);
        assert!(status.input_peak == 0.0);
        assert!(status.output_peak > 0.0);
    }

    #[test]
    fn elapsed_time_preserves_fractional_frame_remainders() {
        let mut backend = EngineBackend::new_dummy(1_000, 64).unwrap();
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 0);
        backend.advance(Duration::from_micros(500));
        assert_eq!(backend.processed_frames(), 1);
    }

    #[test]
    fn elapsed_time_processing_is_bounded_and_reports_dropped_time() {
        let mut backend = EngineBackend::new_dummy(48_000, 256).unwrap();
        backend.advance(Duration::from_secs(10));
        assert_eq!(
            backend.processed_frames(),
            u64::from(256 * MAX_CYCLES_PER_ADVANCE)
        );
        assert_eq!(backend.poll().unwrap().status.xruns, 1);

        backend.advance(Duration::from_millis(1));
        assert_eq!(
            backend.processed_frames(),
            u64::from(256 * MAX_CYCLES_PER_ADVANCE + 48)
        );
    }
}
