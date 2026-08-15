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
    FxLifecycle, ResolvedAudioDriverConfig, TinySynthFxState, TrackFxState,
    TrackProcessorDescriptor, TrackProcessorEditorState,
};
use shoop_audio_protocol::{
    Command, Event, MidiDataChunk, WaveformChunk, WireApplicationPortOwner, WireChannelMode,
    WireCompositeConfig, WireCompositeEntry, WireCompositeKind, WireCompositeTarget,
    WireGrabRequest, WireHostPort, WireLoopMode, WireMidiEvent, WirePortDataType,
    WirePortDirection, WirePortRole, WireSnapshot, WireTinySynthFxMidiCcAssignment,
    WireTinySynthFxParameter, WireTrackControl, WireTrackFxControl, WireTrackTopology,
    COMMAND_CAPACITY, MIDI_BATCH_CAPACITY, MIDI_DETAIL_CHUNK_EVENTS, SESSION_TRANSFER_CHUNK_BYTES,
    SESSION_TRANSFER_MAX_BYTES, STATUS_INTERVAL_MS, WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    default_tiny_synth_fx_state, encode_tiny_synth_fx_state, tiny_synth_fx_descriptor, Backend,
    BackendActiveCompositeChild, BackendChannelMode, BackendCompositeConfig, BackendCompositeId,
    BackendCompositeKind, BackendCompositeState, BackendCompositeTarget, BackendConfirmedLink,
    BackendConnectionFailure, BackendDriverState, BackendGrabRequest, BackendHostPortDescriptor,
    BackendLoopContentUpdate, BackendLoopId, BackendLoopMode, BackendLoopState,
    BackendMidiChannelData, BackendMidiData, BackendMidiEvent, BackendPortDataType,
    BackendPortDescriptor, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendSessionData, BackendSessionReplacement, BackendSnapshot, BackendStatus,
    BackendTrackControl, BackendTrackCreation, BackendTrackFxControl, BackendTrackId,
    BackendTrackState, BackendTrackTopology, DirectTrackRequest, TinySynthFxControl,
    TinySynthFxMidiCcAssignment, TinySynthFxParameter, TrackProcessorTypeId, TrackRequest,
};

use crate::transport::{transport_pair, TransportCore};

struct WaveformAssembly {
    revision: u64,
    channels: Vec<Vec<f32>>,
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
    loops: Vec<BackendLoopId>,
    ports: Vec<BackendPortId>,
}

