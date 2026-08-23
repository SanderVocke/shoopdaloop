mod host_midi;
mod transport;

pub use host_midi::{
    HostMidiBridge, HostMidiDirection, HostMidiEndpoint, HostMidiInput, InMemoryHostMidiBridge,
    NullHostMidiBridge,
};
pub use transport::{
    ConnectionState, MessageEndpoint, ProtocolState, RemoteBackendControl, RemoteEngineState,
    RemoteReadiness, ReplayState, TransportDiagnostics,
};

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverDescriptor, AudioDriverKind, AudioDriverRuntimeState,
    FxLifecycle, LatencyCertaintyState, LatencyComponentKind, LatencyComponentPolicyState,
    LatencyObservationState, LatencyProviderState, LatencyValueMode, OxiSynthMidiCcAssignment,
    OxiSynthParameter, OxiSynthState, ResolvedAudioDriverConfig, TakeLatencyProvenanceState,
    TrackFxState, TrackLatencyPolicyState, TrackProcessorDescriptor, TrackProcessorEditorState,
};
use shoop_audio_protocol::{
    Command, Event, MidiDataChunk, WaveformChunk, WireApplicationPortOwner, WireChannelMode,
    WireCompositeConfig, WireCompositeEntry, WireCompositeKind, WireCompositeTarget,
    WireGrabRequest, WireHostPort, WireLatencyCertainty, WireLatencyComponentKind,
    WireLatencyComponentPolicy, WireLatencyObservation, WireLatencyValueMode, WireLoopMode,
    WireMidiEvent, WireOxiSynthMidiCcAssignment, WireOxiSynthParameter, WirePortDataType,
    WirePortDirection, WirePortRole, WireSnapshot, WireTakeLatencyState, WireTrackControl,
    WireTrackFxControl, WireTrackLatencyPolicy, WireTrackTopology, COMMAND_CAPACITY,
    MIDI_BATCH_CAPACITY, MIDI_DETAIL_CHUNK_EVENTS, SESSION_TRANSFER_CHUNK_BYTES,
    SESSION_TRANSFER_MAX_BYTES, STATUS_INTERVAL_MS, WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    encode_oxisynth_state, oxisynth_descriptor, Backend, BackendActiveCompositeChild,
    BackendAsyncResult, BackendAudioChannelData, BackendAudioData, BackendChannelMode,
    BackendCompositeConfig, BackendCompositeId, BackendCompositeKind, BackendCompositeState,
    BackendCompositeTarget, BackendConfirmedLink, BackendConnectionFailure, BackendDriverState,
    BackendGrabRequest, BackendHostPortDescriptor, BackendLatencyCapability,
    BackendLoopContentUpdate, BackendLoopId, BackendLoopMode, BackendLoopState,
    BackendMidiChannelData, BackendMidiData, BackendMidiEvent, BackendMutationDetail,
    BackendMutationFailure, BackendMutationKind, BackendOperationKind, BackendOperationProgress,
    BackendPortDataType, BackendPortDescriptor, BackendPortDirection, BackendPortId,
    BackendPortOwner, BackendPortRole, BackendSessionData, BackendSessionReplacement,
    BackendSnapshot, BackendStatus, BackendTrackControl, BackendTrackCreation,
    BackendTrackFxControl, BackendTrackId, BackendTrackState, BackendTrackTopology,
    DirectTrackRequest, OxiSynthControl, TrackProcessorTypeId, TrackRequest,
};

use crate::transport::{transport_pair, TransportCore};

struct WaveformAssembly {
    revision: u64,
    channels: Vec<Vec<f32>>,
    timing: Vec<(i32, u32)>,
    next_channel: usize,
    next_offset: usize,
    complete: bool,
    in_flight: bool,
}

struct MidiDataAssembly {
    generation: u64,
    channels: Vec<BackendMidiChannelData>,
    next_channel: usize,
    next_offset: usize,
    complete: bool,
    in_flight: bool,
}

const SESSION_CAPTURE_IN_FLIGHT_LIMIT: usize = 8;

struct SessionCaptureAssembly {
    generation: u64,
    total_bytes: Option<usize>,
    bytes: Vec<u8>,
    next_offset: usize,
    in_flight: usize,
}

struct SessionReplaceAssembly {
    generation: u64,
    session: BackendSessionData,
    bytes: Vec<u8>,
    next_offset: usize,
    commit_sent: bool,
    complete: bool,
}

struct LoopContentReplaceAssembly {
    generation: u64,
    loop_id: BackendLoopId,
    update: BackendLoopContentUpdate,
    bytes: Vec<u8>,
    next_offset: usize,
    commit_sent: bool,
    complete: bool,
}

#[derive(Default)]
struct BrowserTrackResources {
    topology: BackendTrackTopology,
    loops: Vec<BackendLoopId>,
}

pub struct RemoteWorkletBackend {
    transport: Rc<RefCell<TransportCore>>,
    snapshot: BackendSnapshot,
    track_resources: BTreeMap<BackendTrackId, BrowserTrackResources>,
    pending_removed_tracks: BTreeMap<BackendTrackId, BrowserTrackResources>,
    reserved_composites: BTreeSet<BackendCompositeId>,
    next_track_id: u64,
    next_loop_id: u64,
    next_composite_id: u64,
    next_port_id: u64,
    transport_generation: u64,
    poll_elapsed: Duration,
    last_wire_xruns: u32,
    waveform_revisions: BTreeMap<BackendLoopId, u64>,
    waveforms: BTreeMap<BackendLoopId, WaveformAssembly>,
    midi_data_generations: BTreeMap<BackendLoopId, u64>,
    midi_data: BTreeMap<BackendLoopId, MidiDataAssembly>,
    next_session_generation: u64,
    session_capture: Option<SessionCaptureAssembly>,
    session_replace: Option<SessionReplaceAssembly>,
    loop_content_replace: Option<LoopContentReplaceAssembly>,
    session_capture_error: Option<String>,
    session_replace_error: Option<String>,
    loop_content_replace_error: Option<String>,
    midi: Box<dyn HostMidiBridge>,
    midi_revision: u64,
}

impl RemoteWorkletBackend {
    pub fn new(midi: impl HostMidiBridge + 'static) -> (Self, RemoteBackendControl) {
        let (transport, control) = transport_pair();
        control.set_driver_state(BackendDriverState::AwaitingGesture);
        (
            Self {
                transport: transport.clone(),
                snapshot: BackendSnapshot {
                    status: BackendStatus {
                        driver_state: BackendDriverState::AwaitingGesture,
                        ..Default::default()
                    },
                    audio_drivers: AudioDriverRuntimeState {
                        supported: false,
                        catalog: Arc::from([AudioDriverDescriptor {
                            kind: AudioDriverKind::WebAudio,
                            available: true,
                            ..Default::default()
                        }]),
                        active: Some(ResolvedAudioDriverConfig {
                            configured: AudioDriverConfig::WebAudio,
                            sample_rate: 0,
                            buffer_size: 0,
                            instance_name: "Web Audio".to_owned(),
                        }),
                        ..Default::default()
                    },
                    ..Default::default()
                },
                track_resources: BTreeMap::new(),
                pending_removed_tracks: BTreeMap::new(),
                reserved_composites: BTreeSet::new(),
                next_track_id: 1,
                next_loop_id: 1,
                next_composite_id: 1,
                next_port_id: 1,
                transport_generation: 0,
                poll_elapsed: Duration::ZERO,
                last_wire_xruns: 0,
                waveform_revisions: BTreeMap::new(),
                waveforms: BTreeMap::new(),
                midi_data_generations: BTreeMap::new(),
                midi_data: BTreeMap::new(),
                next_session_generation: 1,
                session_capture: None,
                session_replace: None,
                loop_content_replace: None,
                session_capture_error: None,
                session_replace_error: None,
                loop_content_replace_error: None,
                midi: Box::new(midi),
                midi_revision: u64::MAX,
            },
            control,
        )
    }

    pub fn is_quiescent(&self) -> bool {
        self.transport.borrow().is_quiescent()
            && self
                .waveforms
                .values()
                .all(|assembly| assembly.complete && !assembly.in_flight)
            && self
                .midi_data
                .values()
                .all(|assembly| assembly.complete && !assembly.in_flight)
            && self.session_capture.is_none()
            && self.session_replace.is_none()
            && self.loop_content_replace.is_none()
    }

    fn has_loop(&self, loop_id: BackendLoopId) -> bool {
        self.track_resources
            .values()
            .any(|resources| resources.loops.contains(&loop_id))
    }

    fn cancel_transfers(&mut self, reason: &str) {
        if let Some(capture) = self.session_capture.take() {
            self.session_capture_error = Some(format!(
                "session capture operation {} was cancelled: {reason}",
                capture.generation
            ));
        }
        if let Some(replace) = self.session_replace.take() {
            self.session_replace_error = Some(format!(
                "session replacement operation {} was cancelled: {reason}",
                replace.generation
            ));
        }
        if let Some(replace) = self.loop_content_replace.take() {
            self.loop_content_replace_error = Some(format!(
                "loop content replacement operation {} was cancelled: {reason}",
                replace.generation
            ));
        }
    }

    fn submit(&mut self, command: Command) -> Result<()> {
        self.transport.borrow_mut().journal(command)
    }

    pub fn configure_audio_context_latency(
        &mut self,
        base_latency_seconds: Option<f64>,
        output_latency_seconds: Option<f64>,
        sample_rate: u32,
        revision: u64,
    ) -> Result<()> {
        let frames = |seconds: Option<f64>| -> Result<Option<u32>> {
            seconds
                .map(|seconds| {
                    if !seconds.is_finite() || seconds < 0.0 || sample_rate == 0 {
                        return Err(anyhow!("invalid browser latency observation"));
                    }
                    let frames = (seconds * f64::from(sample_rate)).round();
                    if frames > f64::from(u32::MAX) {
                        return Err(anyhow!("browser latency observation exceeds frame range"));
                    }
                    Ok(frames as u32)
                })
                .transpose()
        };
        self.submit(Command::ConfigureBackendLatency {
            base_latency_frames: frames(base_latency_seconds)?,
            output_latency_frames: frames(output_latency_seconds)?,
            sample_rate,
            revision,
        })
    }

    fn submit_ephemeral(&mut self, command: Command) -> Result<()> {
        self.transport.borrow_mut().ephemeral(command)
    }

    fn sync_midi_endpoints(&mut self) -> Result<()> {
        let revision = self.midi.revision();
        if revision == self.midi_revision {
            return Ok(());
        }
        let endpoints = self.midi.endpoints();
        let wire_endpoints = endpoints
            .iter()
            .map(|endpoint| WireHostPort {
                id: endpoint.id.clone(),
                name: endpoint.name.clone(),
                data_type: WirePortDataType::Midi,
                direction: match endpoint.direction {
                    HostMidiDirection::Input => WirePortDirection::Input,
                    HostMidiDirection::Output => WirePortDirection::Output,
                },
            })
            .collect::<Vec<_>>();
        self.submit(Command::ConfigureMidiEndpoints {
            endpoints: wire_endpoints,
        })?;
        self.snapshot
            .connections
            .host_ports
            .retain(|_, host| host.data_type != BackendPortDataType::Midi);
        for endpoint in endpoints {
            self.snapshot.connections.host_ports.insert(
                endpoint.id.clone(),
                BackendHostPortDescriptor {
                    id: endpoint.id,
                    name: endpoint.name,
                    data_type: BackendPortDataType::Midi,
                    direction: match endpoint.direction {
                        HostMidiDirection::Input => BackendPortDirection::Input,
                        HostMidiDirection::Output => BackendPortDirection::Output,
                    },
                },
            );
        }
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.midi_revision = revision;
        Ok(())
    }

    fn pump_midi_input(&mut self, running: bool) -> Result<()> {
        let messages = self.midi.drain_track_messages(MIDI_BATCH_CAPACITY);
        if !running {
            return Ok(());
        }
        let mut batches: BTreeMap<String, Vec<WireMidiEvent>> = BTreeMap::new();
        for HostMidiInput { endpoint_id, data } in messages {
            batches
                .entry(endpoint_id)
                .or_default()
                .push(WireMidiEvent { frame: 0, data });
        }
        for (host_port_id, events) in batches {
            self.transport
                .borrow_mut()
                .ephemeral(Command::PushMidiInput {
                    host_port_id,
                    events,
                })?;
        }
        Ok(())
    }

    fn request_waveform_chunk(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let Some(assembly) = self.waveforms.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.complete || assembly.in_flight {
            return Ok(());
        }
        self.transport
            .borrow_mut()
            .ephemeral(Command::RequestWaveform {
                loop_id: loop_id.raw(),
                revision: assembly.revision,
                channel: assembly.next_channel,
                offset: assembly.next_offset,
                max_samples: WAVEFORM_CHUNK_SAMPLES,
            })?;
        assembly.in_flight = true;
        Ok(())
    }

