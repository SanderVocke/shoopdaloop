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
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{anyhow, Result};
use shoop_app_api::{
    AudioDriverConfig, AudioDriverDescriptor, AudioDriverKind, AudioDriverRuntimeState,
    FxLifecycle, OxiSynthMidiCcAssignment, OxiSynthParameter, OxiSynthState,
    ResolvedAudioDriverConfig, TrackFxState, TrackProcessorDescriptor, TrackProcessorEditorState,
};
use shoop_audio_protocol::{
    decode_binary, encode_binary, Command, Event, MidiDataChunk, WaveformChunk,
    WireApplicationPortOwner, WireBusControl, WireChannelMode, WireCompositeConfig,
    WireCompositeEntry, WireCompositeKind, WireCompositeTarget, WireGrabRequest, WireHostPort,
    WireLoopMode, WireMidiEvent, WireOxiSynthMidiCcAssignment, WireOxiSynthParameter,
    WirePortDataType, WirePortDirection, WirePortRole, WireProcessorLatencyAdjustment,
    WireRecordingOffsetAdjustment, WireSnapshot, WireTrackControl, WireTrackFxControl,
    WireTrackTopology, COMMAND_CAPACITY, MIDI_BATCH_CAPACITY, MIDI_DETAIL_CHUNK_EVENTS,
    SESSION_TRANSFER_CHUNK_BYTES, SESSION_TRANSFER_MAX_BYTES, STATUS_INTERVAL_MS,
    WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    encode_oxisynth_state, oxisynth_descriptor, Backend, BackendActiveCompositeChild,
    BackendAsyncResult, BackendAudioChannelData, BackendAudioData, BackendBusChannelId,
    BackendBusChannelState, BackendBusControl, BackendBusId, BackendBusState, BackendChannelMode,
    BackendCompositeConfig, BackendCompositeId, BackendCompositeKind, BackendCompositeState,
    BackendCompositeTarget, BackendConfirmedLink, BackendConnectionFailure, BackendDriverState,
    BackendGrabRequest, BackendHostPortDescriptor, BackendLoopContentUpdate, BackendLoopId,
    BackendLoopMode, BackendLoopState, BackendMidiChannelData, BackendMidiData, BackendMidiEvent,
    BackendMixerFailure, BackendMixerLink, BackendMutationDetail, BackendMutationFailure,
    BackendMutationKind, BackendOperationKind, BackendOperationProgress, BackendPortDataType,
    BackendPortDescriptor, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendSessionData, BackendSessionReplacement, BackendSnapshot, BackendStatus,
    BackendTrackControl, BackendTrackCreation, BackendTrackFxControl, BackendTrackId,
    BackendTrackState, BackendTrackTopology, DirectTrackRequest, OxiSynthControl,
    TrackProcessorTypeId, TrackRequest, GLOBAL_FX_PORT_ID, MASTER_BUS_CHANNEL_IDS,
    MASTER_BUS_CHANNEL_LABELS, MASTER_BUS_ID, MASTER_BUS_OUTPUT_PORT_IDS,
};

use crate::transport::{transport_pair, TransportCore};