pub struct RemoteWorkletBackend {
    transport: Rc<RefCell<TransportCore>>,
    snapshot: BackendSnapshot,
    track_resources: BTreeMap<BackendTrackId, BrowserTrackResources>,
    removed_tracks: BTreeSet<BackendTrackId>,
    removed_loops: BTreeSet<BackendLoopId>,
    removed_ports: BTreeSet<BackendPortId>,
    next_track_id: u64,
    next_loop_id: u64,
    next_composite_id: u64,
    next_port_id: u64,
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
                removed_tracks: BTreeSet::new(),
                removed_loops: BTreeSet::new(),
                removed_ports: BTreeSet::new(),
                next_track_id: 1,
                next_loop_id: 1,
                next_composite_id: 1,
                next_port_id: 1,
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
                midi: Box::new(midi),
                midi_revision: u64::MAX,
            },
            control,
        )
    }

    fn submit(&mut self, command: Command) -> Result<()> {
        self.transport.borrow_mut().journal(command)
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
        }
        if let Some(channel) = assembly.channels.get_mut(chunk.channel) {
            channel.extend_from_slice(&chunk.samples);
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
        self.removed_tracks.clear();
        self.removed_loops.clear();
        self.removed_ports.clear();
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
                    loops: created.loops.clone(),
                    ports: created.ports.iter().map(|port| port.id).collect(),
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
            .filter(|port| {
                !self
                    .removed_ports
                    .contains(&BackendPortId::from_raw(port.id))
            })
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
            .filter(|link| {
                !self
                    .removed_ports
                    .contains(&BackendPortId::from_raw(link.application_port_id))
            })
            .map(|link| BackendConfirmedLink {
                application_port_id: BackendPortId::from_raw(link.application_port_id),
                host_port_id: link.host_port_id,
            })
            .collect();
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.snapshot.tracks.extend(
            wire.tracks
                .into_iter()
                .filter(|track| {
                    !self
                        .removed_tracks
                        .contains(&BackendTrackId::from_raw(track.id))
                })
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
                                WireTrackTopology::TinySynthFx { audio_channels } => {
                                    BackendTrackTopology::DryWetProcessor {
                                        processor_type: TrackProcessorTypeId::TINY_SYNTH_FX
                                            .to_owned(),
                                        dry_audio_channels: audio_channels,
                                        wet_audio_channels: audio_channels,
                                        dry_midi: true,
                                    }
                                }
                            },
                            fx: track.fx.map(|fx| TrackFxState {
                                processor_type: TrackProcessorTypeId::new(
                                    TrackProcessorTypeId::TINY_SYNTH_FX,
                                ),
                                active: fx.active,
                                visible: fx.visible,
                                lifecycle: FxLifecycle::Running,
                                generation: 0,
                                crash_summary: None,
                                logs: Arc::from([]),
                                editor: Some(TrackProcessorEditorState::TinySynthFx(
                                    TinySynthFxState {
                                        selected_preset_id: fx.tiny.selected_preset_id,
                                        master_gain_db: fx.tiny.master_gain_db,
                                        reverb_enabled: fx.tiny.reverb_enabled,
                                        reverb_amount: fx.tiny.reverb_amount,
                                        distortion_enabled: fx.tiny.distortion_enabled,
                                        distortion_drive: fx.tiny.distortion_drive,
                                        compressor_enabled: fx.tiny.compressor_enabled,
                                        compressor_amount: fx.tiny.compressor_amount,
                                        eq_enabled: fx.tiny.eq_enabled,
                                        eq_low_db: fx.tiny.eq_low_db,
                                        eq_mid_db: fx.tiny.eq_mid_db,
                                        eq_high_db: fx.tiny.eq_high_db,
                                        midi_cc_assignments: fx
                                            .tiny
                                            .midi_cc_assignments
                                            .into_iter()
                                            .map(|assignment| TinySynthFxMidiCcAssignment {
                                                parameter: from_wire_tiny_parameter(
                                                    assignment.parameter,
                                                ),
                                                channel: assignment.channel,
                                                controller: assignment.controller,
                                            })
                                            .collect::<Vec<_>>()
                                            .into(),
                                    },
                                )),
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
                            latest_input_midi_message: track.latest_input_midi_message.map(
                                |message| shoop_backend::BackendLatestMidiMessage {
                                    bytes: message.bytes,
                                    len: message.len,
                                },
                            ),
                            ..Default::default()
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        );
        self.snapshot.loops.extend(
            wire.loops
                .into_iter()
                .filter(|loop_| {
                    !self
                        .removed_loops
                        .contains(&BackendLoopId::from_raw(loop_.id))
                })
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
                        },
                    )
                })
                .collect::<BTreeMap<_, _>>(),
        );
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

fn browser_tiny_port_descriptors(
    base: &str,
    audio_channels: u32,
    next_port_id: &mut u64,
) -> Vec<BackendPortDescriptor> {
    let mut ports = Vec::with_capacity(audio_channels as usize * 2 + 1);
    let mut add = |name: String,
                   data_type: BackendPortDataType,
                   direction: BackendPortDirection,
                   role: BackendPortRole| {
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
        add(
            format!("{base}_audio_dry_in_{}", index + 1),
            BackendPortDataType::Audio,
            BackendPortDirection::Input,
            BackendPortRole::AudioInput,
        );
        add(
            format!("{base}_audio_wet_out_{}", index + 1),
            BackendPortDataType::Audio,
            BackendPortDirection::Output,
            BackendPortRole::AudioOutput,
        );
    }
    add(
        format!("{base}_dry_midi_in"),
        BackendPortDataType::Midi,
        BackendPortDirection::Input,
        BackendPortRole::MidiInput,
    );
    ports
}