    fn apply_waveform_chunk(&mut self, chunk: WaveformChunk) -> Result<()> {
        let loop_id = BackendLoopId::from_raw(chunk.loop_id);
        let Some(assembly) = self.waveforms.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.revision != chunk.revision
            || assembly.next_channel != chunk.channel
            || assembly.next_offset != chunk.offset
        {
            return Ok(());
        }
        assembly.in_flight = false;
        if assembly.channels.len() < chunk.channel_count {
            assembly.channels.resize_with(chunk.channel_count, Vec::new);
            assembly.timing.resize(chunk.channel_count, (0, 0));
        }
        if let Some(channel) = assembly.channels.get_mut(chunk.channel) {
            channel.extend_from_slice(&chunk.samples);
        }
        if let Some(timing) = assembly.timing.get_mut(chunk.channel) {
            *timing = (chunk.start_offset, chunk.preplay);
        }
        if chunk.final_chunk {
            assembly.next_channel += 1;
            assembly.next_offset = 0;
            assembly.complete = assembly.next_channel >= chunk.channel_count;
        } else {
            assembly.next_offset = chunk.offset.saturating_add(chunk.samples.len());
        }
        self.request_waveform_chunk(loop_id)
    }

    fn request_midi_data_chunk(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let Some(assembly) = self.midi_data.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.complete || assembly.in_flight {
            return Ok(());
        }
        self.transport
            .borrow_mut()
            .ephemeral(Command::RequestMidiData {
                loop_id: loop_id.raw(),
                generation: assembly.generation,
                channel: assembly.next_channel,
                offset: assembly.next_offset,
                max_events: MIDI_DETAIL_CHUNK_EVENTS,
            })?;
        assembly.in_flight = true;
        Ok(())
    }

    fn restart_midi_data(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let generation = self
            .midi_data_generations
            .entry(loop_id)
            .and_modify(|generation| *generation = generation.saturating_add(1))
            .or_insert(1);
        self.midi_data.insert(
            loop_id,
            MidiDataAssembly {
                generation: *generation,
                channels: Vec::new(),
                next_channel: 0,
                next_offset: 0,
                complete: false,
                in_flight: false,
            },
        );
        self.request_midi_data_chunk(loop_id)
    }

    fn apply_midi_data_chunk(&mut self, chunk: MidiDataChunk) -> Result<()> {
        let loop_id = BackendLoopId::from_raw(chunk.loop_id);
        let Some(assembly) = self.midi_data.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.generation != chunk.generation
            || assembly.next_channel != chunk.channel
            || assembly.next_offset != chunk.offset
        {
            return Ok(());
        }
        assembly.in_flight = false;
        if chunk.channel_count == 0 {
            if chunk.channel != 0 || chunk.offset != 0 || !chunk.events.is_empty() {
                return Err(anyhow!("malformed empty MIDI detail chunk"));
            }
            assembly.complete = true;
            return Ok(());
        }
        if chunk.channel >= chunk.channel_count
            || chunk.offset > chunk.total_events
            || chunk.offset.saturating_add(chunk.events.len()) > chunk.total_events
            || chunk.events.len() > MIDI_DETAIL_CHUNK_EVENTS
            || chunk.final_chunk
                != (chunk.offset.saturating_add(chunk.events.len()) >= chunk.total_events)
        {
            return Err(anyhow!("malformed MIDI detail chunk"));
        }
        if chunk.offset == 0 {
            if assembly.channels.len() != chunk.channel {
                return Err(anyhow!("out-of-order MIDI detail channel"));
            }
            assembly.channels.push(BackendMidiChannelData {
                content_revision: chunk.content_revision,
                mode: match chunk.mode {
                    WireChannelMode::Direct => BackendChannelMode::Direct,
                    WireChannelMode::Dry => BackendChannelMode::Dry,
                    WireChannelMode::Wet => BackendChannelMode::Wet,
                },
                length: chunk.length,
                events: Vec::with_capacity(chunk.total_events),
                start_offset: chunk.start_offset,
                preplay: chunk.preplay,
                latency: Default::default(),
            });
        }
        let Some(channel) = assembly.channels.get_mut(chunk.channel) else {
            return Err(anyhow!("missing MIDI detail channel assembly"));
        };
        if channel.content_revision != chunk.content_revision {
            return self.restart_midi_data(loop_id);
        }
        if channel.length != chunk.length
            || channel.start_offset != chunk.start_offset
            || channel.preplay != chunk.preplay
            || channel.events.len() != chunk.offset
        {
            return Err(anyhow!("inconsistent MIDI detail chunk metadata"));
        }
        channel
            .events
            .extend(chunk.events.into_iter().map(|event| BackendMidiEvent {
                time: event.frame,
                data: event.data,
            }));
        if chunk.final_chunk {
            assembly.next_channel += 1;
            assembly.next_offset = 0;
            assembly.complete = assembly.next_channel >= chunk.channel_count;
        } else {
            assembly.next_offset = channel.events.len();
        }
        self.request_midi_data_chunk(loop_id)
    }