struct WaveformAssembly {
    revision: u64,
    channels: Vec<Vec<f32>>,
    timing: Vec<(i32, i32, u32)>,
    request_channel: usize,
    request_offset: usize,
    channel_total: Option<usize>,
    expected: VecDeque<(usize, usize)>,
    complete: bool,
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
const WAVEFORM_IN_FLIGHT_LIMIT: usize = 8;
const WAVEFORM_PENDING_COMMAND_LIMIT: usize = COMMAND_CAPACITY / 2;

struct SessionCaptureAssembly {
    generation: u64,
    total_bytes: Option<usize>,
    bytes: Vec<u8>,
    next_offset: usize,
    in_flight: usize,
}

struct SessionReplaceAssembly {
    generation: u64,
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
    acknowledged_composite_removals: BTreeSet<BackendCompositeId>,
    next_track_id: u64,
    next_loop_id: u64,
    next_composite_id: u64,
    next_composite_plan_version: u64,
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

fn replacement_connection_journal(
    session: &BackendSessionData,
    replacement: &BackendSessionReplacement,
) -> Vec<Command> {
    let mut commands = Vec::new();
    for source_track in &session.tracks {
        for source_port in &source_track.ports {
            let Some(&application_port_id) = replacement.ports.get(&source_port.source_id) else {
                continue;
            };
            for host_port_id in &source_port.external_connections {
                commands.push(Command::SetPortConnected {
                    application_port_id: application_port_id.raw(),
                    host_port_id: host_port_id.clone(),
                    connected: true,
                });
            }
        }
    }
    for source_port in &session.global_ports {
        let Some(&application_port_id) = replacement.global_ports.get(&source_port.source_id)
        else {
            continue;
        };
        for host_port_id in &source_port.external_connections {
            commands.push(Command::SetPortConnected {
                application_port_id: application_port_id.raw(),
                host_port_id: host_port_id.clone(),
                connected: true,
            });
        }
    }
    for source_bus in &session.buses {
        let Some(&bus_id) = replacement.buses.get(&source_bus.source_id) else {
            continue;
        };
        for source_channel in &source_bus.channels {
            let Some(&application_port_id) =
                replacement.bus_output_ports.get(&source_channel.source_id)
            else {
                continue;
            };
            for host_port_id in &source_channel.output_port.external_connections {
                commands.push(Command::SetPortConnected {
                    application_port_id: application_port_id.raw(),
                    host_port_id: host_port_id.clone(),
                    connected: true,
                });
            }
        }
        for control in [
            BackendBusControl::GainDb(source_bus.gain_db),
            BackendBusControl::Balance(source_bus.balance),
            BackendBusControl::Mute(source_bus.muted),
        ] {
            commands.push(Command::SetBusControl {
                bus_id: bus_id.raw(),
                control: to_wire_bus_control(control),
            });
        }
    }
    for route in &session.mixer_routes {
        let (Some(&source_port_id), Some(&destination_channel_id)) = (
            replacement.ports.get(&route.source_port_id),
            replacement.bus_channels.get(&route.destination_channel_id),
        ) else {
            continue;
        };
        commands.push(Command::SetMixerRoute {
            source_port_id: source_port_id.raw(),
            destination_channel_id: destination_channel_id.raw(),
            connected: true,
        });
    }
    commands
}

impl RemoteWorkletBackend {
    pub fn new(midi: impl HostMidiBridge + 'static) -> (Self, RemoteBackendControl) {
        let (transport, control) = transport_pair();
        control.set_driver_state(BackendDriverState::AwaitingGesture);
        let mut snapshot = BackendSnapshot {
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
        };
        let channels = MASTER_BUS_CHANNEL_IDS
            .into_iter()
            .zip(MASTER_BUS_OUTPUT_PORT_IDS)
            .zip(MASTER_BUS_CHANNEL_LABELS)
            .enumerate()
            .map(|(index, ((id, output_port_id), label))| {
                snapshot.connections.application_ports.insert(
                    output_port_id,
                    BackendPortDescriptor {
                        id: output_port_id,
                        owner: BackendPortOwner::Bus(MASTER_BUS_ID),
                        name: format!("master_out_{}", index + 1),
                        data_type: BackendPortDataType::Audio,
                        direction: BackendPortDirection::Output,
                        role: BackendPortRole::AudioOutput,
                    },
                );
                BackendBusChannelState {
                    id,
                    label: label.to_owned(),
                    output_port_id,
                }
            })
            .collect::<Vec<_>>();
        snapshot.connections.revision = 1;
        snapshot.mixer.revision = 1;
        snapshot.mixer.buses.insert(
            MASTER_BUS_ID,
            BackendBusState {
                id: MASTER_BUS_ID,
                name: "Master".to_owned(),
                output_peaks_db: vec![-200.0; channels.len()],
                channels,
                gain_db: 0.0,
                balance: 0.0,
                muted: false,
            },
        );
        (
            Self {
                transport: transport.clone(),
                snapshot,
                track_resources: BTreeMap::new(),
                pending_removed_tracks: BTreeMap::new(),
                reserved_composites: BTreeSet::new(),
                acknowledged_composite_removals: BTreeSet::new(),
                next_track_id: 1,
                next_loop_id: 1,
                next_composite_id: 1,
                next_composite_plan_version: 1,
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
                .all(|assembly| assembly.complete && assembly.expected.is_empty())
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
        self.transport
            .borrow_mut()
            .cancel_reserved_session_connection_journal();
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
        while !assembly.complete
            && assembly.expected.len() < WAVEFORM_IN_FLIGHT_LIMIT
            && self.transport.borrow().pending_len() < WAVEFORM_PENDING_COMMAND_LIMIT
        {
            if let Some(total) = assembly.channel_total {
                if assembly.request_offset >= total {
                    break;
                }
            } else if !assembly.expected.is_empty() {
                break;
            }
            let request = (assembly.request_channel, assembly.request_offset);
            self.transport
                .borrow_mut()
                .ephemeral(Command::RequestWaveform {
                    loop_id: loop_id.raw(),
                    revision: assembly.revision,
                    channel: request.0,
                    offset: request.1,
                    max_samples: WAVEFORM_CHUNK_SAMPLES,
                })?;
            assembly.expected.push_back(request);
            assembly.request_offset = assembly
                .request_offset
                .saturating_add(WAVEFORM_CHUNK_SAMPLES);
        }
        Ok(())
    }

    fn apply_waveform_chunk(&mut self, chunk: WaveformChunk) -> Result<()> {
        let loop_id = BackendLoopId::from_raw(chunk.loop_id);
        let Some(assembly) = self.waveforms.get_mut(&loop_id) else {
            return Ok(());
        };
        if assembly.revision != chunk.revision
            || assembly.expected.front().copied() != Some((chunk.channel, chunk.offset))
        {
            return Ok(());
        }
        assembly.expected.pop_front();
        assembly.channel_total = Some(chunk.total_samples);
        if assembly.channels.len() < chunk.channel_count {
            assembly.channels.resize_with(chunk.channel_count, Vec::new);
            assembly.timing.resize(chunk.channel_count, (0, 0, 0));
        }
        if let Some(channel) = assembly.channels.get_mut(chunk.channel) {
            channel.extend_from_slice(&chunk.samples);
        }
        if let Some(timing) = assembly.timing.get_mut(chunk.channel) {
            *timing = (
                chunk.start_offset,
                chunk.capture_alignment_frames,
                chunk.preplay,
            );
        }
        if chunk.final_chunk {
            assembly.request_channel += 1;
            assembly.request_offset = 0;
            assembly.channel_total = None;
            assembly.complete = assembly.request_channel >= chunk.channel_count;
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
                capture_alignment_frames: chunk.capture_alignment_frames,
                preplay: chunk.preplay,
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
            || channel.capture_alignment_frames != chunk.capture_alignment_frames
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
        self.transport
            .borrow_mut()
            .commit_reserved_session_connection_journal();

        self.snapshot.tracks.clear();
        self.snapshot.loops.clear();
        self.snapshot.composites.clear();
        self.next_composite_id = 1;
        self.next_composite_plan_version = 1;
        self.snapshot.connections.application_ports.clear();
        self.snapshot.connections.confirmed_links.clear();
        self.snapshot.mixer.buses.clear();
        self.snapshot.mixer.confirmed_links.clear();
        self.snapshot.mixer.failures.clear();
        self.track_resources.clear();
        self.pending_removed_tracks.clear();
        self.reserved_composites.clear();
        self.acknowledged_composite_removals.clear();
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
        for source_global in &session.global_ports {
            let Some(&global_port_id) = replacement.global_ports.get(&source_global.source_id)
            else {
                continue;
            };
            let mut descriptor = source_global.descriptor.clone();
            descriptor.id = global_port_id;
            descriptor.owner = BackendPortOwner::GlobalFxControl;
            self.snapshot
                .connections
                .application_ports
                .insert(global_port_id, descriptor);
        }
        for source_bus in &session.buses {
            let Some(&bus_id) = replacement.buses.get(&source_bus.source_id) else {
                continue;
            };
            let mut channels = Vec::with_capacity(source_bus.channels.len());
            for source_channel in &source_bus.channels {
                let (Some(&channel_id), Some(&output_port_id)) = (
                    replacement.bus_channels.get(&source_channel.source_id),
                    replacement.bus_output_ports.get(&source_channel.source_id),
                ) else {
                    continue;
                };
                let mut output_descriptor = source_channel.output_port.descriptor.clone();
                output_descriptor.id = output_port_id;
                output_descriptor.owner = BackendPortOwner::Bus(bus_id);
                self.snapshot
                    .connections
                    .application_ports
                    .insert(output_port_id, output_descriptor);
                channels.push(BackendBusChannelState {
                    id: channel_id,
                    label: source_channel.label.clone(),
                    output_port_id,
                });
            }
            self.snapshot.mixer.buses.insert(
                bus_id,
                BackendBusState {
                    id: bus_id,
                    name: source_bus.name.clone(),
                    output_peaks_db: vec![-200.0; channels.len()],
                    channels,
                    gain_db: source_bus.gain_db,
                    balance: source_bus.balance,
                    muted: source_bus.muted,
                },
            );
        }
        self.snapshot.mixer.confirmed_links = session
            .mixer_routes
            .iter()
            .filter_map(|route| {
                Some(BackendMixerLink {
                    source_port_id: *replacement.ports.get(&route.source_port_id)?,
                    destination_channel_id: *replacement
                        .bus_channels
                        .get(&route.destination_channel_id)?,
                })
            })
            .collect();
        self.snapshot.mixer.revision = self.snapshot.mixer.revision.wrapping_add(1);
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
            .tracks
            .values()
            .flat_map(|created| &created.ports)
            .map(|port| port.id.raw())
            .max()
            .unwrap_or(0)
            .saturating_add(1);
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
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
                            WireApplicationPortOwner::Bus { bus_id } => {
                                BackendPortOwner::Bus(BackendBusId::from_raw(bus_id))
                            }
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
        self.snapshot
            .connections
            .failures
            .extend(
                wire.connection_failures
                    .into_iter()
                    .map(|failure| BackendConnectionFailure {
                        port_id: BackendPortId::from_raw(failure.application_port_id),
                        external_port: failure.host_port_id,
                        desired_connected: failure.desired_connected,
                        message: failure.message,
                    }),
            );
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.snapshot.mixer.buses = wire
            .buses
            .into_iter()
            .map(|bus| {
                let id = BackendBusId::from_raw(bus.id);
                let channels = bus
                    .channels
                    .into_iter()
                    .map(|channel| BackendBusChannelState {
                        id: BackendBusChannelId::from_raw(channel.id),
                        label: channel.label,
                        output_port_id: BackendPortId::from_raw(channel.output_port_id),
                    })
                    .collect::<Vec<_>>();
                (
                    id,
                    BackendBusState {
                        id,
                        name: bus.name,
                        channels,
                        gain_db: bus.gain_db,
                        balance: bus.balance,
                        muted: bus.muted,
                        output_peaks_db: bus.output_peaks_db,
                    },
                )
            })
            .collect();
        self.snapshot.mixer.confirmed_links = wire
            .confirmed_mixer_links
            .into_iter()
            .map(|link| BackendMixerLink {
                source_port_id: BackendPortId::from_raw(link.source_port_id),
                destination_channel_id: BackendBusChannelId::from_raw(link.destination_channel_id),
            })
            .collect();
        self.snapshot
            .mixer
            .failures
            .extend(
                wire.mixer_failures
                    .into_iter()
                    .map(|failure| BackendMixerFailure {
                        link: BackendMixerLink {
                            source_port_id: BackendPortId::from_raw(failure.link.source_port_id),
                            destination_channel_id: BackendBusChannelId::from_raw(
                                failure.link.destination_channel_id,
                            ),
                        },
                        desired_connected: failure.desired_connected,
                        message: failure.message,
                    }),
            );
        self.snapshot.mixer.revision = self.snapshot.mixer.revision.wrapping_add(1);
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
                        latency: shoop_backend::BackendTrackLatencyState {
                            automatic_offset_frames: track.latency.automatic_offset_frames,
                            adjustment: match track.latency.adjustment {
                                WireRecordingOffsetAdjustment::Automatic => {
                                    shoop_backend::BackendRecordingOffsetAdjustment::Automatic
                                }
                                WireRecordingOffsetAdjustment::ManualOverride => {
                                    shoop_backend::BackendRecordingOffsetAdjustment::ManualOverride(
                                        track.latency.manual_frames,
                                    )
                                }
                                WireRecordingOffsetAdjustment::AutomaticPlusTrim => {
                                    shoop_backend::BackendRecordingOffsetAdjustment::AutomaticPlusTrim(
                                        track.latency.manual_frames,
                                    )
                                }
                            },
                            effective_offset_frames: track.latency.effective_offset_frames,
                            automatic_processor_advance_frames: track
                                .latency
                                .automatic_processor_advance_frames,
                            processor_adjustment: match track.latency.processor_adjustment {
                                WireProcessorLatencyAdjustment::Automatic => {
                                    shoop_backend::BackendProcessorLatencyAdjustment::Automatic
                                }
                                WireProcessorLatencyAdjustment::ManualOverride => {
                                    shoop_backend::BackendProcessorLatencyAdjustment::ManualOverride
                                }
                                WireProcessorLatencyAdjustment::AutomaticPlusTrim => {
                                    shoop_backend::BackendProcessorLatencyAdjustment::AutomaticPlusTrim
                                }
                            },
                            processor_manual_frames: track.latency.processor_manual_frames,
                            effective_processor_advance_frames: track
                                .latency
                                .effective_processor_advance_frames,
                            pending: track.latency.pending,
                            error: track.latency.error,
                        },
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
                        length: loop_.length,
                        position: loop_.position,
                        next_mode: loop_.next_mode.map(from_wire_loop_mode),
                        next_transition_delay: loop_.next_transition_delay,
                        stereo: loop_.stereo,
                        gain: loop_.gain,
                        balance: loop_.balance,
                        audio_peaks: loop_.audio_peaks,
                        midi_activity: loop_.midi_activity,
                        capture_alignment_frames: loop_.capture_alignment_frames,
                        processor_alignment_frames: loop_.processor_alignment_frames,
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
            .insert(global.source_id, GLOBAL_FX_PORT_ID);
    }
    for source_bus in &session.buses {
        replacement
            .buses
            .insert(source_bus.source_id, MASTER_BUS_ID);
        for (index, source_channel) in source_bus.channels.iter().enumerate() {
            let Some((&channel_id, &output_port_id)) = MASTER_BUS_CHANNEL_IDS
                .get(index)
                .zip(MASTER_BUS_OUTPUT_PORT_IDS.get(index))
            else {
                continue;
            };
            replacement
                .bus_channels
                .insert(source_channel.source_id, channel_id);
            replacement
                .bus_output_ports
                .insert(source_channel.source_id, output_port_id);
            replacement
                .ports
                .insert(source_channel.output_port.source_id, output_port_id);
        }
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
        Command::SetLoopSmoothingMs { .. } => (BackendMutationKind::AudioProcessing, None),
        Command::ConfigureDeviceChannels { .. } | Command::ConfigureMidiEndpoints { .. } => {
            (BackendMutationKind::DriverConfiguration, None)
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
            ..
        } => (
            BackendMutationKind::CompositeStructure,
            Some(*expected_composite_id),
        ),
        Command::SetTrackControl { track_id, .. } | Command::SetTrackLatency { track_id, .. } => {
            (BackendMutationKind::TrackControl, Some(*track_id))
        }
        Command::SetBusControl { bus_id, .. } => (BackendMutationKind::BusControl, Some(*bus_id)),
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
        | Command::SetLoopTiming { loop_id, .. }
        | Command::SetTakeAlignment { loop_id, .. }
        | Command::SetTakeProcessorAlignment { loop_id, .. } => {
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
        Command::SetMixerRoute { source_port_id, .. } => {
            (BackendMutationKind::MixerRoute, Some(*source_port_id))
        }
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
        Command::ConfigureComposite { plan_version, .. } => {
            Some(BackendMutationDetail::CompositeConfiguration {
                plan_version: *plan_version,
            })
        }
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
        Command::SetBusControl { control, .. } => {
            Some(BackendMutationDetail::BusControl(match control {
                WireBusControl::GainDb(value) => BackendBusControl::GainDb(*value),
                WireBusControl::Balance(value) => BackendBusControl::Balance(*value),
                WireBusControl::Mute(value) => BackendBusControl::Mute(*value),
            }))
        }
        Command::SetTrackFxControl { control, .. } => Some(BackendMutationDetail::TrackFxControl(
            from_wire_track_fx_control(control),
        )),
        Command::SetLoopGain { gain, .. } => Some(BackendMutationDetail::LoopGain(*gain)),
        Command::SetLoopBalance { balance, .. } => {
            Some(BackendMutationDetail::LoopBalance(*balance))
        }
        Command::SetLoopLength { .. } | Command::SetLoopTiming { .. } => {
            Some(BackendMutationDetail::LoopTiming)
        }
        Command::SetTakeAlignment { .. } => Some(BackendMutationDetail::TakeAlignment),
        Command::SetTakeProcessorAlignment { .. } => {
            Some(BackendMutationDetail::TakeProcessorAlignment)
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
    fn set_loop_smoothing_ms(&mut self, milliseconds: u32) -> Result<()> {
        self.submit(Command::SetLoopSmoothingMs { milliseconds })
    }

    fn supports_composite_loops(&self) -> bool {
        true
    }

    fn composite_plan_mutations_are_synchronous(&self) -> bool {
        false
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
    ) -> Result<u64> {
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
        let version = self.next_composite_plan_version;
        self.submit(Command::ConfigureComposite {
            composite_id: composite_id.raw(),
            plan_version: version,
            config: wire,
        })?;
        self.next_composite_plan_version = self.next_composite_plan_version.saturating_add(1);
        Ok(version)
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

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<Option<u64>> {
        let plan_version = self
            .reserved_composites
            .contains(&composite_id)
            .then_some(self.next_composite_plan_version);
        self.submit(Command::RemoveComposite {
            composite_id: composite_id.raw(),
            plan_version,
        })?;
        if self.reserved_composites.remove(&composite_id) {
            self.next_composite_plan_version = self.next_composite_plan_version.saturating_add(1);
        }
        Ok(plan_version)
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

    fn set_track_latency(
        &mut self,
        track_id: BackendTrackId,
        adjustment: shoop_backend::BackendRecordingOffsetAdjustment,
        processor_adjustment: shoop_backend::BackendProcessorLatencyAdjustment,
        processor_manual_frames: i32,
    ) -> Result<()> {
        if !self.track_resources.contains_key(&track_id) {
            return Err(anyhow!("unknown remote backend track {track_id:?}"));
        }
        let (adjustment, manual_frames) = match adjustment {
            shoop_backend::BackendRecordingOffsetAdjustment::Automatic => {
                (WireRecordingOffsetAdjustment::Automatic, 0)
            }
            shoop_backend::BackendRecordingOffsetAdjustment::ManualOverride(frames) => {
                (WireRecordingOffsetAdjustment::ManualOverride, frames)
            }
            shoop_backend::BackendRecordingOffsetAdjustment::AutomaticPlusTrim(frames) => {
                (WireRecordingOffsetAdjustment::AutomaticPlusTrim, frames)
            }
        };
        let processor_adjustment = match processor_adjustment {
            shoop_backend::BackendProcessorLatencyAdjustment::Automatic => {
                WireProcessorLatencyAdjustment::Automatic
            }
            shoop_backend::BackendProcessorLatencyAdjustment::ManualOverride => {
                WireProcessorLatencyAdjustment::ManualOverride
            }
            shoop_backend::BackendProcessorLatencyAdjustment::AutomaticPlusTrim => {
                WireProcessorLatencyAdjustment::AutomaticPlusTrim
            }
        };
        self.submit(Command::SetTrackLatency {
            track_id: track_id.raw(),
            adjustment,
            manual_frames,
            processor_adjustment,
            processor_manual_frames,
        })
    }

    fn set_take_alignment(
        &mut self,
        loop_id: BackendLoopId,
        capture_alignment_frames: i32,
    ) -> Result<()> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown remote backend loop {loop_id:?}"));
        }
        self.submit(Command::SetTakeAlignment {
            loop_id: loop_id.raw(),
            capture_alignment_frames,
        })?;
        self.waveforms.remove(&loop_id);
        self.midi_data.remove(&loop_id);
        Ok(())
    }

    fn set_take_processor_alignment(
        &mut self,
        loop_id: BackendLoopId,
        processor_alignment_frames: u32,
    ) -> Result<()> {
        if !self.has_loop(loop_id) {
            return Err(anyhow!("unknown remote backend loop {loop_id:?}"));
        }
        self.submit(Command::SetTakeProcessorAlignment {
            loop_id: loop_id.raw(),
            processor_alignment_frames,
        })?;
        self.waveforms.remove(&loop_id);
        self.midi_data.remove(&loop_id);
        Ok(())
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
            self.request_waveform_chunk(loop_id)?;
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
                request_channel: 0,
                request_offset: 0,
                channel_total: None,
                expected: VecDeque::new(),
                complete: false,
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
                    let (start_offset, capture_alignment_frames, preplay) =
                        timing.get(index).copied().unwrap_or_default();
                    BackendAudioChannelData {
                        samples,
                        start_offset,
                        capture_alignment_frames,
                        preplay,
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
            if assembly.complete {
                return Ok(Some(BackendMidiData {
                    channels: assembly.channels.clone(),
                }));
            }
            self.request_midi_data_chunk(loop_id)?;
            return Ok(None);
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
                    timing.2 = samples;
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
                let session: BackendSessionData = decode_binary(&capture.bytes)
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
        let bytes = encode_binary(session)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("prepared session exceeds browser transfer limit"));
        }
        let replacement = browser_replacement_mapping(session);
        self.transport
            .borrow_mut()
            .reserve_session_connection_journal(replacement_connection_journal(
                session,
                &replacement,
            ))?;
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport_generation = self.transport.borrow().diagnostics().generation;
        if let Err(error) = self
            .transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionReplace {
                generation,
                total_bytes: bytes.len(),
            })
        {
            self.transport
                .borrow_mut()
                .cancel_reserved_session_connection_journal();
            return Err(error);
        }
        let total = bytes.len();
        self.session_replace = Some(SessionReplaceAssembly {
            generation,
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

    fn set_mixer_route(
        &mut self,
        source_port_id: BackendPortId,
        destination_channel_id: BackendBusChannelId,
        connected: bool,
    ) -> Result<()> {
        let source = self
            .snapshot
            .connections
            .application_ports
            .get(&source_port_id)
            .ok_or_else(|| anyhow!("unknown browser mixer source {source_port_id:?}"))?;
        if source.owner != BackendPortOwner::Track
            || source.data_type != BackendPortDataType::Audio
            || source.direction != BackendPortDirection::Output
            || source.role != BackendPortRole::AudioOutput
        {
            return Err(anyhow!("browser mixer source is not a track audio output"));
        }
        if !self.snapshot.mixer.buses.values().any(|bus| {
            bus.channels
                .iter()
                .any(|channel| channel.id == destination_channel_id)
        }) {
            return Err(anyhow!(
                "unknown browser mixer destination {destination_channel_id:?}"
            ));
        }
        self.submit(Command::SetMixerRoute {
            source_port_id: source_port_id.raw(),
            destination_channel_id: destination_channel_id.raw(),
            connected,
        })
    }

    fn set_bus_control(&mut self, bus_id: BackendBusId, control: BackendBusControl) -> Result<()> {
        let bus = self
            .snapshot
            .mixer
            .buses
            .get(&bus_id)
            .ok_or_else(|| anyhow!("unknown browser bus {bus_id:?}"))?;
        let control = control.normalized(bus.channels.len())?;
        self.submit(Command::SetBusControl {
            bus_id: bus_id.raw(),
            control: to_wire_bus_control(control),
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
        } else if matches!(
            connection,
            ConnectionState::Detached | ConnectionState::Failed
        ) && (self.session_capture.is_some()
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
                Event::Ack => {
                    if let Command::RemoveComposite { composite_id, .. } = received.command {
                        self.acknowledged_composite_removals
                            .insert(BackendCompositeId::from_raw(composite_id));
                    }
                }
                Event::Stopped => {}
                Event::Error { message } => {
                    let retry_media_read = match &received.command {
                        Command::RequestWaveform {
                            loop_id,
                            revision,
                            channel,
                            offset,
                            ..
                        } if message.contains("postroll is still finalizing") => {
                            if let Some(assembly) =
                                self.waveforms.get_mut(&BackendLoopId::from_raw(*loop_id))
                            {
                                if assembly.revision == *revision
                                    && assembly.expected.front().copied()
                                        == Some((*channel, *offset))
                                {
                                    assembly.expected.clear();
                                    assembly.request_channel = *channel;
                                    assembly.request_offset = *offset;
                                }
                            }
                            true
                        }
                        Command::RequestMidiData { loop_id, .. }
                            if message == "MIDI detail data is not ready" =>
                        {
                            if let Some(assembly) =
                                self.midi_data.get_mut(&BackendLoopId::from_raw(*loop_id))
                            {
                                assembly.in_flight = false;
                            }
                            true
                        }
                        _ => false,
                    };
                    if retry_media_read {
                        continue;
                    }
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
                        Command::RemoveComposite { composite_id, .. } => {
                            self.reserved_composites
                                .insert(BackendCompositeId::from_raw(*composite_id));
                        }
                        _ => {}
                    }
                    match &received.command {
                        Command::SetLoopLength { loop_id, .. }
                        | Command::SetLoopTiming { loop_id, .. } => {
                            let loop_id = BackendLoopId::from_raw(*loop_id);
                            self.waveforms.remove(&loop_id);
                            self.midi_data.remove(&loop_id);
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
                                self.transport
                                    .borrow_mut()
                                    .cancel_reserved_session_connection_journal();
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
                } => {
                    self.transport
                        .borrow_mut()
                        .reject_journaled(&received.command);
                    self.snapshot
                        .connections
                        .failures
                        .push(BackendConnectionFailure {
                            port_id: BackendPortId::from_raw(application_port_id),
                            external_port: host_port_id,
                            desired_connected,
                            message,
                        });
                }
                Event::MixerMutationFailed {
                    source_port_id,
                    destination_channel_id,
                    desired_connected,
                    message,
                } => {
                    self.transport
                        .borrow_mut()
                        .reject_journaled(&received.command);
                    self.snapshot.mixer.failures.push(BackendMixerFailure {
                        link: BackendMixerLink {
                            source_port_id: BackendPortId::from_raw(source_port_id),
                            destination_channel_id: BackendBusChannelId::from_raw(
                                destination_channel_id,
                            ),
                        },
                        desired_connected,
                        message,
                    });
                }
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
                Event::Snapshot(snapshot) => {
                    self.apply_wire_snapshot(snapshot);
                    self.snapshot
                        .removed_composites
                        .extend(std::mem::take(&mut self.acknowledged_composite_removals));
                }
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
                        self.transport
                            .borrow_mut()
                            .cancel_reserved_session_connection_journal();
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
        self.snapshot.connections.failures.clear();
        self.snapshot.mixer.failures.clear();
        self.snapshot.mutation_failures.clear();
        self.snapshot.removed_composites.clear();
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

fn to_wire_bus_control(control: BackendBusControl) -> WireBusControl {
    match control {
        BackendBusControl::GainDb(value) => WireBusControl::GainDb(value),
        BackendBusControl::Balance(value) => WireBusControl::Balance(value),
        BackendBusControl::Mute(value) => WireBusControl::Mute(value),
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
    fn remote_loop_smoothing_control_is_durable_and_globally_classified() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.set_loop_smoothing_ms(0).unwrap();
        backend.set_loop_smoothing_ms(19).unwrap();
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 3, 0, 2).unwrap();
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
            commands
                .iter()
                .filter(|command| matches!(command, Command::SetLoopSmoothingMs { .. }))
                .collect::<Vec<_>>(),
            vec![&Command::SetLoopSmoothingMs { milliseconds: 19 }]
        );
        assert_eq!(
            command_mutation_identity(&Command::SetLoopSmoothingMs { milliseconds: 19 }),
            Some((BackendMutationKind::AudioProcessing, None))
        );
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
            Event::Error {
                message: "loop alignment postroll is still finalizing; retry after it settles"
                    .to_owned(),
            },
        );
        backend.poll().unwrap();
        assert!(backend.waveforms[&loop_id].expected.is_empty());
        assert!(backend
            .loop_audio_data_with_metadata(loop_id)
            .unwrap()
            .is_none());
        assert_eq!(backend.waveforms[&loop_id].expected.front(), Some(&(0, 0)));
        deliver(
            &control,
            1,
            4,
            Event::Waveform(WaveformChunk {
                loop_id: loop_id.raw(),
                revision: 1,
                channel: 0,
                channel_count: 1,
                offset: 0,
                total_samples: 3,
                start_offset: -4,
                capture_alignment_frames: 3,
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
        assert_eq!(audio.channels[0].capture_alignment_frames, 3);
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
                    capture_alignment_frames: 0,
                    preplay: 6,
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
        deliver(&control, 1, 5, Event::Ack);
        deliver(&control, 1, 6, Event::Ack);
        deliver(
            &control,
            1,
            7,
            Event::Error {
                message: "timeline exceeds retained window".to_owned(),
            },
        );
        let rejected = backend.poll().unwrap();
        assert_eq!(
            rejected.mutation_failures[0].detail,
            Some(BackendMutationDetail::LoopTiming)
        );
        assert!(!backend.waveforms.contains_key(&loop_id));
        assert!(!backend.midi_data.contains_key(&loop_id));

        backend.set_take_alignment(loop_id, 5).unwrap();
        assert!(!backend.waveforms.contains_key(&loop_id));
        assert!(!backend.midi_data.contains_key(&loop_id));
        assert!(sent.borrow().iter().any(|message| {
            matches!(
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command,
                Command::SetTakeAlignment {
                    loop_id: raw,
                    capture_alignment_frames: 5,
                } if raw == loop_id.raw()
            )
        }));
        backend.set_take_processor_alignment(loop_id, 7).unwrap();
        assert!(sent.borrow().iter().any(|message| {
            matches!(
                serde_json::from_str::<CommandEnvelope>(message)
                    .unwrap()
                    .command,
                Command::SetTakeProcessorAlignment {
                    loop_id: raw,
                    processor_alignment_frames: 7,
                } if raw == loop_id.raw()
            )
        }));
        deliver(&control, 1, 8, Event::Ack);
        deliver(
            &control,
            1,
            9,
            Event::Error {
                message: "processor alignment exceeds retained window".to_owned(),
            },
        );
        let rejected = backend.poll().unwrap();
        assert_eq!(
            rejected.mutation_failures.last().unwrap().detail,
            Some(BackendMutationDetail::TakeProcessorAlignment)
        );

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
            2
        );
        assert!(!commands.iter().any(|command| matches!(
            command,
            Command::SetLoopTiming {
                length: Some(32),
                ..
            }
        )));
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
            audio_channels: 2,
            midi: true,
            output_gain_db: -3.0,
            output_balance: 0.25,
            output_muted: true,
            input_gain_db: -4.0,
            input_balance: -0.25,
            input_monitoring: true,
            latency: Default::default(),
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
                        length: 256,
                        position: 64,
                        next_mode: Some(WireLoopMode::Playing),
                        next_transition_delay: Some(2),
                        stereo: true,
                        gain: 0.75,
                        balance: -0.1,
                        audio_peaks: vec![0.2, 0.3],
                        midi_activity: true,
                        capture_alignment_frames: 0,
                        processor_alignment_frames: None,
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
                connection_failures: vec![shoop_audio_protocol::WireConnectionFailure {
                    application_port_id: 1,
                    host_port_id: "missing-output".to_owned(),
                    desired_connected: true,
                    message: "unavailable".to_owned(),
                }],
                buses: vec![shoop_audio_protocol::WireBus {
                    id: 1,
                    name: "Master".to_owned(),
                    channels: vec![shoop_audio_protocol::WireBusChannel {
                        id: 1,
                        label: "Left".to_owned(),
                        output_port_id: 2,
                    }],
                    gain_db: -3.0,
                    balance: 0.0,
                    muted: false,
                    output_peaks_db: vec![-12.0],
                }],
                confirmed_mixer_links: vec![shoop_audio_protocol::WireMixerLink {
                    source_port_id: 2,
                    destination_channel_id: 1,
                }],
                mixer_failures: Vec::new(),
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
        assert_eq!(snapshot.connections.failures.len(), 1);
        assert!(backend.poll().unwrap().connections.failures.is_empty());
        assert_eq!(snapshot.mixer.buses.len(), 1);
        let bus = &snapshot.mixer.buses[&BackendBusId::from_raw(1)];
        assert_eq!(bus.gain_db, -3.0);
        assert_eq!(bus.balance, 0.0);
        assert!(!bus.muted);
        assert_eq!(bus.output_peaks_db, [-12.0]);
        assert_eq!(snapshot.mixer.confirmed_links.len(), 1);
        backend
            .set_bus_control(BackendBusId::from_raw(1), BackendBusControl::GainDb(-6.0))
            .unwrap();
        assert!(backend
            .set_bus_control(BackendBusId::from_raw(1), BackendBusControl::Balance(0.25),)
            .is_err());
        backend
            .set_mixer_route(
                BackendPortId::from_raw(2),
                BackendBusChannelId::from_raw(1),
                false,
            )
            .unwrap();
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
                buses: vec![shoop_audio_protocol::WireBus {
                    id: MASTER_BUS_ID.raw(),
                    name: "Master".to_owned(),
                    channels: MASTER_BUS_CHANNEL_IDS
                        .into_iter()
                        .zip(MASTER_BUS_OUTPUT_PORT_IDS)
                        .map(|(channel, output)| shoop_audio_protocol::WireBusChannel {
                            id: channel.raw(),
                            label: channel.raw().to_string(),
                            output_port_id: output.raw(),
                        })
                        .collect(),
                    gain_db: 0.0,
                    balance: 0.0,
                    muted: false,
                    output_peaks_db: vec![-200.0; 2],
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

        backend
            .set_bus_control(MASTER_BUS_ID, BackendBusControl::GainDb(-3.0))
            .unwrap();
        deliver(
            &control,
            11,
            5,
            Event::Error {
                message: "bus gain rejected".to_owned(),
            },
        );
        let rejected = backend.poll().unwrap();
        let failure = &rejected.mutation_failures[0];
        assert_eq!(failure.kind, BackendMutationKind::BusControl);
        assert_eq!(failure.entity, Some(MASTER_BUS_ID.raw()));
        assert_eq!(
            failure.detail,
            Some(BackendMutationDetail::BusControl(
                BackendBusControl::GainDb(-3.0)
            ))
        );
        assert_eq!(failure.message, "bus gain rejected");
        assert!(!backend
            .transport
            .borrow()
            .journal_commands()
            .contains(&Command::SetBusControl {
                bus_id: MASTER_BUS_ID.raw(),
                control: WireBusControl::GainDb(-3.0),
            }));
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
            buses: Vec::new(),
            mixer_routes: Vec::new(),
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
                capture_alignment_frames: None,
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
    fn saturated_replacement_journal_fails_before_session_transfer_begins() {
        let (mut backend, _) = RemoteWorkletBackend::new(NullHostMidiBridge);
        for loop_id in 0..COMMAND_CAPACITY as u64 {
            backend
                .transport
                .borrow_mut()
                .journal(Command::SetLoopGain { loop_id, gain: 0.5 })
                .unwrap();
        }
        let session = BackendSessionData {
            sample_rate: 48_000,
            tracks: Vec::new(),
            buses: vec![shoop_backend::BackendSessionBus {
                source_id: 41,
                name: "Master".to_owned(),
                channels: Vec::new(),
                gain_db: 0.0,
                balance: 0.0,
                muted: false,
            }],
            mixer_routes: Vec::new(),
            global_ports: Vec::new(),
            use_legacy_browser_default_routes: false,
        };

        let error = backend.replace_session_async(&session).unwrap_err();
        assert!(error
            .to_string()
            .contains("replacement connection command journal is full"));
        assert!(backend.session_replace.is_none());
        assert!(!backend
            .transport
            .borrow()
            .has_reserved_session_connections());
        assert_eq!(backend.transport.borrow().pending_len(), 0);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn fixed_bus_mappings_do_not_advance_the_next_track_port_identity() {
        let session = BackendSessionData {
            sample_rate: 48_000,
            tracks: Vec::new(),
            buses: vec![shoop_backend::BackendSessionBus {
                source_id: 41,
                name: "Master".to_owned(),
                channels: vec![shoop_backend::BackendSessionBusChannel {
                    source_id: 42,
                    label: "Left".to_owned(),
                    output_port: shoop_backend::BackendSessionPort {
                        source_id: 43,
                        descriptor: BackendPortDescriptor {
                            id: BackendPortId::from_raw(43),
                            owner: BackendPortOwner::Bus(BackendBusId::from_raw(41)),
                            name: "master_out_1".to_owned(),
                            data_type: BackendPortDataType::Audio,
                            direction: BackendPortDirection::Output,
                            role: BackendPortRole::AudioOutput,
                        },
                        external_connections: vec!["system:playback_1".to_owned()],
                    },
                }],
                gain_db: -3.0,
                balance: 0.0,
                muted: true,
            }],
            mixer_routes: Vec::new(),
            global_ports: vec![shoop_backend::BackendSessionPort {
                source_id: 44,
                descriptor: BackendPortDescriptor {
                    id: BackendPortId::from_raw(44),
                    owner: BackendPortOwner::GlobalFxControl,
                    name: "Global FX Control MIDI In".to_owned(),
                    data_type: BackendPortDataType::Midi,
                    direction: BackendPortDirection::Input,
                    role: BackendPortRole::MidiInput,
                },
                external_connections: Vec::new(),
            }],
            use_legacy_browser_default_routes: false,
        };
        let replacement = browser_replacement_mapping(&session);
        assert_eq!(
            replacement.bus_output_ports[&42],
            MASTER_BUS_OUTPUT_PORT_IDS[0]
        );
        assert_eq!(replacement.global_ports[&44], GLOBAL_FX_PORT_ID);
        let (mut backend, _) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend
            .transport
            .borrow_mut()
            .journal(Command::SetPortConnected {
                application_port_id: MASTER_BUS_OUTPUT_PORT_IDS[0].raw(),
                host_port_id: "old:playback".to_owned(),
                connected: true,
            })
            .unwrap();
        backend
            .transport
            .borrow_mut()
            .journal(Command::SetBusControl {
                bus_id: MASTER_BUS_ID.raw(),
                control: WireBusControl::Mute(false),
            })
            .unwrap();
        backend
            .transport
            .borrow_mut()
            .journal(Command::SetMixerRoute {
                source_port_id: 99,
                destination_channel_id: 99,
                connected: true,
            })
            .unwrap();
        backend
            .snapshot
            .mixer
            .confirmed_links
            .insert(BackendMixerLink {
                source_port_id: BackendPortId::from_raw(99),
                destination_channel_id: BackendBusChannelId::from_raw(99),
            });
        backend
            .transport
            .borrow_mut()
            .reserve_session_connection_journal(replacement_connection_journal(
                &session,
                &replacement,
            ))
            .unwrap();
        backend.apply_replaced_session(&session, &replacement);
        assert_eq!(backend.next_port_id, 1);
        assert_eq!(backend.snapshot.mixer.buses.len(), 1);
        let master = &backend.snapshot.mixer.buses[&MASTER_BUS_ID];
        assert_eq!(master.gain_db, -3.0);
        assert_eq!(master.balance, 0.0);
        assert!(master.muted);
        assert_eq!(master.output_peaks_db, [-200.0]);
        assert!(backend.snapshot.mixer.confirmed_links.is_empty());
        let connection_journal = backend
            .transport
            .borrow()
            .journal_commands()
            .into_iter()
            .filter(|command| {
                matches!(
                    command,
                    Command::SetPortConnected { .. }
                        | Command::SetBusControl { .. }
                        | Command::SetMixerRoute { .. }
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(
            connection_journal,
            vec![
                Command::SetPortConnected {
                    application_port_id: MASTER_BUS_OUTPUT_PORT_IDS[0].raw(),
                    host_port_id: "system:playback_1".to_owned(),
                    connected: true,
                },
                Command::SetBusControl {
                    bus_id: MASTER_BUS_ID.raw(),
                    control: WireBusControl::GainDb(-3.0),
                },
                Command::SetBusControl {
                    bus_id: MASTER_BUS_ID.raw(),
                    control: WireBusControl::Balance(0.0),
                },
                Command::SetBusControl {
                    bus_id: MASTER_BUS_ID.raw(),
                    control: WireBusControl::Mute(true),
                },
            ]
        );
        assert_eq!(
            backend.snapshot.connections.application_ports[&MASTER_BUS_OUTPUT_PORT_IDS[0]].owner,
            BackendPortOwner::Bus(MASTER_BUS_ID)
        );
        assert_eq!(
            backend.snapshot.connections.application_ports[&GLOBAL_FX_PORT_ID].owner,
            BackendPortOwner::GlobalFxControl
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn detached_remote_snapshot_seeds_fixed_master_and_replays_early_control() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let snapshot = backend.poll().unwrap();
        let master = &snapshot.mixer.buses[&MASTER_BUS_ID];
        assert_eq!(master.name, "Master");
        assert_eq!(master.channels.len(), 2);
        assert_eq!(master.output_peaks_db, [-200.0, -200.0]);
        assert_eq!(
            (master.gain_db, master.balance, master.muted),
            (0.0, 0.0, false)
        );
        for output_port_id in MASTER_BUS_OUTPUT_PORT_IDS {
            assert_eq!(
                snapshot.connections.application_ports[&output_port_id].owner,
                BackendPortOwner::Bus(MASTER_BUS_ID)
            );
        }

        backend
            .set_bus_control(MASTER_BUS_ID, BackendBusControl::GainDb(-6.0))
            .unwrap();
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
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
                Command::ConfigureDeviceChannels {
                    input_channels: 0,
                    output_channels: 2,
                },
                Command::SetBusControl {
                    bus_id: MASTER_BUS_ID.raw(),
                    control: WireBusControl::GainDb(-6.0),
                },
            ]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn rejected_connection_commands_are_removed_from_replay() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        deliver(&control, 1, 1, Event::Ack);

        let route = Command::SetMixerRoute {
            source_port_id: 2,
            destination_channel_id: 1,
            connected: true,
        };
        backend
            .transport
            .borrow_mut()
            .journal(route.clone())
            .unwrap();
        deliver(
            &control,
            1,
            2,
            Event::MixerMutationFailed {
                source_port_id: 2,
                destination_channel_id: 1,
                desired_connected: true,
                message: "route rejected".to_owned(),
            },
        );
        assert_eq!(backend.poll().unwrap().mixer.failures.len(), 1);
        assert!(!backend
            .transport
            .borrow()
            .journal_commands()
            .contains(&route));

        let host_link = Command::SetPortConnected {
            application_port_id: MASTER_BUS_OUTPUT_PORT_IDS[0].raw(),
            host_port_id: "system:playback_1".to_owned(),
            connected: true,
        };
        backend
            .transport
            .borrow_mut()
            .journal(host_link.clone())
            .unwrap();
        deliver(
            &control,
            1,
            3,
            Event::ConnectionMutationFailed {
                application_port_id: MASTER_BUS_OUTPUT_PORT_IDS[0].raw(),
                host_port_id: "system:playback_1".to_owned(),
                desired_connected: true,
                message: "link rejected".to_owned(),
            },
        );
        assert_eq!(backend.poll().unwrap().connections.failures.len(), 1);
        assert!(!backend
            .transport
            .borrow()
            .journal_commands()
            .contains(&host_link));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn mixer_failures_are_published_once_then_drained() {
        let (mut backend, _) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.snapshot.mixer.failures.push(BackendMixerFailure {
            link: BackendMixerLink {
                source_port_id: BackendPortId::from_raw(1),
                destination_channel_id: BackendBusChannelId::from_raw(1),
            },
            desired_connected: true,
            message: "rejected".to_owned(),
        });
        assert_eq!(backend.poll().unwrap().mixer.failures.len(), 1);
        assert!(backend.poll().unwrap().mixer.failures.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn active_session_replacement_is_polled_without_reencoding_or_restarting() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.attach(Box::new(endpoint), 1, 0, 2).unwrap();
        deliver(&control, 1, 1, Event::Ack);
        sent.borrow_mut().clear();
        let session = BackendSessionData {
            sample_rate: 48_000,
            tracks: Vec::new(),
            buses: Vec::new(),
            mixer_routes: Vec::new(),
            global_ports: Vec::new(),
            use_legacy_browser_default_routes: false,
        };
        assert!(matches!(
            backend.replace_session_async(&session).unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        let staged = backend.session_replace.as_ref().unwrap().bytes.clone();
        let mut ignored_argument = session.clone();
        ignored_argument.sample_rate = 44_100;
        assert!(matches!(
            backend.replace_session_async(&ignored_argument).unwrap(),
            BackendAsyncResult::Pending(_)
        ));
        assert_eq!(backend.session_replace.as_ref().unwrap().bytes, staged);
        let begin_count = sent
            .borrow()
            .iter()
            .map(|message| serde_json::from_str::<CommandEnvelope>(message).unwrap())
            .filter(|envelope| matches!(envelope.command, Command::BeginSessionReplace { .. }))
            .count();
        assert_eq!(begin_count, 1);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn representative_waveform_uses_large_chunk_request_count() {
        let frames = 242_526_usize;
        let channels = 4;
        let requests = frames.div_ceil(WAVEFORM_CHUNK_SAMPLES) * channels;
        assert_eq!(requests, 240);
        assert_eq!(frames.div_ceil(512) * channels, 1_896);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn concurrent_waveform_pipelines_reserve_transport_headroom() {
        let (mut backend, control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        backend.midi_revision = 0;
        control
            .attach(Box::new(MemoryEndpoint::default()), 1, 0, 2)
            .unwrap();
        deliver(&control, 1, 1, Event::Ack);

        for raw_loop_id in 1..=COMMAND_CAPACITY as u64 {
            let loop_id = BackendLoopId::from_raw(raw_loop_id);
            backend.waveforms.insert(
                loop_id,
                WaveformAssembly {
                    revision: 1,
                    channels: Vec::new(),
                    timing: Vec::new(),
                    request_channel: 0,
                    request_offset: 0,
                    channel_total: Some(WAVEFORM_CHUNK_SAMPLES * WAVEFORM_IN_FLIGHT_LIMIT),
                    expected: VecDeque::new(),
                    complete: false,
                },
            );
            backend.request_waveform_chunk(loop_id).unwrap();
        }

        assert_eq!(
            backend.transport.borrow().pending_len(),
            WAVEFORM_PENDING_COMMAND_LIMIT
        );
        backend
            .transport
            .borrow_mut()
            .ephemeral(Command::Poll)
            .unwrap();
        assert_eq!(
            backend.transport.borrow().pending_len(),
            WAVEFORM_PENDING_COMMAND_LIMIT + 1
        );
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

    #[shoop_wasm_test_support::shoop_test]
    fn remote_composite_plan_versions_include_removals() {
        let (mut backend, _control) = RemoteWorkletBackend::new(NullHostMidiBridge);
        let config = BackendCompositeConfig {
            kind: BackendCompositeKind::Script,
            sync_source: BackendLoopId::from_raw(1),
            timelines: Vec::new(),
        };
        let first = backend.create_composite_loop().unwrap();
        assert_eq!(backend.configure_composite_loop(first, &config).unwrap(), 1);
        backend.remove_composite_loop(first).unwrap();
        let second = backend.create_composite_loop().unwrap();
        assert_eq!(
            backend.configure_composite_loop(second, &config).unwrap(),
            3
        );
    }
}