impl Backend for RemoteWorkletBackend {
    fn supports_composite_loops(&self) -> bool {
        true
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        Ok(vec![tiny_synth_fx_descriptor()].into())
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
                dry_audio_channels,
                wet_audio_channels,
                dry_midi,
            } if processor_type == TrackProcessorTypeId::TINY_SYNTH_FX
                && dry_audio_channels == wet_audio_channels
                && *dry_midi =>
            {
                let track_id = BackendTrackId::from_raw(self.next_track_id);
                let ports = browser_tiny_port_descriptors(
                    &request.port_name_base,
                    *dry_audio_channels,
                    &mut self.next_port_id,
                );
                let loops: Vec<_> = (0..request.initial_loops)
                    .map(|offset| BackendLoopId::from_raw(self.next_loop_id + offset as u64))
                    .collect();
                self.submit(Command::CreateTrack {
                    expected_track_id: track_id.raw(),
                    expected_loop_ids: loops.iter().map(|id| id.raw()).collect(),
                    port_name_base: request.port_name_base,
                    topology: WireTrackTopology::TinySynthFx {
                        audio_channels: *dry_audio_channels,
                    },
                })?;
                self.next_track_id = self.next_track_id.saturating_add(1);
                self.next_loop_id = self.next_loop_id.saturating_add(loops.len() as u64);
                self.snapshot.tracks.insert(
                    track_id,
                    BackendTrackState {
                        topology: request.topology.clone(),
                        fx: Some(default_tiny_synth_fx_state()),
                        audio_channels: *wet_audio_channels,
                        midi: true,
                        input_peaks: vec![-200.0; *dry_audio_channels as usize],
                        output_peaks: vec![-200.0; *wet_audio_channels as usize],
                        ..Default::default()
                    },
                );
                for port in &ports {
                    self.snapshot
                        .connections
                        .application_ports
                        .insert(port.id, port.clone());
                }
                self.snapshot.connections.revision =
                    self.snapshot.connections.revision.wrapping_add(1);
                self.track_resources.insert(
                    track_id,
                    BrowserTrackResources {
                        loops: loops.clone(),
                        ports: ports.iter().map(|port| port.id).collect(),
                    },
                );
                for loop_id in &loops {
                    self.snapshot.loops.insert(
                        *loop_id,
                        BackendLoopState {
                            mode: BackendLoopMode::Stopped,
                            stereo: *wet_audio_channels == 2,
                            gain: 1.0,
                            audio_peaks: vec![
                                -200.0;
                                dry_audio_channels.saturating_add(*wet_audio_channels)
                                    as usize
                            ],
                            ..Default::default()
                        },
                    );
                }
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
        self.snapshot
            .composites
            .insert(id, BackendCompositeState::default());
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
        self.submit(Command::TransitionComposite {
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
        self.snapshot.composites.remove(&composite_id);
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
        self.snapshot.tracks.insert(
            track_id,
            BackendTrackState {
                audio_channels: request.audio_channels,
                midi: request.midi,
                input_peaks: vec![-200.0; request.audio_channels as usize],
                output_peaks: vec![-200.0; request.audio_channels as usize],
                ..Default::default()
            },
        );
        for port in &ports {
            self.snapshot
                .connections
                .application_ports
                .insert(port.id, port.clone());
        }
        self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        self.track_resources.insert(
            track_id,
            BrowserTrackResources {
                loops: loops.clone(),
                ports: ports.iter().map(|port| port.id).collect(),
            },
        );
        for loop_id in &loops {
            self.snapshot.loops.insert(
                *loop_id,
                BackendLoopState {
                    mode: BackendLoopMode::Stopped,
                    stereo: request.audio_channels == 2,
                    gain: 1.0,
                    audio_peaks: vec![-200.0; request.audio_channels as usize],
                    ..Default::default()
                },
            );
        }
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports,
        })
    }

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        if !self.snapshot.tracks.contains_key(&track_id) {
            return Ok(());
        }
        self.submit(Command::RemoveTrack {
            track_id: track_id.raw(),
        })?;
        self.snapshot.tracks.remove(&track_id);
        self.removed_tracks.insert(track_id);
        if let Some(resources) = self.track_resources.remove(&track_id) {
            for loop_id in resources.loops {
                self.removed_loops.insert(loop_id);
                self.snapshot.loops.remove(&loop_id);
                self.waveform_revisions.remove(&loop_id);
                self.waveforms.remove(&loop_id);
                self.midi_data_generations.remove(&loop_id);
                self.midi_data.remove(&loop_id);
            }
            for port_id in resources.ports {
                self.removed_ports.insert(port_id);
                self.snapshot.connections.application_ports.remove(&port_id);
                self.snapshot
                    .connections
                    .confirmed_links
                    .retain(|link| link.application_port_id != port_id);
            }
            self.snapshot.connections.revision = self.snapshot.connections.revision.wrapping_add(1);
        }
        Ok(())
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        if !self.snapshot.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown browser backend track {track_id:?}"));
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
        let track = &self.snapshot.tracks[&track_id];
        self.snapshot.loops.insert(
            loop_id,
            BackendLoopState {
                mode: BackendLoopMode::Stopped,
                stereo: track.audio_channels == 2,
                gain: 1.0,
                audio_peaks: vec![-200.0; track.audio_channels as usize],
                ..Default::default()
            },
        );
        Ok(loop_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        if !self.snapshot.tracks.contains_key(&track_id) {
            return Err(anyhow!("unknown browser backend track {track_id:?}"));
        }
        self.submit(Command::SetTrackControl {
            track_id: track_id.raw(),
            control: to_wire_track_control(control),
        })?;
        let track = self
            .snapshot
            .tracks
            .get_mut(&track_id)
            .expect("track checked");
        match control {
            BackendTrackControl::OutputGainDb(value) => track.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => track.output_balance = value,
            BackendTrackControl::OutputMute(value) => track.output_muted = value,
            BackendTrackControl::InputGainDb(value) => track.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => track.input_balance = value,
            BackendTrackControl::InputMonitoring(value) => track.input_monitoring = value,
        }
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
        if let BackendTrackFxControl::TinySynthFx(tiny) = &control {
            if !matches!(
                fx.editor.as_ref(),
                Some(TrackProcessorEditorState::TinySynthFx(_))
            ) {
                return Err(anyhow!("track has no Tiny Synth/FX editor state"));
            }
            match tiny {
                TinySynthFxControl::SelectPreset(id)
                    if !matches!(
                        tiny_synth_fx_descriptor().editor,
                        Some(shoop_app_api::TrackProcessorEditorDescriptor::TinySynthFx {
                            presets
                        }) if presets.iter().any(|preset| preset.id == *id)
                    ) =>
                {
                    return Err(anyhow!("unknown Tiny Synth/FX preset {id}"));
                }
                TinySynthFxControl::SetMasterGainDb(value)
                    if !value.is_finite()
                        || !(shoop_app_api::MIN_TINY_SYNTH_FX_GAIN_DB
                            ..=shoop_app_api::MAX_TINY_SYNTH_FX_GAIN_DB)
                            .contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX master gain"));
                }
                TinySynthFxControl::SetReverbAmount(value)
                    if !value.is_finite() || !(0.0..=1.0).contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX reverb amount"));
                }
                TinySynthFxControl::SetDistortionDrive(value)
                    if !value.is_finite() || !(1.0..=20.0).contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX distortion drive"));
                }
                TinySynthFxControl::SetCompressorAmount(value)
                    if !value.is_finite() || !(0.0..=1.0).contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX compressor amount"));
                }
                TinySynthFxControl::AssignMidiCc(assignment)
                    if assignment.channel > 15 || assignment.controller > 127 =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX MIDI CC assignment"));
                }
                TinySynthFxControl::SetEqLowDb(value)
                | TinySynthFxControl::SetEqMidDb(value)
                | TinySynthFxControl::SetEqHighDb(value)
                    if !value.is_finite()
                        || !(shoop_app_api::MIN_TINY_SYNTH_FX_EQ_GAIN_DB
                            ..=shoop_app_api::MAX_TINY_SYNTH_FX_EQ_GAIN_DB)
                            .contains(value) =>
                {
                    return Err(anyhow!("invalid Tiny Synth/FX EQ gain"));
                }
                _ => {}
            }
        }
        self.submit(Command::SetTrackFxControl {
            track_id: track_id.raw(),
            control: to_wire_track_fx_control(control.clone()),
        })?;

        let fx = self
            .snapshot
            .tracks
            .get_mut(&track_id)
            .expect("track checked")
            .fx
            .as_mut()
            .expect("processor checked");
        match control {
            BackendTrackFxControl::SetActive(value) => fx.active = value,
            BackendTrackFxControl::SetVisible(value) => fx.visible = value,
            BackendTrackFxControl::ToggleOrRecover => fx.visible = !fx.visible,
            BackendTrackFxControl::RestoreState(_) | BackendTrackFxControl::ClearLogs => {}
            BackendTrackFxControl::TinySynthFx(tiny) => {
                let Some(TrackProcessorEditorState::TinySynthFx(editor)) = fx.editor.as_mut()
                else {
                    unreachable!("Tiny Synth/FX editor was checked before submission");
                };
                match tiny {
                    TinySynthFxControl::SelectPreset(value) => {
                        editor.selected_preset_id = Some(value)
                    }
                    TinySynthFxControl::SetMasterGainDb(value) => editor.master_gain_db = value,
                    TinySynthFxControl::SetReverbEnabled(value) => editor.reverb_enabled = value,
                    TinySynthFxControl::SetReverbAmount(value) => editor.reverb_amount = value,
                    TinySynthFxControl::SetDistortionEnabled(value) => {
                        editor.distortion_enabled = value
                    }
                    TinySynthFxControl::SetDistortionDrive(value) => {
                        editor.distortion_drive = value
                    }
                    TinySynthFxControl::SetCompressorEnabled(value) => {
                        editor.compressor_enabled = value
                    }
                    TinySynthFxControl::SetCompressorAmount(value) => {
                        editor.compressor_amount = value
                    }
                    TinySynthFxControl::SetEqEnabled(value) => editor.eq_enabled = value,
                    TinySynthFxControl::SetEqLowDb(value) => editor.eq_low_db = value,
                    TinySynthFxControl::SetEqMidDb(value) => editor.eq_mid_db = value,
                    TinySynthFxControl::SetEqHighDb(value) => editor.eq_high_db = value,
                    TinySynthFxControl::AssignMidiCc(assignment) => {
                        let mut assignments = editor.midi_cc_assignments.to_vec();
                        assignments.retain(|current| {
                            current.parameter != assignment.parameter
                                && (current.channel, current.controller)
                                    != (assignment.channel, assignment.controller)
                        });
                        assignments.push(assignment);
                        assignments.sort_by_key(|assignment| assignment.parameter);
                        editor.midi_cc_assignments = assignments.into();
                    }
                    TinySynthFxControl::RemoveMidiCc(parameter) => {
                        let mut assignments = editor.midi_cc_assignments.to_vec();
                        assignments.retain(|assignment| assignment.parameter != parameter);
                        editor.midi_cc_assignments = assignments.into();
                    }
                    TinySynthFxControl::ClearMidiCcAssignments => {
                        editor.midi_cc_assignments = Arc::from([]);
                    }
                    TinySynthFxControl::Panic => {}
                }
            }
        }
        Ok(())
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
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) = &fx.editor else {
            return Ok(None);
        };
        let sample_rate = self.snapshot.status.sample_rate.max(1) as f32;
        Ok(Some(encode_tiny_synth_fx_state(sample_rate, editor)?))
    }

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        let track = self
            .snapshot
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown browser backend track {track_id:?}"))?;
        if !track.topology.has_midi() {
            return Err(anyhow!(
                "browser backend track has no MIDI input {track_id:?}"
            ));
        }
        if events.len() > MIDI_BATCH_CAPACITY
            || events
                .iter()
                .any(|event| event.time != 0 || event.data.is_empty() || event.data.len() > 4)
        {
            return Err(anyhow!("invalid browser MIDI input injection batch"));
        }
        self.submit(Command::InjectTrackMidiInput {
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
        if !self.snapshot.loops.contains_key(&loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopGain {
            loop_id: loop_id.raw(),
            gain,
        })?;
        self.snapshot
            .loops
            .get_mut(&loop_id)
            .expect("loop checked")
            .gain = gain;
        Ok(())
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        if !self.snapshot.loops.contains_key(&loop_id) {
            return Err(anyhow!("unknown browser backend loop {loop_id:?}"));
        }
        self.submit(Command::SetLoopBalance {
            loop_id: loop_id.raw(),
            balance,
        })?;
        self.snapshot
            .loops
            .get_mut(&loop_id)
            .expect("loop checked")
            .balance = balance.clamp(-1.0, 1.0);
        Ok(())
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        for request in requests {
            if !self.snapshot.loops.contains_key(&request.loop_id) {
                return Err(anyhow!(
                    "unknown browser backend loop {:?}",
                    request.loop_id
                ));
            }
        }
        self.submit(Command::GrabLoops {
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
        if !self.snapshot.loops.contains_key(&loop_id) {
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
                next_channel: 0,
                next_offset: 0,
                complete: false,
                in_flight: false,
            },
        );
        self.request_waveform_chunk(loop_id)?;
        Ok(None)
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        if !self.snapshot.loops.contains_key(&loop_id) {
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
        self.submit(Command::TransitionLoop {
            loop_id: loop_id.raw(),
            mode: to_wire_loop_mode(mode),
            cycles_delay,
        })?;
        self.waveforms.remove(&loop_id);
        self.midi_data.remove(&loop_id);
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.submit(Command::ClearLoop {
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
                return Ok(());
            }
            self.pump_loop_content_replace()?;
            return Err(anyhow!("loop content replacement pending"));
        }
        let bytes = serde_json::to_vec(update)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!(
                "prepared loop content exceeds browser transfer limit"
            ));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginLoopContentReplace {
                generation,
                loop_id: loop_id.raw(),
                total_bytes: bytes.len(),
            })?;
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
        Err(anyhow!("loop content replacement pending"))
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        self.submit(Command::SetLoopLength {
            loop_id: loop_id.raw(),
            length,
        })
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
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
                return Ok(session);
            }
            return Err(anyhow!("session capture pending"));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
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
        Err(anyhow!("session capture pending"))
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        if let Some(replace) = &self.session_replace {
            if &replace.session != session {
                return Err(anyhow!("another session replacement is active"));
            }
            if replace.complete {
                let replacement = browser_replacement_mapping(session);
                self.apply_replaced_session(session, &replacement);
                self.session_replace = None;
                return Ok(replacement);
            }
            self.pump_session_replace()?;
            return Err(anyhow!("session replacement pending"));
        }
        let bytes = serde_json::to_vec(session)?;
        if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
            return Err(anyhow!("prepared session exceeds browser transfer limit"));
        }
        let generation = self.next_session_generation;
        self.next_session_generation = self.next_session_generation.saturating_add(1);
        self.transport
            .borrow_mut()
            .ephemeral(Command::BeginSessionReplace {
                generation,
                total_bytes: bytes.len(),
            })?;
        self.session_replace = Some(SessionReplaceAssembly {
            generation,
            session: session.clone(),
            bytes,
            next_offset: 0,
            commit_sent: false,
            complete: false,
        });
        self.pump_session_replace()?;
        Err(anyhow!("session replacement pending"))
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
        self.sync_midi_endpoints()?;
        let state = self.transport.borrow().driver_state();
        let running = matches!(state, BackendDriverState::Running);
        self.pump_midi_input(running)?;
        self.snapshot.status.driver_state = state;
        self.snapshot.status.command_overflows = self.transport.borrow().overflows();
        if matches!(
            state,
            BackendDriverState::Running | BackendDriverState::Suspended
        ) && self.poll_elapsed >= Duration::from_millis(u64::from(STATUS_INTERVAL_MS))
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
        for envelope in events {
            match envelope.event {
                Event::Ack | Event::Stopped => {}
                Event::Error { message } => {
                    self.loop_content_replace = None;
                    return Err(anyhow!(message));
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
                    }
                    if self
                        .session_replace
                        .as_ref()
                        .is_some_and(|replace| replace.generation == generation)
                    {
                        self.session_replace = None;
                    }
                    if self
                        .loop_content_replace
                        .as_ref()
                        .is_some_and(|replace| replace.generation == generation)
                    {
                        self.loop_content_replace = None;
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

fn from_wire_tiny_parameter(parameter: WireTinySynthFxParameter) -> TinySynthFxParameter {
    match parameter {
        WireTinySynthFxParameter::MasterGain => TinySynthFxParameter::MasterGain,
        WireTinySynthFxParameter::ReverbAmount => TinySynthFxParameter::ReverbAmount,
        WireTinySynthFxParameter::DistortionDrive => TinySynthFxParameter::DistortionDrive,
        WireTinySynthFxParameter::CompressorAmount => TinySynthFxParameter::CompressorAmount,
        WireTinySynthFxParameter::EqLow => TinySynthFxParameter::EqLow,
        WireTinySynthFxParameter::EqMid => TinySynthFxParameter::EqMid,
        WireTinySynthFxParameter::EqHigh => TinySynthFxParameter::EqHigh,
    }
}

fn to_wire_tiny_parameter(parameter: TinySynthFxParameter) -> WireTinySynthFxParameter {
    match parameter {
        TinySynthFxParameter::MasterGain => WireTinySynthFxParameter::MasterGain,
        TinySynthFxParameter::ReverbAmount => WireTinySynthFxParameter::ReverbAmount,
        TinySynthFxParameter::DistortionDrive => WireTinySynthFxParameter::DistortionDrive,
        TinySynthFxParameter::CompressorAmount => WireTinySynthFxParameter::CompressorAmount,
        TinySynthFxParameter::EqLow => WireTinySynthFxParameter::EqLow,
        TinySynthFxParameter::EqMid => WireTinySynthFxParameter::EqMid,
        TinySynthFxParameter::EqHigh => WireTinySynthFxParameter::EqHigh,
    }
}

fn to_wire_track_fx_control(control: BackendTrackFxControl) -> WireTrackFxControl {
    match control {
        BackendTrackFxControl::SetActive(value) => WireTrackFxControl::SetActive(value),
        BackendTrackFxControl::SetVisible(value) => WireTrackFxControl::SetVisible(value),
        BackendTrackFxControl::ToggleOrRecover => WireTrackFxControl::ToggleOrRecover,
        BackendTrackFxControl::RestoreState(value) => WireTrackFxControl::RestoreState(value),
        BackendTrackFxControl::ClearLogs => WireTrackFxControl::ClearLogs,
        BackendTrackFxControl::TinySynthFx(control) => match control {
            TinySynthFxControl::SelectPreset(value) => WireTrackFxControl::TinySelectPreset(value),
            TinySynthFxControl::SetMasterGainDb(value) => {
                WireTrackFxControl::TinySetMasterGainDb(value)
            }
            TinySynthFxControl::SetReverbEnabled(value) => {
                WireTrackFxControl::TinySetReverbEnabled(value)
            }
            TinySynthFxControl::SetReverbAmount(value) => {
                WireTrackFxControl::TinySetReverbAmount(value)
            }
            TinySynthFxControl::SetDistortionEnabled(value) => {
                WireTrackFxControl::TinySetDistortionEnabled(value)
            }
            TinySynthFxControl::SetDistortionDrive(value) => {
                WireTrackFxControl::TinySetDistortionDrive(value)
            }
            TinySynthFxControl::SetCompressorEnabled(value) => {
                WireTrackFxControl::TinySetCompressorEnabled(value)
            }
            TinySynthFxControl::SetCompressorAmount(value) => {
                WireTrackFxControl::TinySetCompressorAmount(value)
            }
            TinySynthFxControl::SetEqEnabled(value) => WireTrackFxControl::TinySetEqEnabled(value),
            TinySynthFxControl::SetEqLowDb(value) => WireTrackFxControl::TinySetEqLowDb(value),
            TinySynthFxControl::SetEqMidDb(value) => WireTrackFxControl::TinySetEqMidDb(value),
            TinySynthFxControl::SetEqHighDb(value) => WireTrackFxControl::TinySetEqHighDb(value),
            TinySynthFxControl::AssignMidiCc(assignment) => {
                WireTrackFxControl::TinyAssignMidiCc(WireTinySynthFxMidiCcAssignment {
                    parameter: to_wire_tiny_parameter(assignment.parameter),
                    channel: assignment.channel,
                    controller: assignment.controller,
                })
            }
            TinySynthFxControl::RemoveMidiCc(parameter) => {
                WireTrackFxControl::TinyRemoveMidiCc(to_wire_tiny_parameter(parameter))
            }
            TinySynthFxControl::ClearMidiCcAssignments => {
                WireTrackFxControl::TinyClearMidiCcAssignments
            }
            TinySynthFxControl::Panic => WireTrackFxControl::TinyPanic,
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

    #[tracy_nextest_capture::tracy_capture_test]
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

        let endpoint = MemoryEndpoint::default();
        let sent = endpoint.sent.clone();
        control.set_driver_state(BackendDriverState::Running);
        control.attach(Box::new(endpoint), 9, 1, 2).unwrap();
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
}