    fn request_session_capture_chunks(&mut self) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        let Some(total_bytes) = capture.total_bytes else {
            return Ok(());
        };
        while capture.next_offset < total_bytes
            && capture.in_flight < SESSION_CAPTURE_IN_FLIGHT_LIMIT
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY / 2
        {
            let offset = capture.next_offset;
            capture.next_offset = offset
                .saturating_add(SESSION_TRANSFER_CHUNK_BYTES)
                .min(total_bytes);
            self.transport
                .borrow_mut()
                .ephemeral(Command::ReadSessionCapture {
                    generation: capture.generation,
                    offset,
                    max_bytes: SESSION_TRANSFER_CHUNK_BYTES,
                })?;
            capture.in_flight += 1;
        }
        Ok(())
    }

    fn apply_session_capture_ready(&mut self, generation: u64, total_bytes: usize) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        if capture.generation != generation || total_bytes > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("invalid session capture metadata"));
        }
        capture.total_bytes = Some(total_bytes);
        capture.bytes.reserve(total_bytes);
        capture.next_offset = 0;
        capture.in_flight = 0;
        self.request_session_capture_chunks()
    }

    fn apply_session_capture_chunk(
        &mut self,
        generation: u64,
        offset: usize,
        total_bytes: usize,
        final_chunk: bool,
        bytes: Vec<u8>,
    ) -> Result<()> {
        let Some(capture) = self.session_capture.as_mut() else {
            return Ok(());
        };
        if capture.generation != generation
            || capture.total_bytes != Some(total_bytes)
            || capture.bytes.len() != offset
            || capture.in_flight == 0
            || bytes.len() > SESSION_TRANSFER_CHUNK_BYTES
            || offset.saturating_add(bytes.len()) > total_bytes
            || final_chunk != (offset.saturating_add(bytes.len()) >= total_bytes)
        {
            return Err(anyhow!("invalid session capture chunk"));
        }
        capture.bytes.extend_from_slice(&bytes);
        capture.in_flight -= 1;
        self.request_session_capture_chunks()
    }

    fn pump_session_replace(&mut self) -> Result<()> {
        let Some(replace) = self.session_replace.as_mut() else {
            return Ok(());
        };
        if replace.complete {
            return Ok(());
        }
        while replace.next_offset < replace.bytes.len()
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY / 2
        {
            let end = replace
                .next_offset
                .saturating_add(SESSION_TRANSFER_CHUNK_BYTES)
                .min(replace.bytes.len());
            self.transport
                .borrow_mut()
                .ephemeral(Command::WriteSessionReplace {
                    generation: replace.generation,
                    offset: replace.next_offset,
                    bytes: replace.bytes[replace.next_offset..end].to_vec(),
                })?;
            replace.next_offset = end;
        }
        if replace.next_offset == replace.bytes.len()
            && !replace.commit_sent
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY
        {
            self.transport
                .borrow_mut()
                .ephemeral(Command::CommitSessionReplace {
                    generation: replace.generation,
                })?;
            replace.commit_sent = true;
        }
        Ok(())
    }

    fn pump_loop_content_replace(&mut self) -> Result<()> {
        let Some(replace) = self.loop_content_replace.as_mut() else {
            return Ok(());
        };
        if replace.complete {
            return Ok(());
        }
        while replace.next_offset < replace.bytes.len()
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY / 2
        {
            let end = replace
                .next_offset
                .saturating_add(SESSION_TRANSFER_CHUNK_BYTES)
                .min(replace.bytes.len());
            self.transport
                .borrow_mut()
                .ephemeral(Command::WriteLoopContentReplace {
                    generation: replace.generation,
                    offset: replace.next_offset,
                    bytes: replace.bytes[replace.next_offset..end].to_vec(),
                })?;
            replace.next_offset = end;
        }
        if replace.next_offset == replace.bytes.len()
            && !replace.commit_sent
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY
        {
            self.transport
                .borrow_mut()
                .ephemeral(Command::CommitLoopContentReplace {
                    generation: replace.generation,
                })?;
            replace.commit_sent = true;
        }
        Ok(())
    }

    fn apply_replaced_session(
        &mut self,
        session: &BackendSessionData,
        replacement: &BackendSessionReplacement,
    ) {
        self.snapshot.tracks.clear();
        self.snapshot.loops.clear();
        self.snapshot.composites.clear();
        self.next_composite_id = 1;
        self.snapshot.connections.application_ports.clear();
        self.snapshot.connections.confirmed_links.clear();
        self.track_resources.clear();
        self.pending_removed_tracks.clear();
        self.reserved_composites.clear();
        self.waveforms.clear();
        self.midi_data.clear();
        for source_track in &session.tracks {
            let Some(created) = replacement.tracks.get(&source_track.source_id) else {
                continue;
            };
            self.snapshot
                .tracks
                .insert(created.track_id, source_track.state.clone());
            self.track_resources.insert(
                created.track_id,
                BrowserTrackResources {
                    topology: source_track.topology.clone(),
                    loops: created.loops.clone(),
                },
            );
            for (source_loop, loop_id) in source_track.loops.iter().zip(&created.loops) {
                self.snapshot.loops.insert(
                    *loop_id,
                    BackendLoopState {
                        mode: BackendLoopMode::Stopped,
                        length: source_loop.length,
                        stereo: source_track.state.audio_channels == 2,
                        gain: source_loop.gain,
                        balance: source_loop.balance,
                        audio_peaks: vec![-200.0; source_track.state.audio_channels as usize],
                        ..Default::default()
                    },
                );
            }
            for (source_port, created_port) in source_track.ports.iter().zip(&created.ports) {
                self.snapshot
                    .connections
                    .application_ports
                    .insert(created_port.id, created_port.clone());
                debug_assert_eq!(
                    replacement.ports.get(&source_port.source_id),
                    Some(&created_port.id)
                );
            }
        }
        self.next_track_id = replacement
            .tracks
            .values()
            .map(|created| created.track_id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_loop_id = replacement
            .loops
            .values()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.next_port_id = replacement
            .ports
            .values()
            .map(|id| id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
    }

    fn latency_observation(wire: WireLatencyObservation) -> LatencyObservationState {
        LatencyObservationState {
            minimum_frames: wire.minimum_frames,
            maximum_frames: wire.maximum_frames,
            certainty: match wire.certainty {
                WireLatencyCertainty::Exact => LatencyCertaintyState::Exact,
                WireLatencyCertainty::Range => LatencyCertaintyState::Range,
                WireLatencyCertainty::Estimated => LatencyCertaintyState::Estimated,
                WireLatencyCertainty::ManualOnly => LatencyCertaintyState::ManualOnly,
                WireLatencyCertainty::Unknown => LatencyCertaintyState::Unknown,
            },
            sample_rate: wire.sample_rate,
            revision: wire.revision,
        }
    }

    fn latency_policy(wire: WireTrackLatencyPolicy) -> TrackLatencyPolicyState {
        TrackLatencyPolicyState {
            cue_followed: wire.cue_followed,
            components: wire
                .components
                .into_iter()
                .map(|component| LatencyComponentPolicyState {
                    kind: match component.kind {
                        WireLatencyComponentKind::ExternalCapture => {
                            LatencyComponentKind::ExternalCapture
                        }
                        WireLatencyComponentKind::Processor => LatencyComponentKind::Processor,
                        WireLatencyComponentKind::CuePlayback => LatencyComponentKind::CuePlayback,
                        WireLatencyComponentKind::BackendBuffering => {
                            LatencyComponentKind::BackendBuffering
                        }
                        WireLatencyComponentKind::Manual => LatencyComponentKind::Manual,
                    },
                    enabled: component.enabled,
                    value_mode: match component.value_mode {
                        WireLatencyValueMode::Automatic => LatencyValueMode::Automatic,
                        WireLatencyValueMode::Manual(frames) => LatencyValueMode::Manual(frames),
                        WireLatencyValueMode::AutomaticPlusTrim(frames) => {
                            LatencyValueMode::AutomaticPlusTrim(frames)
                        }
                    },
                })
                .collect::<Vec<_>>()
                .into(),
            revision: wire.revision,
            pending: false,
            error: None,
        }
    }

    fn wire_latency_policy(policy: &TrackLatencyPolicyState) -> WireTrackLatencyPolicy {
        WireTrackLatencyPolicy {
            cue_followed: policy.cue_followed,
            components: policy
                .components
                .iter()
                .map(|component| WireLatencyComponentPolicy {
                    kind: match component.kind {
                        LatencyComponentKind::ExternalCapture => {
                            WireLatencyComponentKind::ExternalCapture
                        }
                        LatencyComponentKind::Processor => WireLatencyComponentKind::Processor,
                        LatencyComponentKind::CuePlayback => WireLatencyComponentKind::CuePlayback,
                        LatencyComponentKind::BackendBuffering => {
                            WireLatencyComponentKind::BackendBuffering
                        }
                        LatencyComponentKind::Manual => WireLatencyComponentKind::Manual,
                    },
                    enabled: component.enabled,
                    value_mode: match component.value_mode {
                        LatencyValueMode::Automatic => WireLatencyValueMode::Automatic,
                        LatencyValueMode::Manual(frames) => WireLatencyValueMode::Manual(frames),
                        LatencyValueMode::AutomaticPlusTrim(frames) => {
                            WireLatencyValueMode::AutomaticPlusTrim(frames)
                        }
                    },
                })
                .collect(),
            revision: policy.revision,
        }
    }

    fn take_latency(wire: WireTakeLatencyState) -> TakeLatencyProvenanceState {
        let observation = Self::latency_observation(wire.observation);
        TakeLatencyProvenanceState {
            capture_alignment_frames: wire.capture_alignment_frames,
            render_advance_frames: wire.render_advance_frames,
            certainty: observation.certainty,
            observation_min_frames: observation.minimum_frames,
            observation_max_frames: observation.maximum_frames,
            observation_sample_rate: observation.sample_rate,
            observation_revision: observation.revision,
            variable_history: wire.variable_history,
            history_revisions: wire.history_revisions,
            changed_during_operation: wire.changed_during_operation,
            incomplete: wire.incomplete,
            deferred_mode: None,
            finalizing: wire.finalizing,
            error: wire.error,
        }
    }

    fn apply_wire_snapshot(&mut self, wire: WireSnapshot) {
        let state = self.transport.borrow().driver_state();
        let xruns = if wire.xruns >= self.last_wire_xruns {
            wire.xruns.saturating_sub(self.last_wire_xruns)
        } else {
            wire.xruns
        };
        self.last_wire_xruns = wire.xruns;
        self.snapshot.status = BackendStatus {
            sample_rate: wire.sample_rate,
            buffer_size: wire.quantum,
            callback_count: wire.callback_count,
            processed_frames: wire.processed_frames,
            input_peak: wire.input_peak,
            output_peak: wire.output_peak,
            xruns,
            callback_budget_overruns: wire.callback_budget_overruns,
            render_discontinuities: wire.render_discontinuities,
            memory_growths: wire.memory_growths,
            render_memory_growths: wire.render_memory_growths,
            command_overflows: wire
                .command_overflows
                .saturating_add(self.transport.borrow().overflows()),
            storage_low_channels: wire.storage_low_channels,
            storage_exhaustions: wire.storage_exhaustions,
            backend_capture_latency: Self::latency_observation(wire.backend_capture_latency),
            backend_playback_latency: Self::latency_observation(wire.backend_playback_latency),
            driver_state: state,
            ..Default::default()
        };
        if let Some(active) = self.snapshot.audio_drivers.active.as_mut() {
            active.sample_rate = wire.sample_rate;
            active.buffer_size = wire.quantum;
        }
        self.snapshot.connections.available = true;
        self.snapshot.connections.application_ports = wire
            .application_ports
            .into_iter()
            .map(|port| {
                let id = BackendPortId::from_raw(port.id);
                (
                    id,
                    BackendPortDescriptor {
                        id,
                        owner: match port.owner {
                            WireApplicationPortOwner::Track => BackendPortOwner::Track,
                            WireApplicationPortOwner::GlobalFxControl => {
                                BackendPortOwner::GlobalFxControl
                            }
                        },
                        name: port.name,
                        data_type: from_wire_data_type(port.data_type),
                        direction: from_wire_direction(port.direction),
                        role: from_wire_role(port.role),
                    },
                )
            })
            .collect();
        self.snapshot.connections.host_ports = wire
            .host_ports
            .into_iter()
            .map(|port| {
                (
                    port.id.clone(),
                    BackendHostPortDescriptor {
                        id: port.id,
                        name: port.name,
                        data_type: from_wire_data_type(port.data_type),
                        direction: from_wire_direction(port.direction),
                    },
                )
            })
            .collect();
        self.snapshot.connections.confirmed_links = wire
            .confirmed_links
            .into_iter()
            .map(|link| BackendConfirmedLink {
                application_port_id: BackendPortId::from_raw(link.application_port_id),
                host_port_id: link.host_port_id,
            })
            .collect();
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        let observed_track_ids = wire
            .tracks
            .iter()
            .map(|track| BackendTrackId::from_raw(track.id))
            .collect::<BTreeSet<_>>();
        self.pending_removed_tracks
            .retain(|track_id, _| observed_track_ids.contains(track_id));
        self.snapshot.tracks = wire
            .tracks
            .into_iter()
            .map(|track| {
                (
                    BackendTrackId::from_raw(track.id),
                    BackendTrackState {
                        topology: match track.topology {
                            WireTrackTopology::Direct {
                                audio_channels,
                                midi,
                            } => BackendTrackTopology::Direct {
                                audio_channels,
                                midi,
                            },
                            WireTrackTopology::OxiSynth => BackendTrackTopology::DryWetProcessor {
                                processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                                dry_audio_channels: 2,
                                wet_audio_channels: 2,
                                dry_midi: true,
                            },
                        },
                        latency_policy: Self::latency_policy(track.latency_policy),
                        fx: track.fx.map(|fx| {
                            let oxisynth = fx.oxisynth.map(|oxisynth| {
                                TrackProcessorEditorState::OxiSynth(OxiSynthState {
                                    selected_preset_id: oxisynth.selected_preset_id,
                                    reverb_send: oxisynth.reverb_send,
                                    chorus_send: oxisynth.chorus_send,
                                    midi_cc_assignments: oxisynth
                                        .midi_cc_assignments
                                        .into_iter()
                                        .map(|assignment| OxiSynthMidiCcAssignment {
                                            parameter: from_wire_oxisynth_parameter(
                                                assignment.parameter,
                                            ),
                                            channel: assignment.channel,
                                            controller: assignment.controller,
                                        })
                                        .collect::<Vec<_>>()
                                        .into(),
                                })
                            });
                            TrackFxState {
                                processor_type: TrackProcessorTypeId::new(fx.processor_type),
                                active: fx.active,
                                visible: fx.visible,
                                lifecycle: FxLifecycle::Running,
                                generation: 0,
                                crash_summary: None,
                                logs: Arc::from([]),
                                latency: Self::latency_observation(fx.latency),
                                latency_provider: LatencyProviderState::BuiltInSynthPhaseRange,
                                editor: oxisynth,
                            }
                        }),
                        audio_channels: track.audio_channels,
                        midi: track.midi,
                        output_gain_db: track.output_gain_db,
                        output_balance: track.output_balance,
                        output_muted: track.output_muted,
                        input_gain_db: track.input_gain_db,
                        input_balance: track.input_balance,
                        input_monitoring: track.input_monitoring,
                        input_peaks: track.input_peaks,
                        output_peaks: track.output_peaks,
                        latest_input_midi_message: track.latest_input_midi_message.map(|message| {
                            shoop_backend::BackendLatestMidiMessage {
                                bytes: message.bytes,
                                len: message.len,
                            }
                        }),
                        ..Default::default()
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        self.snapshot.loops = wire
            .loops
            .into_iter()
            .map(|loop_| {
                (
                    BackendLoopId::from_raw(loop_.id),
                    BackendLoopState {
                        mode: from_wire_loop_mode(loop_.mode),
                        latency: Self::take_latency(loop_.latency),
                        length: loop_.length,
                        position: loop_.position,
                        next_mode: loop_.next_mode.map(from_wire_loop_mode),
                        next_transition_delay: loop_.next_transition_delay,
                        stereo: loop_.stereo,
                        gain: loop_.gain,
                        balance: loop_.balance,
                        audio_peaks: loop_.audio_peaks,
                        midi_activity: loop_.midi_activity,
                    },
                )
            })
            .collect::<BTreeMap<_, _>>();
        self.snapshot.composites = wire
            .composites
            .into_iter()
            .map(|composite| {
                let target = |target| match target {
                    WireCompositeTarget::Loop(id) => {
                        BackendCompositeTarget::Loop(BackendLoopId::from_raw(id))
                    }
                    WireCompositeTarget::Composite(id) => {
                        BackendCompositeTarget::Composite(BackendCompositeId::from_raw(id))
                    }
                };
                (
                    BackendCompositeId::from_raw(composite.id),
                    BackendCompositeState {
                        mode: from_wire_loop_mode(composite.mode),
                        next_mode: composite.next_mode.map(from_wire_loop_mode),
                        next_transition_delay: composite.next_transition_delay,
                        iteration: composite.iteration,
                        cycle_count: composite.cycle_count,
                        length: composite.length,
                        position: composite.position,
                        active_plan_version: composite.active_plan_version,
                        pending_plan_version: composite.pending_plan_version,
                        active_children: composite
                            .active_children
                            .into_iter()
                            .map(|child| BackendActiveCompositeChild {
                                target: target(child.target),
                                mode: from_wire_loop_mode(child.mode),
                                cycle_offset: child.cycle_offset,
                            })
                            .collect(),
                    },
                )
            })
            .collect();
    }
}

fn browser_replacement_mapping(session: &BackendSessionData) -> BackendSessionReplacement {
    let mut replacement = BackendSessionReplacement::default();
    for global in &session.global_ports {
        replacement
            .global_ports
            .insert(global.source_id, global.descriptor.id);
    }
    let mut next_track_id = 1_u64;
    let mut next_loop_id = 1_u64;
    let mut next_port_id = 1_u64;
    for source_track in &session.tracks {
        let track_id = BackendTrackId::from_raw(next_track_id);
        next_track_id = next_track_id.saturating_add(1);
        let loops = source_track
            .loops
            .iter()
            .map(|source_loop| {
                let id = BackendLoopId::from_raw(next_loop_id);
                next_loop_id = next_loop_id.saturating_add(1);
                replacement.loops.insert(source_loop.source_id, id);
                id
            })
            .collect::<Vec<_>>();
        let ports = source_track
            .ports
            .iter()
            .map(|source_port| {
                let mut descriptor = source_port.descriptor.clone();
                descriptor.id = BackendPortId::from_raw(next_port_id);
                next_port_id = next_port_id.saturating_add(1);
                replacement
                    .ports
                    .insert(source_port.source_id, descriptor.id);
                descriptor
            })
            .collect::<Vec<_>>();
        replacement.tracks.insert(
            source_track.source_id,
            BackendTrackCreation {
                track_id,
                loops,
                ports,
            },
        );
    }
    replacement
}

fn browser_port_descriptors(
    base: &str,
    audio_channels: u32,
    midi: bool,
    next_port_id: &mut u64,
) -> Vec<BackendPortDescriptor> {
    let mut ports = Vec::with_capacity(audio_channels as usize * 2 + 2);
    let mut add = |name: String, data_type, direction, role| {
        let id = BackendPortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.push(BackendPortDescriptor {
            id,
            owner: BackendPortOwner::Track,
            name,
            data_type,
            direction,
            role,
        });
    };
    for index in 0..audio_channels {
        let suffix = if audio_channels == 1 {
            String::new()
        } else {
            format!("_{}", index + 1)
        };
        add(
            format!("{base}_direct_in{suffix}"),
            BackendPortDataType::Audio,
            BackendPortDirection::Input,
            BackendPortRole::AudioInput,
        );
        add(
            format!("{base}_direct_out{suffix}"),
            BackendPortDataType::Audio,
            BackendPortDirection::Output,
            BackendPortRole::AudioOutput,
        );
    }
    if midi {
        add(
            format!("{base}_direct_midi_in"),
            BackendPortDataType::Midi,
            BackendPortDirection::Input,
            BackendPortRole::MidiInput,
        );
        add(
            format!("{base}_direct_midi_out"),
            BackendPortDataType::Midi,
            BackendPortDirection::Output,
            BackendPortRole::MidiOutput,
        );
    }
    ports
}

fn browser_oxisynth_port_descriptors(
    base: &str,
    next_port_id: &mut u64,
) -> Vec<BackendPortDescriptor> {
    let mut ports = Vec::with_capacity(5);
    for index in 0..2 {
        let id = BackendPortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.push(BackendPortDescriptor {
            id,
            owner: BackendPortOwner::Track,
            name: format!("{base}_audio_dry_in_{}", index + 1),
            data_type: BackendPortDataType::Audio,
            direction: BackendPortDirection::Input,
            role: BackendPortRole::AudioInput,
        });
    }
    for index in 0..2 {
        let id = BackendPortId::from_raw(*next_port_id);
        *next_port_id = next_port_id.saturating_add(1);
        ports.push(BackendPortDescriptor {
            id,
            owner: BackendPortOwner::Track,
            name: format!("{base}_audio_wet_out_{}", index + 1),
            data_type: BackendPortDataType::Audio,
            direction: BackendPortDirection::Output,
            role: BackendPortRole::AudioOutput,
        });
    }
    let id = BackendPortId::from_raw(*next_port_id);
    *next_port_id = next_port_id.saturating_add(1);
    ports.push(BackendPortDescriptor {
        id,
        owner: BackendPortOwner::Track,
        name: format!("{base}_dry_midi_in"),
        data_type: BackendPortDataType::Midi,
        direction: BackendPortDirection::Input,
        role: BackendPortRole::MidiInput,
    });
    ports
}

fn command_mutation_identity(command: &Command) -> Option<(BackendMutationKind, Option<u64>)> {
    Some(match command {
        Command::ConfigureDeviceChannels { .. }
        | Command::ConfigureBackendLatency { .. }
        | Command::ConfigureMidiEndpoints { .. } => {
            (BackendMutationKind::DriverConfiguration, None)
        }
        Command::SetTrackLatencyPolicy { track_id, .. } => {
            (BackendMutationKind::TrackControl, Some(*track_id))
        }
        Command::SetTakeLatencyPolicy { loop_id, .. } => {
            (BackendMutationKind::LoopControl, Some(*loop_id))
        }
        Command::ConsolidateTakeLatency { loop_id } => {
            (BackendMutationKind::LoopContent, Some(*loop_id))
        }
        Command::CreateTrack {
            expected_track_id, ..
        }
        | Command::RemoveTrack {
            track_id: expected_track_id,
        }
        | Command::AddLoop {
            track_id: expected_track_id,
            ..
        } => (
            BackendMutationKind::TrackStructure,
            Some(*expected_track_id),
        ),
        Command::CreateComposite {
            expected_composite_id,
        }
        | Command::ConfigureComposite {
            composite_id: expected_composite_id,
            ..
        }
        | Command::TransitionComposite {
            composite_id: expected_composite_id,
            ..
        }
        | Command::SetCompositePlayAfterRecord {
            composite_id: expected_composite_id,
            ..
        }
        | Command::RemoveComposite {
            composite_id: expected_composite_id,
        } => (
            BackendMutationKind::CompositeStructure,
            Some(*expected_composite_id),
        ),
        Command::SetTrackControl { track_id, .. } => {
            (BackendMutationKind::TrackControl, Some(*track_id))
        }
        Command::SetTrackFxControl { track_id, .. } => {
            (BackendMutationKind::TrackFxControl, Some(*track_id))
        }
        Command::PushMidiInput { .. } => (BackendMutationKind::MidiInput, None),
        Command::InjectTrackMidiInput { track_id, .. } => {
            (BackendMutationKind::MidiInput, Some(*track_id))
        }
        Command::SetLoopGain { loop_id, .. }
        | Command::SetLoopBalance { loop_id, .. }
        | Command::SetLoopSyncSource { loop_id, .. }
        | Command::TransitionLoop { loop_id, .. }
        | Command::ClearLoop { loop_id }
        | Command::SetLoopLength { loop_id, .. }
        | Command::SetLoopTiming { loop_id, .. } => {
            (BackendMutationKind::LoopControl, Some(*loop_id))
        }
        Command::GrabLoops { requests } => (
            BackendMutationKind::LoopControl,
            requests.first().map(|request| request.loop_id),
        ),
        Command::BeginLoopContentReplace { loop_id, .. }
        | Command::CommitLoopContentReplace {
            generation: loop_id,
        } => (BackendMutationKind::LoopContent, Some(*loop_id)),
        Command::WriteLoopContentReplace { generation, .. } => {
            (BackendMutationKind::LoopContent, Some(*generation))
        }
        Command::SetPortConnected {
            application_port_id,
            ..
        } => (BackendMutationKind::Connection, Some(*application_port_id)),
        Command::BeginSessionCapture { .. }
        | Command::ReadSessionCapture { .. }
        | Command::BeginSessionReplace { .. }
        | Command::WriteSessionReplace { .. }
        | Command::CommitSessionReplace { .. }
        | Command::AbortSessionTransfer { .. } => (BackendMutationKind::SessionTransfer, None),
        Command::DrainMidiOutput { .. }
        | Command::RequestWaveform { .. }
        | Command::RequestMidiData { .. }
        | Command::Poll
        | Command::Shutdown => return None,
    })
}

fn from_wire_track_fx_control(control: &WireTrackFxControl) -> BackendTrackFxControl {
    match control {
        WireTrackFxControl::SetActive(value) => BackendTrackFxControl::SetActive(*value),
        WireTrackFxControl::SetVisible(value) => BackendTrackFxControl::SetVisible(*value),
        WireTrackFxControl::ToggleOrRecover => BackendTrackFxControl::ToggleOrRecover,
        WireTrackFxControl::RestoreState(value) => {
            BackendTrackFxControl::RestoreState(value.clone())
        }
        WireTrackFxControl::ClearLogs => BackendTrackFxControl::ClearLogs,
        WireTrackFxControl::OxiSelectPreset(value) => {
            BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset(value.clone()))
        }
        WireTrackFxControl::OxiSetReverbSend(value) => {
            BackendTrackFxControl::OxiSynth(OxiSynthControl::SetReverbSend(*value))
        }
        WireTrackFxControl::OxiSetChorusSend(value) => {
            BackendTrackFxControl::OxiSynth(OxiSynthControl::SetChorusSend(*value))
        }
        WireTrackFxControl::OxiAssignMidiCc(assignment) => BackendTrackFxControl::OxiSynth(
            OxiSynthControl::AssignMidiCc(OxiSynthMidiCcAssignment {
                parameter: from_wire_oxisynth_parameter(assignment.parameter),
                channel: assignment.channel,
                controller: assignment.controller,
            }),
        ),
        WireTrackFxControl::OxiRemoveMidiCc(parameter) => BackendTrackFxControl::OxiSynth(
            OxiSynthControl::RemoveMidiCc(from_wire_oxisynth_parameter(*parameter)),
        ),
        WireTrackFxControl::OxiClearMidiCcAssignments => {
            BackendTrackFxControl::OxiSynth(OxiSynthControl::ClearMidiCcAssignments)
        }
        WireTrackFxControl::OxiPanic => BackendTrackFxControl::OxiSynth(OxiSynthControl::Panic),
    }
}

fn mutation_detail(command: &Command) -> Option<BackendMutationDetail> {
    match command {
        Command::CreateTrack { .. } => Some(BackendMutationDetail::TrackCreation),
        Command::RemoveTrack { .. } => Some(BackendMutationDetail::TrackRemoval),
        Command::AddLoop {
            expected_loop_id, ..
        } => Some(BackendMutationDetail::LoopCreation {
            loop_id: BackendLoopId::from_raw(*expected_loop_id),
        }),
        Command::SetTrackControl { control, .. } => {
            Some(BackendMutationDetail::TrackControl(match control {
                WireTrackControl::OutputGainDb(value) => BackendTrackControl::OutputGainDb(*value),
                WireTrackControl::OutputBalance(value) => {
                    BackendTrackControl::OutputBalance(*value)
                }
                WireTrackControl::OutputMute(value) => BackendTrackControl::OutputMute(*value),
                WireTrackControl::InputGainDb(value) => BackendTrackControl::InputGainDb(*value),
                WireTrackControl::InputBalance(value) => BackendTrackControl::InputBalance(*value),
                WireTrackControl::InputMonitoring(value) => {
                    BackendTrackControl::InputMonitoring(*value)
                }
            }))
        }
        Command::SetTrackFxControl { control, .. } => Some(BackendMutationDetail::TrackFxControl(
            from_wire_track_fx_control(control),
        )),
        Command::SetLoopGain { gain, .. } => Some(BackendMutationDetail::LoopGain(*gain)),
        Command::SetLoopBalance { balance, .. } => {
            Some(BackendMutationDetail::LoopBalance(*balance))
        }
        _ => None,
    }
}

fn transfer_identity(command: &Command) -> Option<(BackendOperationKind, u64)> {
    match command {
        Command::BeginSessionCapture { generation }
        | Command::ReadSessionCapture { generation, .. } => {
            Some((BackendOperationKind::SessionCapture, *generation))
        }
        Command::BeginSessionReplace { generation, .. }
        | Command::WriteSessionReplace { generation, .. }
        | Command::CommitSessionReplace { generation } => {
            Some((BackendOperationKind::SessionReplacement, *generation))
        }
        Command::BeginLoopContentReplace { generation, .. }
        | Command::WriteLoopContentReplace { generation, .. }
        | Command::CommitLoopContentReplace { generation } => {
            Some((BackendOperationKind::LoopContentReplacement, *generation))
        }
        Command::AbortSessionTransfer { generation } => {
            Some((BackendOperationKind::SessionReplacement, *generation))
        }
        _ => None,
    }
}

impl Backend for RemoteWorkletBackend {
    fn latency_capability(&self) -> BackendLatencyCapability {
        BackendLatencyCapability::Observed
    }

    fn set_track_latency_policy(
        &mut self,
        track_id: BackendTrackId,
        policy: &TrackLatencyPolicyState,
    ) -> Result<()> {
        if policy.components.len() > shoop_audio_protocol::LATENCY_COMPONENT_CAPACITY {
            return Err(anyhow!("latency component capacity exceeded"));
        }
        self.submit(Command::SetTrackLatencyPolicy {
            track_id: track_id.raw(),
            policy: Self::wire_latency_policy(policy),
        })
    }

    fn set_take_latency_policy(
        &mut self,
        loop_id: BackendLoopId,
        capture_alignment_frames: i32,
    ) -> Result<()> {
        self.submit(Command::SetTakeLatencyPolicy {
            loop_id: loop_id.raw(),
            capture_alignment_frames,
        })
    }

    fn consolidate_take_latency(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.submit(Command::ConsolidateTakeLatency {
            loop_id: loop_id.raw(),
        })
    }

    fn supports_composite_loops(&self) -> bool {
        true
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(vec![oxisynth_descriptor()].into())
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match &request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels: *audio_channels,
                midi: *midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetProcessor {
                processor_type,
                dry_audio_channels: 2,
                wet_audio_channels: 2,
                dry_midi: true,
            } if processor_type == TrackProcessorTypeId::OXISYNTH => {
                let track_id = BackendTrackId::from_raw(self.next_track_id);
                let ports = browser_oxisynth_port_descriptors(
                    &request.port_name_base,
                    &mut self.next_port_id,
                );
                let loops: Vec<_> = (0..request.initial_loops)
                    .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
                    .collect();
                self.submit(Command::CreateTrack {
                    expected_track_id: track_id.raw(),
                    expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
                    port_name_base: request.port_name_base,
                    topology: WireTrackTopology::OxiSynth,
                })?;
                self.next_track_id = self.next_track_id.saturating_add(1);
                self.next_loop_id = self.next_loop_id.saturating_add(loops.len() as u64);
                self.track_resources.insert(
                    track_id,
                    BrowserTrackResources {
                        topology: request.topology.clone(),
                        loops: loops.clone(),
                    },
                );
                Ok(BackendTrackCreation {
                    track_id,
                    loops,
                    ports,
                })
            }
            _ => Err(anyhow!("requested browser track processor is unavailable")),
        }
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        Err(anyhow!("standalone browser loops are unsupported"))
    }

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        let id = BackendCompositeId::from_raw(self.next_composite_id);
        self.submit(Command::CreateComposite {
            expected_composite_id: id.raw(),
        })?;
        self.next_composite_id = self.next_composite_id.saturating_add(1);
        self.reserved_composites.insert(id);
        Ok(id)
    }

    fn configure_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        let wire = WireCompositeConfig {
            kind: match config.kind {
                BackendCompositeKind::Regular => WireCompositeKind::Regular,
                BackendCompositeKind::Script => WireCompositeKind::Script,
            },
            sync_source: config.sync_source.raw(),
            timelines: config
                .timelines
                .iter()
                .map(|timeline| {
                    timeline
                        .iter()
                        .map(|section| {
                            section
                                .iter()
                                .map(|entry| WireCompositeEntry {
                                    target: match entry.target {
                                        BackendCompositeTarget::Loop(id) => {
                                            WireCompositeTarget::Loop(id.raw())
                                        }
                                        BackendCompositeTarget::Composite(id) => {
                                            WireCompositeTarget::Composite(id.raw())
                                        }
                                    },
                                    delay: entry.delay,
                                    n_cycles: entry.n_cycles,
                                    mode: entry.mode.map(to_wire_loop_mode),
                                })
                                .collect()
                        })
                        .collect()
                })
                .collect(),
        };
        self.submit(Command::ConfigureComposite {
            composite_id: composite_id.raw(),
            config: wire,
        })
    }

    fn transition_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    ) -> Result<()> {
        self.submit_ephemeral(Command::TransitionComposite {
            composite_id: composite_id.raw(),
            mode: to_wire_loop_mode(mode),
            cycles_delay,
            align_to_iteration,
        })
    }

    fn set_composite_play_after_record(
        &mut self,
        composite_id: BackendCompositeId,
        enabled: bool,
    ) -> Result<()> {
        self.submit(Command::SetCompositePlayAfterRecord {
            composite_id: composite_id.raw(),
            enabled,
        })
    }

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<()> {
        self.submit(Command::RemoveComposite {
            composite_id: composite_id.raw(),
        })?;
        self.reserved_composites.remove(&composite_id);
        Ok(())
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        let ports = browser_port_descriptors(
            &request.port_name_base,
            request.audio_channels,
            request.midi,
            &mut self.next_port_id,
        );
        let loops: Vec<_> = (0..request.initial_loops)
            .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
            .collect();
        self.submit(Command::CreateTrack {
            expected_track_id: track_id.raw(),
            expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
            port_name_base: request.port_name_base.clone(),
            topology: WireTrackTopology::Direct {
                audio_channels: request.audio_channels,
                midi: request.midi,
            },
        })?;
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.next_loop_id = self.next_loop_id.saturating_add(loops.len() as u64);
        self.track_resources.insert(
            track_id,
            BrowserTrackResources {
                topology: BackendTrackTopology::Direct {
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                },
                loops: loops.clone(),
            },
        );
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        if !self.track_resources.contains_key(&track_id) {
            return Ok(());
        }
        self.submit(Command::RemoveTrack {
            track_id: track_id.raw(),
        })?;
        if let Some(resources) = self.track_resources.remove(&track_id) {
            for loop_id in &resources.loops {
                self.waveform_revisions.remove(loop_id);
                self.waveforms.remove(loop_id);
                self.midi_data_generations.remove(loop_id);
                self.midi_data.remove(loop_id);
            }
            self.pending_removed_tracks.insert(track_id, resources);
        }
        Ok(())
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        if !self.track_resources.contains_key(&track_id) {
            return Err(anyhow!("unknown remote backend track {track_id:?}"));
        }
        let loop_id = BackendLoopId::from_raw(self.next_loop_id);
        self.submit(Command::AddLoop {
            track_id: track_id.raw(),
            expected_loop_id: loop_id.raw(),
        })?;
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.track_resources
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("browser track resources are missing"))?
            .loops
            .push(loop_id);
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        if !self.track_resources.contains_key(&track_id) {
            return Err(anyhow!("unknown remote backend track {track_id:?}"));
        }
        self.submit(Command::SetTrackControl {
            track_id: track_id.raw(),
            control: to_wire_track_control(control),
        })
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        let fx = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?
            .fx
            .as_ref()
            .ok_or_else(|| anyhow!("track has no processor"))?;
        if let BackendTrackFxControl::OxiSynth(oxisynth) = &control {
            if !matches!(
                fx.editor.as_ref(),
                Some(TrackProcessorEditorState::OxiSynth(_))
            ) {
                return Err(anyhow!("track has no OxiSynth editor state"));
            }
            match oxisynth {
                OxiSynthControl::SelectPreset(id)
                    if !matches!(
                        oxisynth_descriptor().editor,
                        Some(shoop_app_api::TrackProcessorEditorDescriptor::OxiSynth { presets })
                            if presets.iter().any(|preset| preset.id == *id)
                    ) =>
                {
                    return Err(anyhow!("unknown OxiSynth preset {id}"));
                }
                OxiSynthControl::SetReverbSend(value) | OxiSynthControl::SetChorusSend(value)
                    if !value.is_finite() || !(0.0..=1.0).contains(value) =>
                {
                    return Err(anyhow!("invalid OxiSynth send"));
                }
                OxiSynthControl::AssignMidiCc(assignment)
                    if assignment.channel > 15 || assignment.controller > 127 =>
                {
                    return Err(anyhow!("invalid OxiSynth MIDI CC assignment"));
                }
                _ => {}
            }
        }
        let command = Command::SetTrackFxControl {
            track_id: track_id.raw(),
            control: to_wire_track_fx_control(control.clone()),
        };
        if matches!(
            &control,
            BackendTrackFxControl::ToggleOrRecover
                | BackendTrackFxControl::ClearLogs
                | BackendTrackFxControl::OxiSynth(OxiSynthControl::Panic)
        ) {
            self.submit_ephemeral(command)
        } else {
            self.submit(command)
        }
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        let track = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?;
        let Some(fx) = &track.fx else {
            return Ok(None);
        };
        match &fx.editor {
            Some(TrackProcessorEditorState::OxiSynth(editor)) => {
                Ok(Some(encode_oxisynth_state(editor)?))
            }
            None => Ok(None),
        }
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        let track = self
            .track_resources
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown remote backend track {track_id:?}"))?;
        if !track.topology.has_midi() {
            return Err(anyhow!(
                "remote backend track has no MIDI input {track_id:?}"
            ));
        }
        if events.len() > MIDI_BATCH_CAPACITY
            || events
                .iter()
                .any(|event| event.time != 0 || event.data.is_empty() || event.data.len() > 4)
        {
            return Err(anyhow!("invalid browser MIDI input injection batch"));
        }
        self.submit_ephemeral(Command::InjectTrackMidiInput {
            track_id: track_id.raw(),
            events: events
                .iter()
                .map(|event| WireMidiEvent {
                    frame: event.time,
                    data: event.data.clone(),
                })
                .collect(),
        })
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopGain {
            loop_id: loop_id.raw(),
            gain,
        })
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopBalance {
            loop_id: loop_id.raw(),
            balance,
        })
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        for request in requests {
            if !self.has_loop(request.loop_id) {
                return Err(anyhow!(
                    "unknown browser backend loop {:?}",
                    request.loop_id
                ));
            }
        }
        self.submit_ephemeral(Command::GrabLoops {
            requests: requests
                .iter()
                .map(|request| WireGrabRequest {
                    loop_id: request.loop_id.raw(),
                    reverse_start_cycle: request.reverse_start_cycle,
                    cycles_length: request.cycles_length,
                    go_to_cycle: request.go_to_cycle,
                    go_to_mode: to_wire_loop_mode(request.go_to_mode),
                })
                .collect(),
        })?;
        for request in requests {
            self.waveforms.remove(&request.loop_id);
            self.midi_data.remove(&request.loop_id);
        }
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        if let Some(assembly) = self.waveforms.get(&loop_id) {
            if assembly.complete {
                return Ok(Some(
                    assembly
                        .channels
                        .iter()
                        .map(|channel| Arc::from(channel.clone()))
                        .collect(),
                ));
            }
            return Ok(None);
        }
        let revision = self
            .waveform_revisions
            .entry(loop_id)
            .and_modify(|revision| *revision = revision.saturating_add(1))
            .or_insert(1);
        self.waveforms.insert(
            loop_id,
            WaveformAssembly {
                revision: *revision,
                channels: Vec::new(),
                timing: Vec::new(),
                next_channel: 0,
                next_offset: 0,
                complete: false,
                in_flight: false,
            },
        );
        self.request_waveform_chunk(loop_id)?;
        Ok(None)
    }

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        let Some(channels) = self.loop_audio_data(loop_id)? else {
            return Ok(None);
        };
        let timing = &self
            .waveforms
            .get(&loop_id)
            .ok_or_else(|| anyhow!("missing completed waveform assembly"))?
            .timing;
        Ok(Some(BackendAudioData {
            channels: channels
                .into_iter()
                .enumerate()
                .map(|(index, samples)| {
                    let (start_offset, preplay) = timing.get(index).copied().unwrap_or_default();
                    BackendAudioChannelData {
                        samples,
                        start_offset,
                        preplay,
                        latency: Default::default(),
                    }
                })
                .collect(),
        }))
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        if let Some(assembly) = self.midi_data.get(&loop_id) {
            return Ok(assembly.complete.then(|| BackendMidiData {
                channels: assembly.channels.clone(),
            }));
        }
        self.restart_midi_data(loop_id)?;
        Ok(None)
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        self.submit(Command::SetLoopSyncSource {
            loop_id: loop_id.raw(),
            source: source.map(BackendLoopId::raw),
        })
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        self.submit_ephemeral(Command::TransitionLoop {
            loop_id: loop_id.raw(),
            mode: to_wire_loop_mode(mode),
            cycles_delay,
        })?;
        self.waveforms.remove(&loop_id);
        self.midi_data.remove(&loop_id);
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.submit_ephemeral(Command::ClearLoop {
            loop_id: loop_id.raw(),
        })?;
        self.waveforms.remove(&loop_id);
        self.midi_data.remove(&loop_id);
        Ok(())
    }

    fn replace_loop_content(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<()> {
        match self.replace_loop_content_async(loop_id, update)? {
            BackendAsyncResult::Ready(()) => Ok(()),
            BackendAsyncResult::Pending(_) => Err(anyhow!(
                "asynchronous loop content replacement is not complete"
            )),
        }
    }

    fn replace_loop_content_async(
        &mut self,
        loop_id: BackendLoopId,
        update: &BackendLoopContentUpdate,
    ) -> Result<BackendAsyncResult<()>> {
        if let Some(error) = self.loop_content_replace_error.take() {
            return Err(anyhow!(error));
        }
        if update.audio.is_empty() && update.midi.is_empty() {
            return Err(anyhow!("loop content update is empty"));
        }
        if let Some(replace) = &self.loop_content_replace {
            if replace.loop_id != loop_id || &replace.update != update {
                return Err(anyhow!("another loop content replacement is active"));
            }
            if replace.complete {
                self.loop_content_replace = None;
                self.waveforms.remove(&loop_id);
                self.midi_data.remove(&loop_id);
                return Ok(BackendAsyncResult::Ready(()));
            }
            let progress = BackendOperationProgress {
                key: replace.generation,
                kind: BackendOperationKind::LoopContentReplacement,
                completed: replace.next_offset,
                total: Some(replace.bytes.len()),
            };
            self.pump_loop_content_replace()?;
            return Ok(BackendAsyncResult::Pending(progress));
        }
        let bytes = serde_json::to_vec(update)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!(
                "prepared loop content exceeds browser transfer limit"
            ));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport_generation = self.transport.borrow().diagnostics().generation;
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginLoopContentReplace {
                generation,
                loop_id: loop_id.raw(),
                total_bytes: bytes.len(),
            })?;
        let total = bytes.len();
        self.loop_content_replace = Some(LoopContentReplaceAssembly {
            generation,
            loop_id,
            update: update.clone(),
            bytes,
            next_offset: 0,
            commit_sent: false,
            complete: false,
        });
        self.pump_loop_content_replace()?;
        Ok(BackendAsyncResult::Pending(BackendOperationProgress {
            key: generation,
            kind: BackendOperationKind::LoopContentReplacement,
            completed: 0,
            total: Some(total),
        }))
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        self.submit(Command::SetLoopLength {
            loop_id: loop_id.raw(),
            length,
        })
    }

    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        self.submit(Command::SetLoopTiming {
            loop_id: loop_id.raw(),
            start_offset,
            preplay,
            length,
        })?;
        if let Some(assembly) = self.waveforms.get_mut(&loop_id) {
            for timing in &mut assembly.timing {
                if let Some(offset) = start_offset {
                    timing.0 = offset;
                }
                if let Some(samples) = preplay {
                    timing.1 = samples;
                }
            }
        }
        if let Some(assembly) = self.midi_data.get_mut(&loop_id) {
            for channel in &mut assembly.channels {
                if let Some(offset) = start_offset {
                    channel.start_offset = offset;
                }
                if let Some(samples) = preplay {
                    channel.preplay = samples;
                }
                if let Some(length) = length {
                    channel.length = length;
                }
            }
        }
        Ok(())
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        match self.capture_session_async()? {
            BackendAsyncResult::Ready(session) => Ok(session),
            BackendAsyncResult::Pending(_) => {
                Err(anyhow!("asynchronous session capture is not complete"))
            }
        }
    }

    fn capture_session_async(&mut self) -> Result<BackendAsyncResult<BackendSessionData>> {
        if let Some(error) = self.session_capture_error.take() {
            return Err(anyhow!(error));
        }
        if let Some(capture) = &self.session_capture {
            if capture.total_bytes == Some(capture.bytes.len()) && capture.in_flight == 0 {
                let session: BackendSessionData = serde_json::from_slice(&capture.bytes)
                    .map_err(|error| anyhow!("invalid worklet session capture: {error}"))?;
                for global in &session.global_ports {
                    self.snapshot
                        .connections
                        .application_ports
                        .insert(global.descriptor.id, global.descriptor.clone());
                }
                self.session_capture = None;
                return Ok(BackendAsyncResult::Ready(session));
            }
            return Ok(BackendAsyncResult::Pending(BackendOperationProgress {
                key: capture.generation,
                kind: BackendOperationKind::SessionCapture,
                completed: capture.bytes.len(),
                total: capture.total_bytes,
            }));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport_generation = self.transport.borrow().diagnostics().generation;
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionCapture { generation })?;
        self.session_capture = Some(SessionCaptureAssembly {
            generation,
            total_bytes: None,
            bytes: Vec::new(),
            next_offset: 0,
            in_flight: 0,
        });
        Ok(BackendAsyncResult::Pending(BackendOperationProgress {
            key: generation,
            kind: BackendOperationKind::SessionCapture,
            completed: 0,
            total: None,
        }))
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        match self.replace_session_async(session)? {
            BackendAsyncResult::Ready(replacement) => Ok(replacement),
            BackendAsyncResult::Pending(_) => {
                Err(anyhow!("asynchronous session replacement is not complete"))
            }
        }
    }

    fn replace_session_async(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendAsyncResult<BackendSessionReplacement>> {
        if let Some(error) = self.session_replace_error.take() {
            return Err(anyhow!(error));
        }
        if let Some(replace) = &self.session_replace {
            if &replace.session != session {
                return Err(anyhow!("another session replacement is active"));
            }
            if replace.complete {
                let replacement = browser_replacement_mapping(session);
                self.apply_replaced_session(session, &replacement);
                self.session_replace = None;
                return Ok(BackendAsyncResult::Ready(replacement));
            }
            let progress = BackendOperationProgress {
                key: replace.generation,
                kind: BackendOperationKind::SessionReplacement,
                completed: replace.next_offset,
                total: Some(replace.bytes.len()),
            };
            self.pump_session_replace()?;
            return Ok(BackendAsyncResult::Pending(progress));
        }
        let bytes = serde_json::to_vec(session)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("prepared session exceeds browser transfer limit"));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport_generation = self.transport.borrow().diagnostics().generation;
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionReplace {
                generation,
                total_bytes: bytes.len(),
            })?;
        let total = bytes.len();
        self.session_replace = Some(SessionReplaceAssembly {
            generation,
            session: session.clone(),
            bytes,
            next_offset: 0,
            commit_sent: false,
            complete: false,
        });
        self.pump_session_replace()?;
        Ok(BackendAsyncResult::Pending(BackendOperationProgress {
            key: generation,
            kind: BackendOperationKind::SessionReplacement,
            completed: 0,
            total: Some(total),
        }))
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        let port = self
            .snapshot
            .connections
            .application_ports
            .get(&port_id)
            .ok_or_else(|| anyhow!("unknown browser application port {port_id:?}"))?;
        let host = self
            .snapshot
            .connections
            .host_ports
            .get(external_port)
            .ok_or_else(|| anyhow!("browser host port disappeared: {external_port}"))?;
        if port.data_type != host.data_type || port.direction == host.direction {
            return Err(anyhow!(
                "browser host port is incompatible: {external_port}"
            ));
        }
        self.submit(Command::SetPortConnected {
            application_port_id: port_id.raw(),
            host_port_id: external_port.to_owned(),
            connected,
        })
    }

    fn advance(&mut self, elapsed: Duration) {
        self.poll_elapsed = self.poll_elapsed.saturating_add(elapsed);
    }

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let transport = self.transport.borrow().diagnostics();
        let connection = self.transport.borrow().readiness().connection;
        if self.transport_generation != 0 && transport.generation != self.transport_generation {
            self.cancel_transfers("driver generation changed");
        } else if connection == ConnectionState::Detached
            && (self.session_capture.is_some()
                || self.session_replace.is_some()
                || self.loop_content_replace.is_some())
        {
            self.cancel_transfers("transport detached");
        }
        self.transport_generation = transport.generation;
        self.sync_midi_endpoints()?;
        let readiness = self.transport.borrow().readiness();
        let state = self.transport.borrow().driver_state();
        let running = matches!(state, BackendDriverState::Running);
        self.pump_midi_input(running)?;
        self.snapshot.status.driver_state = state;
        self.snapshot.status.command_overflows = self.transport.borrow().overflows();
        let engine_pollable = readiness.connection == ConnectionState::Attached
            && readiness.protocol == ProtocolState::Negotiated
            && readiness.replay == ReplayState::Complete
            && matches!(
                readiness.driver_state,
                BackendDriverState::Running
                    | BackendDriverState::Dummy
                    | BackendDriverState::Suspended
            );
        if engine_pollable
            && self.poll_elapsed >= Duration::from_millis(u64::from(STATUS_INTERVAL_MS))
            && self.transport.borrow().pending_len() < COMMAND_CAPACITY / 2
        {
            self.transport.borrow_mut().ephemeral(Command::Poll)?;
            self.transport
                .borrow_mut()
                .ephemeral(Command::DrainMidiOutput {
                    max_events: MIDI_BATCH_CAPACITY,
                })?;
            self.poll_elapsed = Duration::ZERO;
        }
        let events = self.transport.borrow_mut().drain_events();
        for received in events {
            let sequence = received.envelope.sequence;
            let generation = received.generation;
            match received.envelope.event {
                Event::Ack | Event::Stopped => {}
                Event::Error { message } => {
                    self.transport
                        .borrow_mut()
                        .reject_journaled(&received.command);
                    match &received.command {
                        Command::CreateTrack {
                            expected_track_id, ..
                        } => {
                            self.track_resources
                                .remove(&BackendTrackId::from_raw(*expected_track_id));
                        }
                        Command::RemoveTrack { track_id } => {
                            let track_id = BackendTrackId::from_raw(*track_id);
                            if let Some(resources) = self.pending_removed_tracks.remove(&track_id) {
                                self.track_resources.insert(track_id, resources);
                            }
                        }
                        Command::AddLoop {
                            track_id,
                            expected_loop_id,
                        } => {
                            if let Some(resources) = self
                                .track_resources
                                .get_mut(&BackendTrackId::from_raw(*track_id))
                            {
                                resources
                                    .loops
                                    .retain(|loop_id| loop_id.raw() != *expected_loop_id);
                            }
                        }
                        Command::CreateComposite {
                            expected_composite_id,
                        } => {
                            self.reserved_composites
                                .remove(&BackendCompositeId::from_raw(*expected_composite_id));
                        }
                        Command::RemoveComposite { composite_id } => {
                            self.reserved_composites
                                .insert(BackendCompositeId::from_raw(*composite_id));
                        }
                        _ => {}
                    }
                    if let Some((operation, operation_generation)) =
                        transfer_identity(&received.command)
                    {
                        match operation {
                            BackendOperationKind::SessionCapture => {
                                self.session_capture = None;
                                self.session_capture_error = Some(message.clone());
                            }
                            BackendOperationKind::SessionReplacement => {
                                self.session_replace = None;
                                self.session_replace_error = Some(message.clone());
                            }
                            BackendOperationKind::LoopContentReplacement => {
                                self.loop_content_replace = None;
                                self.loop_content_replace_error = Some(message.clone());
                            }
                        }
                        self.snapshot
                            .mutation_failures
                            .push(BackendMutationFailure {
                                driver_generation: generation,
                                sequence,
                                operation_key: Some(operation_generation),
                                kind: match operation {
                                    BackendOperationKind::LoopContentReplacement => {
                                        BackendMutationKind::LoopContent
                                    }
                                    BackendOperationKind::SessionCapture
                                    | BackendOperationKind::SessionReplacement => {
                                        BackendMutationKind::SessionTransfer
                                    }
                                },
                                entity: command_mutation_identity(&received.command)
                                    .and_then(|(_, entity)| entity),
                                detail: mutation_detail(&received.command),
                                message,
                            });
                    } else if let Some((kind, entity)) =
                        command_mutation_identity(&received.command)
                    {
                        self.snapshot
                            .mutation_failures
                            .push(BackendMutationFailure {
                                driver_generation: generation,
                                sequence,
                                operation_key: None,
                                kind,
                                entity,
                                detail: mutation_detail(&received.command),
                                message,
                            });
                    } else {
                        return Err(anyhow!(message));
                    }
                }
                Event::ConnectionMutationFailed {
                    application_port_id,
                    host_port_id,
                    desired_connected,
                    message,
                } => self
                    .snapshot
                    .connections
                    .failures
                    .push(BackendConnectionFailure {
                        port_id: BackendPortId::from_raw(application_port_id),
                        external_port: host_port_id,
                        desired_connected,
                        message,
                    }),
                Event::MidiOutput {
                    events,
                    dropped,
                    refused_input,
                } => {
                    self.transport
                        .borrow_mut()
                        .add_overflows(dropped.saturating_add(refused_input));
                    for event in events {
                        if let Err(error) = self.midi.send(&event.host_port_id, &event.data) {
                            self.snapshot
                                .connections
                                .failures
                                .push(BackendConnectionFailure {
                                    port_id: BackendPortId::from_raw(event.application_port_id),
                                    external_port: event.host_port_id,
                                    desired_connected: true,
                                    message: error.to_string(),
                                });
                        }
                    }
                }
                Event::Snapshot(snapshot) => self.apply_wire_snapshot(snapshot),
                Event::Waveform(chunk) => self.apply_waveform_chunk(chunk)?,
                Event::MidiData(chunk) => self.apply_midi_data_chunk(chunk)?,
                Event::SessionCaptureReady {
                    generation,
                    total_bytes,
                } => self.apply_session_capture_ready(generation, total_bytes)?,
                Event::SessionCaptureChunk {
                    generation,
                    offset,
                    total_bytes,
                    final_chunk,
                    bytes,
                } => self.apply_session_capture_chunk(
                    generation,
                    offset,
                    total_bytes,
                    final_chunk,
                    bytes,
                )?,
                Event::SessionReplaceComplete { generation } => {
                    if let Some(replace) = self.session_replace.as_mut() {
                        if replace.generation == generation {
                            replace.complete = true;
                        }
                    }
                }
                Event::LoopContentReplaceComplete { generation } => {
                    if let Some(replace) = self.loop_content_replace.as_mut() {
                        if replace.generation == generation {
                            replace.complete = true;
                        }
                    }
                }
                Event::SessionTransferAborted { generation } => {
                    if self
                        .session_capture
                        .as_ref()
                        .is_some_and(|capture| capture.generation == generation)
                    {
                        self.session_capture = None;
                        self.session_capture_error = Some(format!(
                            "session capture operation {generation} was cancelled"
                        ));
                    }
                    if self
                        .session_replace
                        .as_ref()
                        .is_some_and(|replace| replace.generation == generation)
                    {
                        self.session_replace = None;
                        self.session_replace_error = Some(format!(
                            "session replacement operation {generation} was cancelled"
                        ));
                    }
                    if self
                        .loop_content_replace
                        .as_ref()
                        .is_some_and(|replace| replace.generation == generation)
                    {
                        self.loop_content_replace = None;
                        self.loop_content_replace_error = Some(format!(
                            "loop content replacement operation {generation} was cancelled"
                        ));
                    }
                }
            }
        }
        self.pump_session_replace()?;
        self.pump_loop_content_replace()?;
        if let Some(error) = self.transport.borrow_mut().take_error() {
            return Err(anyhow!(error));
        }
        let snapshot = self.snapshot.clone();
        self.snapshot.status.xruns = 0;
        self.snapshot.mutation_failures.clear();
        Ok(snapshot)
    }

    fn wait_idle(&mut self) {}
}

fn from_wire_data_type(value: WirePortDataType) -> BackendPortDataType {
    match value {
        WirePortDataType::Audio => BackendPortDataType::Audio,
        WirePortDataType::Midi => BackendPortDataType::Midi,
    }
}

fn from_wire_direction(value: WirePortDirection) -> BackendPortDirection {
    match value {
        WirePortDirection::Input => BackendPortDirection::Input,
        WirePortDirection::Output => BackendPortDirection::Output,
    }
}

fn from_wire_role(value: WirePortRole) -> BackendPortRole {
    match value {
        WirePortRole::AudioInput => BackendPortRole::AudioInput,
        WirePortRole::AudioOutput => BackendPortRole::AudioOutput,
        WirePortRole::AudioSend => BackendPortRole::AudioSend,
        WirePortRole::AudioReturn => BackendPortRole::AudioReturn,
        WirePortRole::MidiInput => BackendPortRole::MidiInput,
        WirePortRole::MidiOutput => BackendPortRole::MidiOutput,
        WirePortRole::MidiSend => BackendPortRole::MidiSend,
    }
}

fn to_wire_track_control(control: BackendTrackControl) -> WireTrackControl {
    match control {
        BackendTrackControl::OutputGainDb(value) => WireTrackControl::OutputGainDb(value),
        BackendTrackControl::OutputBalance(value) => WireTrackControl::OutputBalance(value),
        BackendTrackControl::OutputMute(value) => WireTrackControl::OutputMute(value),
        BackendTrackControl::InputGainDb(value) => WireTrackControl::InputGainDb(value),
        BackendTrackControl::InputBalance(value) => WireTrackControl::InputBalance(value),
        BackendTrackControl::InputMonitoring(value) => WireTrackControl::InputMonitoring(value),
    }
}

fn from_wire_oxisynth_parameter(parameter: WireOxiSynthParameter) -> OxiSynthParameter {
    match parameter {
        WireOxiSynthParameter::ReverbSend => OxiSynthParameter::ReverbSend,
        WireOxiSynthParameter::ChorusSend => OxiSynthParameter::ChorusSend,
    }
}

fn to_wire_oxisynth_parameter(parameter: OxiSynthParameter) -> WireOxiSynthParameter {
    match parameter {
        OxiSynthParameter::ReverbSend => WireOxiSynthParameter::ReverbSend,
        OxiSynthParameter::ChorusSend => WireOxiSynthParameter::ChorusSend,
    }
}

fn to_wire_track_fx_control(control: BackendTrackFxControl) -> WireTrackFxControl {
    match control {
        BackendTrackFxControl::SetActive(value) => WireTrackFxControl::SetActive(value),
        BackendTrackFxControl::SetVisible(value) => WireTrackFxControl::SetVisible(value),
        BackendTrackFxControl::ToggleOrRecover => WireTrackFxControl::ToggleOrRecover,
        BackendTrackFxControl::RestoreState(value) => WireTrackFxControl::RestoreState(value),
        BackendTrackFxControl::ClearLogs => WireTrackFxControl::ClearLogs,
        BackendTrackFxControl::OxiSynth(control) => match control {
            OxiSynthControl::SelectPreset(value) => WireTrackFxControl::OxiSelectPreset(value),
            OxiSynthControl::SetReverbSend(value) => WireTrackFxControl::OxiSetReverbSend(value),
            OxiSynthControl::SetChorusSend(value) => WireTrackFxControl::OxiSetChorusSend(value),
            OxiSynthControl::AssignMidiCc(assignment) => {
                WireTrackFxControl::OxiAssignMidiCc(WireOxiSynthMidiCcAssignment {
                    parameter: to_wire_oxisynth_parameter(assignment.parameter),
                    channel: assignment.channel,
                    controller: assignment.controller,
                })
            }
            OxiSynthControl::RemoveMidiCc(parameter) => {
                WireTrackFxControl::OxiRemoveMidiCc(to_wire_oxisynth_parameter(parameter))
            }
            OxiSynthControl::ClearMidiCcAssignments => {
                WireTrackFxControl::OxiClearMidiCcAssignments
            }
            OxiSynthControl::Panic => WireTrackFxControl::OxiPanic,
        },
    }
}

fn to_wire_loop_mode(mode: BackendLoopMode) -> WireLoopMode {
    match mode {
        BackendLoopMode::Unknown => WireLoopMode::Unknown,
        BackendLoopMode::Stopped => WireLoopMode::Stopped,
        BackendLoopMode::Playing => WireLoopMode::Playing,
        BackendLoopMode::Recording => WireLoopMode::Recording,
        BackendLoopMode::Replacing => WireLoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => WireLoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => WireLoopMode::RecordingDryIntoWet,
    }
}

fn from_wire_loop_mode(mode: WireLoopMode) -> BackendLoopMode {
    match mode {
        WireLoopMode::Unknown => BackendLoopMode::Unknown,
        WireLoopMode::Stopped => BackendLoopMode::Stopped,
        WireLoopMode::Playing => BackendLoopMode::Playing,
        WireLoopMode::Recording => BackendLoopMode::Recording,
        WireLoopMode::Replacing => BackendLoopMode::Replacing,
        WireLoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        WireLoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}
#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use shoop_audio_protocol::{CommandEnvelope, EventEnvelope, PROTOCOL_VERSION};

    use super::*;

    #[cfg(all(target_arch = "wasm32", feature = "wasm-test-browser"))]
    shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

    #[shoop_wasm_test_support::shoop_test]
    fn oxisynth_port_reservations_match_worklet_registration_order() {
        let mut next_port_id = 10;
        let ports = browser_oxisynth_port_descriptors("synth", &mut next_port_id);
        assert_eq!(next_port_id, 15);
        assert_eq!(
            ports
                .iter()
                .map(|port| (port.id.raw(), port.role, port.direction))
                .collect::<Vec<_>>(),
            vec![
                (10, BackendPortRole::AudioInput, BackendPortDirection::Input,),
                (11, BackendPortRole::AudioInput, BackendPortDirection::Input,),
                (
                    12,
                    BackendPortRole::AudioOutput,
                    BackendPortDirection::Output,
                ),
                (
                    13,
                    BackendPortRole::AudioOutput,
                    BackendPortDirection::Output,
                ),
                (14, BackendPortRole::MidiInput, BackendPortDirection::Input,),
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn worklet_backend_exposes_observed_capability_without_inventing_zero() {
        let (mut backend, _control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        assert_eq!(
            backend.latency_capability(),
            BackendLatencyCapability::Observed
        );
        assert!(backend
            .configure_audio_context_latency(Some(0.01), Some(0.005), 48_000, 2)
            .is_ok());
        assert!(backend
            .configure_audio_context_latency(Some(f64::NAN), None, 48_000, 3)
            .is_err());
        assert!(backend
            .set_take_latency_policy(BackendLoopId::from_raw(1), 0)
            .is_ok());
        assert_eq!(
            backend.snapshot.status.backend_playback_latency,
            LatencyObservationState::default()
        );
    }

    #[derive(Default)]
    struct MemoryEndpoint {
        sent: Rc<RefCell<Vec<String>>>,
    }

    impl MessageEndpoint for MemoryEndpoint {
        fn post_message(&self, message: &str) -> Result<()> {
            self.sent.borrow_mut().push(message.to_owned());
            Ok(())
        }
    }

    fn deliver(control: &RemoteBackendControl, generation: u64, sequence: u64, event: Event) {
        control
            .receive(
                generation,
                &serde_json::to_string(&EventEnvelope {
                    version: PROTOCOL_VERSION,
                    sequence,
                    event,
                })
                .unwrap(),
            )
            .unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn remote_backend_reserves_stable_ids_replays_and_assembles_observed_snapshots() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        let creation = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "track".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        assert_eq!(creation.track_id, BackendTrackId::from_raw(1));
        assert_eq!(
            creation.loops,
            vec![BackendLoopId::from_raw(1), BackendLoopId::from_raw(2)]
        );

        backend.midi_revision = 0;
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.set_driver_state(BackendDriverState::Running);
        control.attach(Box::new(endpoint), 9, 1, 2).unwrap();
        assert_eq!(
            backend.poll().unwrap().status.driver_state,
            BackendDriverState::Starting
        );
        let commands = sent
            .borrow()
            .iter()
            .map(|message| serde_json::from_str::<CommandEnvelope>(message).unwrap())
            .collect::<Vec<_>>();
        assert!(matches!(
            commands[0].command,
            Command::ConfigureDeviceChannels {
                input_channels: 1,
                output_channels: 2
            }
        ));
        assert!(matches!(
            commands[1].command,
            Command::CreateTrack {
                expected_track_id: 1,
                ref expected_loop_ids,
                ..
            } if expected_loop_ids == &[1, 2]
        ));

        deliver(&control, 9, 1, Event::Ack);
        deliver(&control, 9, 2, Event::Ack);
        backend
            .transport
            .borrow_mut()
            .ephemeral(Command::Poll)
            .unwrap();
        deliver(
            &control,
            9,
            3,
            Event::Snapshot(WireSnapshot {
                sample_rate: 48_000,
                quantum: 128,
                callback_count: 12,
                processed_frames: 1_536,
                ..Default::default()
            }),
        );
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.status.sample_rate, 48_000);
        assert_eq!(snapshot.status.buffer_size, 128);
        assert_eq!(snapshot.status.callback_count, 12);
        assert_eq!(snapshot.status.processed_frames, 1_536);
        assert_eq!(snapshot.status.driver_state, BackendDriverState::Running);
        assert!(control.readiness().is_ready());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn waveform_timing_is_assembled_edited_and_replayed_without_losing_partial_updates() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        deliver(&control, 1, 1, Event::Ack);
        let creation = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "timing".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        deliver(&control, 1, 2, Event::Ack);
        let loop_id = creation.loops[0];

        assert!(backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .is_none());
        deliver(
            &control,
            1,
            3,
            Event::Waveform(WaveformChunk {
                loop_id: loop_id.raw(),
                revision: 1,
                channel: 0,
                channel_count: 1,
                offset: 0,
                total_samples: 3,
                start_offset: -4,
                preplay: 6,
                final_chunk: true,
                samples: vec![0.25, -0.5, 0.75],
            }),
        );
        backend.poll().unwrap();
        let audio = backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .unwrap();
        assert_eq!(audio.channels[0].samples.as_ref(), [0.25, -0.5, 0.75]);
        assert_eq!(audio.channels[0].start_offset, -4);
        assert_eq!(audio.channels[0].preplay, 6);

        backend.midi_data.insert(
            loop_id,
            MidiDataAssembly {
                generation: 1,
                channels: vec![BackendMidiChannelData {
                    content_revision: 1,
                    mode: BackendChannelMode::Direct,
                    length: 16,
                    events: Vec::new(),
                    start_offset: -4,
                    preplay: 6,
                    latency: Default::default(),
                }],
                next_channel: 1,
                next_offset: 0,
                complete: true,
                in_flight: false,
            },
        );
        backend
            .set_loop_timing(loop_id, Some(-8), None, None)
            .unwrap();
        backend
            .set_loop_timing(loop_id, None, Some(12), None)
            .unwrap();
        backend
            .set_loop_timing(loop_id, None, None, Some(32))
            .unwrap();
        let audio = backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .unwrap();
        assert_eq!(audio.channels[0].start_offset, -8);
        assert_eq!(audio.channels[0].preplay, 12);
        let midi = &backend.midi_data[&loop_id].channels[0];
        assert_eq!((midi.start_offset, midi.preplay, midi.length), (-8, 12, 32));

        let commands_before_restart = sent.borrow().len();
        control.detach(false);
        let restarted = MemoryEndpoint::default();
        let replayed = restarted.sent.clone();
        control.attach(Box::new(restarted), 2, 0, 2).unwrap();
        let commands = replayed
            .borrow()
            .iter()
            .map(|message| {
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert!(commands_before_restart >= 6);
        assert_eq!(
            commands
                .iter()
                .filter(|command| matches!(command, Command::SetLoopTiming { .. }))
                .count(),
            3
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rich_wire_snapshot_converts_every_remote_domain_shape() {
        use shoop_audio_protocol::{
            WireActiveCompositeChild, WireApplicationPort, WireApplicationPortOwner,
            WireCompositeState, WireConfirmedLink, WireHostPort, WireLatestMidiMessage,
            WireLoopState, WireOxiSynthState, WireTrackFxState, WireTrackState,
        };

        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control.set_driver_state(BackendDriverState::Running);
        control
            .attach(Box::new(MemoryEndpoint::default()), 3, 2, 2)
            .unwrap();
        deliver(&control, 3, 1, Event::Ack);
        backend
            .transport
            .borrow_mut()
            .ephemeral(Command::Poll)
            .unwrap();

        let track = |id, topology, fx| WireTrackState {
            id,
            topology,
            fx,
            latency_policy: Default::default(),
            audio_channels: 2,
            midi: true,
            output_gain_db: -3.0,
            output_balance: 0.25,
            output_muted: true,
            input_gain_db: -4.0,
            input_balance: -0.25,
            input_monitoring: true,
            input_peaks: vec![0.1, 0.2],
            output_peaks: vec![0.3, 0.4],
            latest_input_midi_message: Some(WireLatestMidiMessage {
                bytes: [0x90, 60, 100, 0],
                len: 3,
            }),
        };
        let modes = [
            WireLoopMode::Unknown,
            WireLoopMode::Stopped,
            WireLoopMode::Playing,
            WireLoopMode::Recording,
            WireLoopMode::Replacing,
            WireLoopMode::PlayingDryThroughWet,
            WireLoopMode::RecordingDryIntoWet,
        ];
        let roles = [
            WirePortRole::AudioInput,
            WirePortRole::AudioOutput,
            WirePortRole::AudioSend,
            WirePortRole::AudioReturn,
            WirePortRole::MidiInput,
            WirePortRole::MidiOutput,
            WirePortRole::MidiSend,
        ];
        deliver(
            &control,
            3,
            2,
            Event::Snapshot(WireSnapshot {
                sample_rate: 48_000,
                quantum: 128,
                callback_count: 4,
                processed_frames: 512,
                input_peak: 0.2,
                output_peak: 0.4,
                xruns: 1,
                callback_budget_overruns: 2,
                render_discontinuities: 3,
                memory_growths: 4,
                render_memory_growths: 5,
                command_overflows: 6,
                storage_low_channels: 7,
                storage_exhaustions: 8,
                backend_capture_latency: Default::default(),
                backend_playback_latency: Default::default(),
                tracks: vec![
                    track(
                        1,
                        WireTrackTopology::Direct {
                            audio_channels: 2,
                            midi: true,
                        },
                        None,
                    ),
                    track(
                        3,
                        WireTrackTopology::OxiSynth,
                        Some(WireTrackFxState {
                            processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                            active: true,
                            visible: false,
                            latency: Default::default(),
                            oxisynth: Some(WireOxiSynthState {
                                selected_preset_id: "0:40".to_owned(),
                                reverb_send: 0.25,
                                chorus_send: 0.5,
                                midi_cc_assignments: Vec::new(),
                            }),
                        }),
                    ),
                ],
                loops: modes
                    .into_iter()
                    .enumerate()
                    .map(|(index, mode)| WireLoopState {
                        id: index as u64 + 1,
                        mode,
                        latency: Default::default(),
                        length: 256,
                        position: 64,
                        next_mode: Some(WireLoopMode::Playing),
                        next_transition_delay: Some(2),
                        stereo: true,
                        gain: 0.75,
                        balance: -0.1,
                        audio_peaks: vec![0.2, 0.3],
                        midi_activity: true,
                    })
                    .collect(),
                composites: vec![WireCompositeState {
                    id: 1,
                    mode: WireLoopMode::Playing,
                    next_mode: Some(WireLoopMode::Stopped),
                    next_transition_delay: Some(1),
                    iteration: 2,
                    cycle_count: 3,
                    length: 4,
                    position: 1,
                    active_plan_version: 5,
                    pending_plan_version: Some(6),
                    active_children: vec![
                        WireActiveCompositeChild {
                            target: WireCompositeTarget::Loop(1),
                            mode: WireLoopMode::Playing,
                            cycle_offset: 0,
                        },
                        WireActiveCompositeChild {
                            target: WireCompositeTarget::Composite(2),
                            mode: WireLoopMode::Recording,
                            cycle_offset: 1,
                        },
                    ],
                }],
                application_ports: roles
                    .into_iter()
                    .enumerate()
                    .map(|(index, role)| WireApplicationPort {
                        id: index as u64 + 1,
                        owner: if index == 0 {
                            WireApplicationPortOwner::GlobalFxControl
                        } else {
                            WireApplicationPortOwner::Track
                        },
                        name: format!("port-{index}"),
                        data_type: if matches!(
                            role,
                            WirePortRole::MidiInput
                                | WirePortRole::MidiOutput
                                | WirePortRole::MidiSend
                        ) {
                            WirePortDataType::Midi
                        } else {
                            WirePortDataType::Audio
                        },
                        direction: if matches!(
                            role,
                            WirePortRole::AudioOutput
                                | WirePortRole::AudioReturn
                                | WirePortRole::MidiOutput
                        ) {
                            WirePortDirection::Output
                        } else {
                            WirePortDirection::Input
                        },
                        role,
                    })
                    .collect(),
                host_ports: vec![
                    WireHostPort {
                        id: "audio-in".to_owned(),
                        name: "Audio In".to_owned(),
                        data_type: WirePortDataType::Audio,
                        direction: WirePortDirection::Input,
                    },
                    WireHostPort {
                        id: "midi-out".to_owned(),
                        name: "MIDI Out".to_owned(),
                        data_type: WirePortDataType::Midi,
                        direction: WirePortDirection::Output,
                    },
                ],
                confirmed_links: vec![WireConfirmedLink {
                    application_port_id: 1,
                    host_port_id: "audio-in".to_owned(),
                }],
            }),
        );
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.tracks.len(), 2);
        let oxisynth_track = BackendTrackId::from_raw(3);
        let Some(TrackProcessorEditorState::OxiSynth(editor)) = snapshot.tracks[&oxisynth_track]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing remote OxiSynth editor state");
        };
        assert_eq!(editor.selected_preset_id, "0:40");
        assert_eq!(
            backend
                .track_fx_state_string(oxisynth_track)
                .unwrap()
                .as_deref(),
            Some("shoop-oxisynth:2:timgm6mb:0:40:3e800000:3f000000")
        );
        backend
            .set_track_fx_control(
                oxisynth_track,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset("0:41".to_owned())),
            )
            .unwrap();
        assert!(backend
            .set_track_fx_control(
                oxisynth_track,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectPreset("1:0".to_owned(),)),
            )
            .is_err());
        backend
            .set_track_fx_control(
                oxisynth_track,
                BackendTrackFxControl::OxiSynth(OxiSynthControl::Panic),
            )
            .unwrap();
        assert_eq!(snapshot.loops.len(), modes.len());
        assert_eq!(snapshot.composites.len(), 1);
        assert_eq!(snapshot.connections.application_ports.len(), roles.len());
        assert_eq!(snapshot.connections.host_ports.len(), 2);
        assert_eq!(snapshot.connections.confirmed_links.len(), 1);
        assert_eq!(snapshot.status.storage_exhaustions, 8);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn ephemeral_input_and_runtime_actions_are_not_replayed() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        let creation = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "track".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        let first = MemoryEndpoint::default();
        control.attach(Box::new(first), 1, 0, 2).unwrap();
        deliver(&control, 1, 1, Event::Ack);
        deliver(&control, 1, 2, Event::Ack);
        backend
            .inject_midi_input(
                creation.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, 60, 100],
                }],
            )
            .unwrap();
        backend
            .transition_loop(creation.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .grab_loops(&[BackendGrabRequest {
                loop_id: creation.loops[0],
                reverse_start_cycle: None,
                cycles_length: None,
                go_to_cycle: None,
                go_to_mode: BackendLoopMode::Playing,
            }])
            .unwrap();

        control.detach(false);
        let restarted = MemoryEndpoint::default();
        let replayed = restarted.sent.clone();
        control.attach(Box::new(restarted), 2, 0, 2).unwrap();
        let commands = replayed
            .borrow()
            .iter()
            .map(|message| {
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(commands.len(), 2);
        assert!(matches!(
            commands[0],
            Command::ConfigureDeviceChannels { .. }
        ));
        assert!(matches!(commands[1], Command::CreateTrack { .. }));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejected_structural_reservation_is_removed_from_resources_and_replay() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let creation = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "rejected".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        deliver(&control, 1, 1, Event::Ack);
        deliver(
            &control,
            1,
            2,
            Event::Error {
                message: "track rejected".to_owned(),
            },
        );
        let snapshot = backend.poll().unwrap();
        assert_eq!(
            snapshot.mutation_failures[0].detail,
            Some(BackendMutationDetail::TrackCreation)
        );
        assert!(!backend.track_resources.contains_key(&creation.track_id));

        control.detach(false);
        let restarted = MemoryEndpoint::default();
        let replayed = restarted.sent.clone();
        control.attach(Box::new(restarted), 2, 0, 2).unwrap();
        let commands = replayed
            .borrow()
            .iter()
            .map(|message| {
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(
            commands,
            vec![Command::ConfigureDeviceChannels {
                input_channels: 0,
                output_channels: 2,
            }]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn delayed_command_rejection_is_typed_correlated_and_does_not_fail_polling() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let creation = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "track".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        control.set_driver_state(BackendDriverState::Running);
        control
            .attach(Box::new(MemoryEndpoint::default()), 11, 0, 2)
            .unwrap();
        deliver(&control, 11, 1, Event::Ack);
        deliver(&control, 11, 2, Event::Ack);
        backend
            .transport
            .borrow_mut()
            .ephemeral(Command::Poll)
            .unwrap();
        deliver(
            &control,
            11,
            3,
            Event::Snapshot(WireSnapshot {
                loops: vec![shoop_audio_protocol::WireLoopState {
                    id: creation.loops[0].raw(),
                    gain: 1.0,
                    ..Default::default()
                }],
                ..Default::default()
            }),
        );
        assert_eq!(backend.poll().unwrap().loops[&creation.loops[0]].gain, 1.0);
        backend.set_loop_gain(creation.loops[0], 0.25).unwrap();
        assert_eq!(backend.poll().unwrap().loops[&creation.loops[0]].gain, 1.0);
        deliver(
            &control,
            11,
            4,
            Event::Error {
                message: "gain rejected".to_owned(),
            },
        );

        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.mutation_failures.len(), 1);
        let failure = &snapshot.mutation_failures[0];
        assert_eq!(failure.driver_generation, 11);
        assert_eq!(failure.sequence, 4);
        assert_eq!(failure.operation_key, None);
        assert_eq!(failure.kind, BackendMutationKind::LoopControl);
        assert_eq!(failure.entity, Some(creation.loops[0].raw()));
        assert_eq!(failure.message, "gain rejected");
        assert!(backend.poll().unwrap().mutation_failures.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn remote_clients_keep_sequences_resources_and_failures_isolated() {
        let (mut first, first_control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        let (mut second, second_control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        first.midi_revision = 0;
        second.midi_revision = 0;
        let first_endpoint = MemoryEndpoint::default();
        let second_endpoint = MemoryEndpoint::default();
        let first_sent = first_endpoint.sent.clone();
        let second_sent = second_endpoint.sent.clone();
        first_control
            .attach(Box::new(first_endpoint), 1, 0, 2)
            .unwrap();
        second_control
            .attach(Box::new(second_endpoint), 1, 0, 2)
            .unwrap();
        deliver(&first_control, 1, 1, Event::Ack);
        deliver(&second_control, 1, 1, Event::Ack);

        let request = TrackRequest {
            port_name_base: "isolated".to_owned(),
            topology: BackendTrackTopology::Direct {
                audio_channels: 0,
                midi: false,
            },
            initial_loops: 1,
        };
        let first_creation = first.create_track(request.clone()).unwrap();
        let second_creation = second.create_track(request).unwrap();
        assert_eq!(first_creation.track_id.raw(), 1);
        assert_eq!(second_creation.track_id.raw(), 1);
        let sequence = |message: &String| {
            serde_json::from_str::<CommandEnvelope>(message)
                .unwrap()
                .sequence
        };
        assert_eq!(sequence(first_sent.borrow().last().unwrap()), 2);
        assert_eq!(sequence(second_sent.borrow().last().unwrap()), 2);

        deliver(
            &first_control,
            1,
            2,
            Event::Error {
                message: "first rejected".to_owned(),
            },
        );
        deliver(&second_control, 1, 2, Event::Ack);
        assert_eq!(first.poll().unwrap().mutation_failures.len(), 1);
        assert!(second.poll().unwrap().mutation_failures.is_empty());
        assert!(first.track_resources.is_empty());
        assert_eq!(second.track_resources.len(), 1);

        first_control.detach(false);
        assert!(second_control.is_quiescent());
        assert_eq!(second.track_resources.len(), 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn transfer_progress_and_rejection_are_typed_and_release_retained_state() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 5, 0, 2)
            .unwrap();
        deliver(&control, 5, 1, Event::Ack);
        let BackendAsyncResult::Pending(progress) = backend.capture_session_async().unwrap() else {
            panic!("session capture completed before a remote response");
        };
        assert_eq!(progress.key, 1);
        assert_eq!(progress.kind, BackendOperationKind::SessionCapture);
        assert_eq!(progress.completed, 0);
        assert_eq!(progress.total, None);
        deliver(
            &control,
            5,
            2,
            Event::Error {
                message: "capture rejected".to_owned(),
            },
        );
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.mutation_failures[0].operation_key, Some(1));
        assert_eq!(
            snapshot.mutation_failures[0].kind,
            BackendMutationKind::SessionTransfer
        );
        assert!(backend.capture_session_async().is_err());
        assert!(backend.is_quiescent());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn driver_restart_cancels_active_transfer_and_releases_staged_bytes() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        deliver(&control, 1, 1, Event::Ack);
        assert!(matches!(
            backend.capture_session_async().unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        assert!(backend.session_capture.is_some());

        control
            .attach(Box::new(MemoryEndpoint::default()), 2, 0, 2)
            .unwrap();
        backend.poll().unwrap();
        assert!(backend.session_capture.is_none());
        assert!(backend.capture_session_async().is_err());

        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 3, 0, 2)
            .unwrap();
        deliver(&control, 3, 1, Event::Ack);
        let session = BackendSessionData {
            sample_rate: 48_000,
            tracks: Vec::new(),
            global_ports: Vec::new(),
            use_legacy_browser_default_routes: false,
        };
        assert!(matches!(
            backend.replace_session_async(&session).unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        assert!(!backend.session_replace.as_ref().unwrap().bytes.is_empty());
        control
            .attach(Box::new(MemoryEndpoint::default()), 4, 0, 2)
            .unwrap();
        backend.poll().unwrap();
        assert!(backend.session_replace.is_none());
        assert!(backend.replace_session_async(&session).is_err());

        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 5, 0, 2)
            .unwrap();
        deliver(&control, 5, 1, Event::Ack);
        let creation = backend
            .create_track(TrackRequest {
                port_name_base: "restart-transfer".to_owned(),
                topology: BackendTrackTopology::Direct {
                    audio_channels: 0,
                    midi: false,
                },
                initial_loops: 1,
            })
            .unwrap();
        deliver(&control, 5, 2, Event::Ack);
        let loop_update = BackendLoopContentUpdate {
            audio: vec![shoop_backend::BackendAudioChannelUpdate {
                channel: 0,
                samples: vec![0.25; 128],
                start_offset: None,
                preplay: None,
            }],
            length: Some(128),
            ..Default::default()
        };
        assert!(matches!(
            backend
                .replace_loop_content_async(creation.loops[0], &loop_update)
                .unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        assert!(!backend
            .loop_content_replace
            .as_ref()
            .unwrap()
            .bytes
            .is_empty());
        control
            .attach(Box::new(MemoryEndpoint::default()), 6, 0, 2)
            .unwrap();
        backend.poll().unwrap();
        assert!(backend.loop_content_replace.is_none());
        assert!(backend
            .replace_loop_content_async(creation.loops[0], &loop_update)
            .is_err());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn negotiated_transport_polls_to_observe_the_engine_before_ready() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.set_driver_state(BackendDriverState::Running);
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        deliver(&control, 1, 1, Event::Ack);
        assert_eq!(
            backend.poll().unwrap().status.driver_state,
            BackendDriverState::Starting
        );
        sent.borrow_mut().clear();

        backend.advance(Duration::from_millis(50));
        backend.poll().unwrap();
        let commands = sent
            .borrow()
            .iter()
            .map(|message| {
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert!(matches!(commands.first(), Some(Command::Poll)));
        assert!(matches!(
            commands.get(1),
            Some(Command::DrainMidiOutput { .. })
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn logical_elapsed_time_drives_polling_and_quiescence() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.set_driver_state(BackendDriverState::Running);
        control.attach(Box::new(endpoint), 2, 0, 2).unwrap();
        deliver(&control, 2, 1, Event::Ack);
        backend
            .transport
            .borrow_mut()
            .ephemeral(Command::Poll)
            .unwrap();
        deliver(&control, 2, 2, Event::Snapshot(WireSnapshot::default()));
        backend.poll().unwrap();
        sent.borrow_mut().clear();

        backend.advance(Duration::from_millis(49));
        backend.poll().unwrap();
        assert!(sent.borrow().is_empty());
        assert!(backend.is_quiescent());

        backend.advance(Duration::from_millis(1));
        backend.poll().unwrap();
        let commands = sent
            .borrow()
            .iter()
            .map(|message| {
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command
            })
            .collect::<Vec<_>>();
        assert_eq!(
            commands,
            vec![
                Command::Poll,
                Command::DrainMidiOutput {
                    max_events: MIDI_BATCH_CAPACITY,
                },
            ]
        );
        assert!(!backend.is_quiescent());

        deliver(&control, 2, 3, Event::Snapshot(WireSnapshot::default()));
        deliver(
            &control,
            2,
            4,
            Event::MidiOutput {
                events: Vec::new(),
                dropped: 0,
                refused_input: 0,
            },
        );
        backend.poll().unwrap();
        assert!(backend.is_quiescent());
    }
}
