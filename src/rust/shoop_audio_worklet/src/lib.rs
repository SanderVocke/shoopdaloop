#[cfg(all(test, target_arch = "wasm32", feature = "wasm-test-browser"))]
shoop_wasm_test_support::wasm_bindgen_test_configure!(run_in_browser);

use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, MidiDataChunk, WaveformChunk,
    WireActiveCompositeChild, WireApplicationPort, WireApplicationPortOwner, WireChannelMode,
    WireCompositeConfig, WireCompositeKind, WireCompositeState, WireCompositeTarget,
    WireConfirmedLink, WireHostPort, WireLatestMidiMessage, WireLoopMode, WireLoopState,
    WireMidiOutputEvent, WirePortDataType, WirePortDirection, WirePortRole, WireSnapshot,
    WireTinySynthFxMidiCcAssignment, WireTinySynthFxParameter, WireTinySynthFxState,
    WireTrackControl, WireTrackFxControl, WireTrackFxState, WireTrackState, WireTrackTopology,
    COMMAND_MAX_BYTES, MAX_DEVICE_AUDIO_CHANNELS, MIDI_BATCH_CAPACITY, MIDI_DETAIL_CHUNK_EVENTS,
    PROTOCOL_VERSION, SESSION_TRANSFER_CHUNK_BYTES, SESSION_TRANSFER_MAX_BYTES,
    TRACK_MIDI_MESSAGE_BYTES, WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    Backend, BackendCompositeConfig, BackendCompositeEntry, BackendCompositeId,
    BackendCompositeKind, BackendCompositeTarget, BackendGrabRequest, BackendHostPortDescriptor,
    BackendLoopContentUpdate, BackendLoopId, BackendLoopMode, BackendMidiEvent,
    BackendPortDataType, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendSessionData, BackendSnapshot, BackendTrackControl, BackendTrackFxControl,
    BackendTrackId, BackendTrackTopology, EngineBackend, TinySynthFxControl,
    TinySynthFxMidiCcAssignment, TinySynthFxParameter, TrackProcessorEditorState,
    TrackProcessorTypeId, TrackRequest, MAX_WEB_AUDIO_QUANTUM,
};

pub struct WorkletHost {
    backend: EngineBackend,
    max_quantum: usize,
    input: Vec<f32>,
    output: Vec<f32>,
    packed_input: Vec<f32>,
    packed_output: Vec<f32>,
    command_buffer: Vec<u8>,
    response: String,
    next_sequence: u64,
    stopped: bool,
    fatal_error: Option<String>,
    capture_generation: Option<u64>,
    capture_bytes: Vec<u8>,
    replace_generation: Option<u64>,
    replace_expected_bytes: usize,
    replace_bytes: Vec<u8>,
    loop_content_generation: Option<u64>,
    loop_content_id: Option<BackendLoopId>,
    loop_content_expected_bytes: usize,
    loop_content_bytes: Vec<u8>,
}

impl WorkletHost {
    pub fn new(sample_rate: u32, max_quantum: u32) -> Result<Self, String> {
        if max_quantum == 0 || max_quantum > MAX_WEB_AUDIO_QUANTUM {
            return Err(format!(
                "render quantum ceiling must be in 1..={MAX_WEB_AUDIO_QUANTUM}"
            ));
        }
        let max_quantum = max_quantum as usize;
        Ok(Self {
            backend: EngineBackend::new_web_audio(sample_rate, max_quantum as u32)
                .map_err(|error| error.to_string())?,
            max_quantum,
            input: vec![0.0; MAX_DEVICE_AUDIO_CHANNELS * max_quantum],
            output: vec![0.0; MAX_DEVICE_AUDIO_CHANNELS * max_quantum],
            packed_input: vec![0.0; MAX_DEVICE_AUDIO_CHANNELS * max_quantum],
            packed_output: vec![0.0; MAX_DEVICE_AUDIO_CHANNELS * max_quantum],
            command_buffer: vec![0; COMMAND_MAX_BYTES],
            response: String::with_capacity(COMMAND_MAX_BYTES * 2),
            next_sequence: 1,
            stopped: false,
            fatal_error: None,
            capture_generation: None,
            capture_bytes: Vec::new(),
            replace_generation: None,
            replace_expected_bytes: 0,
            replace_bytes: Vec::new(),
            loop_content_generation: None,
            loop_content_id: None,
            loop_content_expected_bytes: 0,
            loop_content_bytes: Vec::new(),
        })
    }

    pub fn input(&mut self) -> &mut [f32] {
        &mut self.input
    }

    pub fn output(&self) -> &[f32] {
        &self.output
    }

    pub fn process(
        &mut self,
        input_channels: usize,
        output_channels: usize,
        n_frames: usize,
    ) -> bool {
        if self.stopped
            || self.fatal_error.is_some()
            || input_channels > MAX_DEVICE_AUDIO_CHANNELS
            || output_channels > MAX_DEVICE_AUDIO_CHANNELS
            || n_frames == 0
            || n_frames > self.max_quantum
        {
            return false;
        }
        for channel in 0..input_channels {
            self.packed_input[channel * n_frames..(channel + 1) * n_frames].copy_from_slice(
                &self.input[channel * self.max_quantum..channel * self.max_quantum + n_frames],
            );
        }
        self.packed_output[..output_channels * n_frames].fill(0.0);
        if let Err(error) = self.backend.process_audio_quantum(
            &self.packed_input,
            input_channels,
            &mut self.packed_output,
            output_channels,
            n_frames,
        ) {
            self.fatal_error = Some(error.to_string());
            return false;
        }
        for channel in 0..output_channels {
            self.output[channel * self.max_quantum..channel * self.max_quantum + n_frames]
                .copy_from_slice(&self.packed_output[channel * n_frames..(channel + 1) * n_frames]);
        }
        true
    }

    pub fn handle_json(&mut self, json: &[u8]) -> &str {
        let result = self.decode_and_handle(json);
        let event = match result {
            Ok(event) => event,
            Err(message) => Event::Error { message },
        };
        let sequence = serde_json::from_slice::<CommandEnvelope>(json)
            .map(|command| command.sequence)
            .unwrap_or(0);
        self.response = serde_json::to_string(&EventEnvelope {
            version: PROTOCOL_VERSION,
            sequence,
            event,
        })
        .unwrap_or_else(|_| {
            r#"{"version":6,"sequence":0,"event":{"kind":"error","message":"response serialization failed"}}"#
                .to_owned()
        });
        &self.response
    }

    fn decode_and_handle(&mut self, json: &[u8]) -> Result<Event, String> {
        if json.len() > COMMAND_MAX_BYTES {
            return Err("command exceeds the protocol byte capacity".to_owned());
        }
        let envelope: CommandEnvelope =
            serde_json::from_slice(json).map_err(|error| format!("malformed command: {error}"))?;
        if envelope.version != PROTOCOL_VERSION {
            return Err(format!(
                "protocol version mismatch: received {}, expected {PROTOCOL_VERSION}",
                envelope.version
            ));
        }
        if envelope.sequence != self.next_sequence {
            return Err(format!(
                "out-of-order command {}, expected {}",
                envelope.sequence, self.next_sequence
            ));
        }
        self.next_sequence = self.next_sequence.saturating_add(1);
        self.handle(envelope.command)
    }

    fn handle(&mut self, command: Command) -> Result<Event, String> {
        if self.stopped && !matches!(command, Command::Poll | Command::Shutdown) {
            return Err("worklet host is stopped".to_owned());
        }
        match command {
            Command::ConfigureDeviceChannels {
                input_channels,
                output_channels,
            } => {
                self.backend
                    .configure_web_audio_channels(input_channels, output_channels)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::ConfigureMidiEndpoints { endpoints } => {
                let endpoints = endpoints
                    .into_iter()
                    .map(|endpoint| BackendHostPortDescriptor {
                        id: endpoint.id,
                        name: endpoint.name,
                        data_type: from_wire_data_type(endpoint.data_type),
                        direction: from_wire_direction(endpoint.direction),
                    })
                    .collect();
                self.backend
                    .configure_web_midi_endpoints(endpoints)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::PushMidiInput {
                host_port_id,
                events,
            } => {
                if events.len() > MIDI_BATCH_CAPACITY {
                    return Err("Web MIDI input batch exceeds capacity".to_owned());
                }
                for event in events {
                    if event.frame != 0
                        || event.data.is_empty()
                        || event.data.len() > TRACK_MIDI_MESSAGE_BYTES
                    {
                        return Err("invalid Web MIDI track input event".to_owned());
                    }
                    self.backend
                        .stage_web_midi_input(&host_port_id, &event.data)
                        .map_err(|error| error.to_string())?;
                }
                Ok(Event::Ack)
            }
            Command::InjectTrackMidiInput { track_id, events } => {
                if events.len() > MIDI_BATCH_CAPACITY {
                    return Err("piano MIDI input batch exceeds capacity".to_owned());
                }
                let events = events
                    .into_iter()
                    .map(|event| BackendMidiEvent {
                        time: event.frame,
                        data: event.data,
                    })
                    .collect::<Vec<_>>();
                self.backend
                    .inject_midi_input(BackendTrackId::from_raw(track_id), &events)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::DrainMidiOutput { max_events } => {
                let (events, dropped, refused_input) = self
                    .backend
                    .drain_web_midi_output(max_events.min(MIDI_BATCH_CAPACITY));
                Ok(Event::MidiOutput {
                    events: events
                        .into_iter()
                        .map(|event| WireMidiOutputEvent {
                            application_port_id: event.application_port_id.raw(),
                            host_port_id: event.host_port_id,
                            frame: event.frame,
                            data: event.data,
                        })
                        .collect(),
                    dropped,
                    refused_input,
                })
            }
            Command::CreateTrack {
                expected_track_id,
                expected_loop_ids,
                port_name_base,
                topology,
            } => {
                let created = self
                    .backend
                    .create_track(TrackRequest {
                        port_name_base,
                        topology: from_wire_track_topology(topology),
                        initial_loops: expected_loop_ids.len(),
                    })
                    .map_err(|error| error.to_string())?;
                let actual_loops: Vec<_> = created.loops.iter().map(|id| id.raw()).collect();
                if created.track_id.raw() != expected_track_id || actual_loops != expected_loop_ids
                {
                    return Err("stable-ID mismatch while creating a track".to_owned());
                }
                Ok(Event::Ack)
            }
            Command::RemoveTrack { track_id } => {
                self.backend
                    .remove_track(BackendTrackId::from_raw(track_id))
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::AddLoop {
                track_id,
                expected_loop_id,
            } => {
                let actual = self
                    .backend
                    .add_loop_to_track(BackendTrackId::from_raw(track_id))
                    .map_err(|error| error.to_string())?;
                if actual.raw() != expected_loop_id {
                    return Err("stable-ID mismatch while creating a loop".to_owned());
                }
                Ok(Event::Ack)
            }
            Command::CreateComposite {
                expected_composite_id,
            } => {
                let actual = self
                    .backend
                    .create_composite_loop()
                    .map_err(|error| error.to_string())?;
                if actual.raw() != expected_composite_id {
                    return Err("stable-ID mismatch while creating a composite".to_owned());
                }
                Ok(Event::Ack)
            }
            Command::ConfigureComposite {
                composite_id,
                config,
            } => {
                self.backend
                    .configure_composite_loop(
                        BackendCompositeId::from_raw(composite_id),
                        &from_wire_composite_config(config),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::TransitionComposite {
                composite_id,
                mode,
                cycles_delay,
                align_to_iteration,
            } => {
                self.backend
                    .transition_composite_loop(
                        BackendCompositeId::from_raw(composite_id),
                        from_wire_loop_mode(mode),
                        cycles_delay,
                        align_to_iteration,
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetCompositePlayAfterRecord {
                composite_id,
                enabled,
            } => {
                self.backend
                    .set_composite_play_after_record(
                        BackendCompositeId::from_raw(composite_id),
                        enabled,
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::RemoveComposite { composite_id } => {
                self.backend
                    .remove_composite_loop(BackendCompositeId::from_raw(composite_id))
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetTrackControl { track_id, control } => {
                self.backend
                    .set_track_control(
                        BackendTrackId::from_raw(track_id),
                        from_wire_track_control(control),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetTrackFxControl { track_id, control } => {
                self.backend
                    .set_track_fx_control(
                        BackendTrackId::from_raw(track_id),
                        from_wire_track_fx_control(control),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetLoopGain { loop_id, gain } => {
                self.backend
                    .set_loop_gain(BackendLoopId::from_raw(loop_id), gain)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetLoopBalance { loop_id, balance } => {
                self.backend
                    .set_loop_balance(BackendLoopId::from_raw(loop_id), balance)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::GrabLoops { requests } => {
                let requests = requests
                    .into_iter()
                    .map(|request| BackendGrabRequest {
                        loop_id: BackendLoopId::from_raw(request.loop_id),
                        reverse_start_cycle: request.reverse_start_cycle,
                        cycles_length: request.cycles_length,
                        go_to_cycle: request.go_to_cycle,
                        go_to_mode: from_wire_loop_mode(request.go_to_mode),
                    })
                    .collect::<Vec<_>>();
                self.backend
                    .grab_loops(&requests)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetLoopSyncSource { loop_id, source } => {
                self.backend
                    .set_loop_sync_source(
                        BackendLoopId::from_raw(loop_id),
                        source.map(BackendLoopId::from_raw),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::TransitionLoop {
                loop_id,
                mode,
                cycles_delay,
            } => {
                self.backend
                    .transition_loop(
                        BackendLoopId::from_raw(loop_id),
                        from_wire_loop_mode(mode),
                        cycles_delay,
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::ClearLoop { loop_id } => {
                self.backend
                    .clear_loop(BackendLoopId::from_raw(loop_id))
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetLoopLength { loop_id, length } => {
                self.backend
                    .set_loop_length(BackendLoopId::from_raw(loop_id), length)
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::SetLoopTiming {
                loop_id,
                start_offset,
                preplay,
                length,
            } => {
                self.backend
                    .set_loop_timing(
                        BackendLoopId::from_raw(loop_id),
                        start_offset,
                        preplay,
                        length,
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Ack)
            }
            Command::BeginLoopContentReplace {
                generation,
                loop_id,
                total_bytes,
            } => {
                if generation == 0 || total_bytes > SESSION_TRANSFER_MAX_BYTES {
                    return Err("invalid loop content replacement size or generation".to_owned());
                }
                self.loop_content_generation = Some(generation);
                self.loop_content_id = Some(BackendLoopId::from_raw(loop_id));
                self.loop_content_expected_bytes = total_bytes;
                self.loop_content_bytes = Vec::with_capacity(total_bytes);
                Ok(Event::Ack)
            }
            Command::WriteLoopContentReplace {
                generation,
                offset,
                bytes,
            } => {
                if self.loop_content_generation != Some(generation) {
                    return Err("stale loop content replacement generation".to_owned());
                }
                if bytes.len() > SESSION_TRANSFER_CHUNK_BYTES
                    || offset != self.loop_content_bytes.len()
                    || self.loop_content_bytes.len().saturating_add(bytes.len())
                        > self.loop_content_expected_bytes
                {
                    return Err("invalid loop content replacement chunk".to_owned());
                }
                self.loop_content_bytes.extend_from_slice(&bytes);
                Ok(Event::Ack)
            }
            Command::CommitLoopContentReplace { generation } => {
                if self.loop_content_generation != Some(generation)
                    || self.loop_content_bytes.len() != self.loop_content_expected_bytes
                {
                    return Err("loop content replacement is incomplete or stale".to_owned());
                }
                let update: BackendLoopContentUpdate =
                    serde_json::from_slice(&self.loop_content_bytes)
                        .map_err(|error| format!("invalid prepared loop content: {error}"))?;
                let loop_id = self
                    .loop_content_id
                    .ok_or_else(|| "loop content replacement omitted its target".to_owned())?;
                self.backend
                    .replace_loop_content(loop_id, &update)
                    .map_err(|error| error.to_string())?;
                self.loop_content_generation = None;
                self.loop_content_id = None;
                self.loop_content_expected_bytes = 0;
                self.loop_content_bytes.clear();
                Ok(Event::LoopContentReplaceComplete { generation })
            }
            Command::SetPortConnected {
                application_port_id,
                host_port_id,
                connected,
            } => {
                match self.backend.set_port_connected(
                    BackendPortId::from_raw(application_port_id),
                    &host_port_id,
                    connected,
                ) {
                    Ok(()) => Ok(Event::Ack),
                    Err(error) => Ok(Event::ConnectionMutationFailed {
                        application_port_id,
                        host_port_id,
                        desired_connected: connected,
                        message: error.to_string(),
                    }),
                }
            }
            Command::RequestWaveform {
                loop_id,
                revision,
                channel,
                offset,
                max_samples,
            } => {
                let chunk = self
                    .backend
                    .loop_audio_data_chunk(
                        BackendLoopId::from_raw(loop_id),
                        channel,
                        offset,
                        max_samples.min(WAVEFORM_CHUNK_SAMPLES),
                    )
                    .map_err(|error| error.to_string())?;
                Ok(Event::Waveform(WaveformChunk {
                    loop_id,
                    revision,
                    channel: chunk.channel,
                    channel_count: chunk.channel_count,
                    offset: chunk.offset,
                    total_samples: chunk.total_samples,
                    start_offset: chunk.start_offset,
                    preplay: chunk.preplay,
                    final_chunk: chunk.offset.saturating_add(chunk.samples.len())
                        >= chunk.total_samples,
                    samples: chunk.samples,
                }))
            }
            Command::RequestMidiData {
                loop_id,
                generation,
                channel,
                offset,
                max_events,
            } => {
                if generation == 0 || max_events == 0 {
                    return Err("invalid MIDI detail request".to_owned());
                }
                let data = self
                    .backend
                    .loop_midi_data(BackendLoopId::from_raw(loop_id))
                    .map_err(|error| error.to_string())?
                    .ok_or_else(|| "MIDI detail data is not ready".to_owned())?;
                let channel_count = data.channels.len();
                if channel_count == 0 {
                    return Ok(Event::MidiData(MidiDataChunk {
                        loop_id,
                        generation,
                        final_chunk: true,
                        ..Default::default()
                    }));
                }
                let channel_data = data
                    .channels
                    .get(channel)
                    .ok_or_else(|| "unknown MIDI detail channel".to_owned())?;
                if offset > channel_data.events.len() {
                    return Err("invalid MIDI detail event offset".to_owned());
                }
                let end = offset
                    .saturating_add(max_events.min(MIDI_DETAIL_CHUNK_EVENTS))
                    .min(channel_data.events.len());
                Ok(Event::MidiData(MidiDataChunk {
                    loop_id,
                    generation,
                    content_revision: channel_data.content_revision,
                    mode: match channel_data.mode {
                        shoop_backend::BackendChannelMode::Direct => WireChannelMode::Direct,
                        shoop_backend::BackendChannelMode::Dry => WireChannelMode::Dry,
                        shoop_backend::BackendChannelMode::Wet => WireChannelMode::Wet,
                    },
                    channel,
                    channel_count,
                    offset,
                    total_events: channel_data.events.len(),
                    length: channel_data.length,
                    start_offset: channel_data.start_offset,
                    preplay: channel_data.preplay,
                    final_chunk: end >= channel_data.events.len(),
                    events: channel_data.events[offset..end]
                        .iter()
                        .map(|event| shoop_audio_protocol::WireMidiEvent {
                            frame: event.time,
                            data: event.data.clone(),
                        })
                        .collect(),
                }))
            }
            Command::BeginSessionCapture { generation } => {
                if generation == 0 {
                    return Err("session capture generation must be non-zero".to_owned());
                }
                let capture = self
                    .backend
                    .capture_session()
                    .map_err(|error| error.to_string())?;
                let bytes = serde_json::to_vec(&capture).map_err(|error| error.to_string())?;
                if bytes.len() > SESSION_TRANSFER_MAX_BYTES {
                    return Err("session capture exceeds the transfer limit".to_owned());
                }
                self.capture_generation = Some(generation);
                self.capture_bytes = bytes;
                Ok(Event::SessionCaptureReady {
                    generation,
                    total_bytes: self.capture_bytes.len(),
                })
            }
            Command::ReadSessionCapture {
                generation,
                offset,
                max_bytes,
            } => {
                if self.capture_generation != Some(generation) {
                    return Err("stale session capture generation".to_owned());
                }
                if offset > self.capture_bytes.len() {
                    return Err("session capture offset is out of range".to_owned());
                }
                let wanted = max_bytes.min(SESSION_TRANSFER_CHUNK_BYTES);
                let end = offset.saturating_add(wanted).min(self.capture_bytes.len());
                Ok(Event::SessionCaptureChunk {
                    generation,
                    offset,
                    total_bytes: self.capture_bytes.len(),
                    final_chunk: end >= self.capture_bytes.len(),
                    bytes: self.capture_bytes[offset..end].to_vec(),
                })
            }
            Command::BeginSessionReplace {
                generation,
                total_bytes,
            } => {
                if generation == 0 || total_bytes > SESSION_TRANSFER_MAX_BYTES {
                    return Err("invalid session replacement size or generation".to_owned());
                }
                self.replace_generation = Some(generation);
                self.replace_expected_bytes = total_bytes;
                self.replace_bytes = Vec::with_capacity(total_bytes);
                Ok(Event::Ack)
            }
            Command::WriteSessionReplace {
                generation,
                offset,
                bytes,
            } => {
                if self.replace_generation != Some(generation) {
                    return Err("stale session replacement generation".to_owned());
                }
                if bytes.len() > SESSION_TRANSFER_CHUNK_BYTES
                    || offset != self.replace_bytes.len()
                    || self.replace_bytes.len().saturating_add(bytes.len())
                        > self.replace_expected_bytes
                {
                    return Err("invalid session replacement chunk".to_owned());
                }
                self.replace_bytes.extend_from_slice(&bytes);
                Ok(Event::Ack)
            }
            Command::CommitSessionReplace { generation } => {
                if self.replace_generation != Some(generation)
                    || self.replace_bytes.len() != self.replace_expected_bytes
                {
                    return Err("session replacement is incomplete or stale".to_owned());
                }
                let session: BackendSessionData = serde_json::from_slice(&self.replace_bytes)
                    .map_err(|error| format!("invalid prepared session: {error}"))?;
                self.backend
                    .replace_session(&session)
                    .map_err(|error| error.to_string())?;
                self.replace_generation = None;
                self.replace_expected_bytes = 0;
                self.replace_bytes.clear();
                self.capture_generation = None;
                self.capture_bytes.clear();
                Ok(Event::SessionReplaceComplete { generation })
            }
            Command::AbortSessionTransfer { generation } => {
                if self.capture_generation == Some(generation) {
                    self.capture_generation = None;
                    self.capture_bytes.clear();
                }
                if self.replace_generation == Some(generation) {
                    self.replace_generation = None;
                    self.replace_expected_bytes = 0;
                    self.replace_bytes.clear();
                }
                if self.loop_content_generation == Some(generation) {
                    self.loop_content_generation = None;
                    self.loop_content_id = None;
                    self.loop_content_expected_bytes = 0;
                    self.loop_content_bytes.clear();
                }
                Ok(Event::SessionTransferAborted { generation })
            }
            Command::Poll => {
                if let Some(message) = self.fatal_error.clone() {
                    return Err(message);
                }
                self.backend
                    .poll()
                    .map(to_wire_snapshot)
                    .map(Event::Snapshot)
                    .map_err(|error| error.to_string())
            }
            Command::Shutdown => {
                self.stopped = true;
                self.capture_generation = None;
                self.capture_bytes.clear();
                self.replace_generation = None;
                self.replace_expected_bytes = 0;
                self.replace_bytes.clear();
                self.loop_content_generation = None;
                self.loop_content_id = None;
                self.loop_content_expected_bytes = 0;
                self.loop_content_bytes.clear();
                Ok(Event::Stopped)
            }
        }
    }
}

fn from_wire_track_control(control: WireTrackControl) -> BackendTrackControl {
    match control {
        WireTrackControl::OutputGainDb(value) => BackendTrackControl::OutputGainDb(value),
        WireTrackControl::OutputBalance(value) => BackendTrackControl::OutputBalance(value),
        WireTrackControl::OutputMute(value) => BackendTrackControl::OutputMute(value),
        WireTrackControl::InputGainDb(value) => BackendTrackControl::InputGainDb(value),
        WireTrackControl::InputBalance(value) => BackendTrackControl::InputBalance(value),
        WireTrackControl::InputMonitoring(value) => BackendTrackControl::InputMonitoring(value),
    }
}

fn from_wire_track_topology(topology: WireTrackTopology) -> BackendTrackTopology {
    match topology {
        WireTrackTopology::Direct {
            audio_channels,
            midi,
        } => BackendTrackTopology::Direct {
            audio_channels,
            midi,
        },
        WireTrackTopology::TinySynthFx { audio_channels } => {
            BackendTrackTopology::DryWetProcessor {
                processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                dry_audio_channels: audio_channels,
                wet_audio_channels: audio_channels,
                dry_midi: true,
            }
        }
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

fn from_wire_track_fx_control(control: WireTrackFxControl) -> BackendTrackFxControl {
    match control {
        WireTrackFxControl::SetActive(value) => BackendTrackFxControl::SetActive(value),
        WireTrackFxControl::SetVisible(value) => BackendTrackFxControl::SetVisible(value),
        WireTrackFxControl::ToggleOrRecover => BackendTrackFxControl::ToggleOrRecover,
        WireTrackFxControl::RestoreState(value) => BackendTrackFxControl::RestoreState(value),
        WireTrackFxControl::ClearLogs => BackendTrackFxControl::ClearLogs,
        WireTrackFxControl::TinySelectPreset(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SelectPreset(value))
        }
        WireTrackFxControl::TinySetMasterGainDb(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetMasterGainDb(value))
        }
        WireTrackFxControl::TinySetReverbEnabled(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetReverbEnabled(value))
        }
        WireTrackFxControl::TinySetReverbAmount(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetReverbAmount(value))
        }
        WireTrackFxControl::TinySetDistortionEnabled(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetDistortionEnabled(value))
        }
        WireTrackFxControl::TinySetDistortionDrive(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetDistortionDrive(value))
        }
        WireTrackFxControl::TinySetCompressorEnabled(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetCompressorEnabled(value))
        }
        WireTrackFxControl::TinySetCompressorAmount(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetCompressorAmount(value))
        }
        WireTrackFxControl::TinySetEqEnabled(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetEqEnabled(value))
        }
        WireTrackFxControl::TinySetEqLowDb(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetEqLowDb(value))
        }
        WireTrackFxControl::TinySetEqMidDb(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetEqMidDb(value))
        }
        WireTrackFxControl::TinySetEqHighDb(value) => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SetEqHighDb(value))
        }
        WireTrackFxControl::TinyAssignMidiCc(assignment) => BackendTrackFxControl::TinySynthFx(
            TinySynthFxControl::AssignMidiCc(TinySynthFxMidiCcAssignment {
                parameter: from_wire_tiny_parameter(assignment.parameter),
                channel: assignment.channel,
                controller: assignment.controller,
            }),
        ),
        WireTrackFxControl::TinyRemoveMidiCc(parameter) => BackendTrackFxControl::TinySynthFx(
            TinySynthFxControl::RemoveMidiCc(from_wire_tiny_parameter(parameter)),
        ),
        WireTrackFxControl::TinyClearMidiCcAssignments => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::ClearMidiCcAssignments)
        }
        WireTrackFxControl::TinyPanic => {
            BackendTrackFxControl::TinySynthFx(TinySynthFxControl::Panic)
        }
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

fn from_wire_composite_config(config: WireCompositeConfig) -> BackendCompositeConfig {
    BackendCompositeConfig {
        kind: match config.kind {
            WireCompositeKind::Regular => BackendCompositeKind::Regular,
            WireCompositeKind::Script => BackendCompositeKind::Script,
        },
        sync_source: BackendLoopId::from_raw(config.sync_source),
        timelines: config
            .timelines
            .into_iter()
            .map(|timeline| {
                timeline
                    .into_iter()
                    .map(|section| {
                        section
                            .into_iter()
                            .map(|entry| BackendCompositeEntry {
                                target: match entry.target {
                                    WireCompositeTarget::Loop(id) => {
                                        BackendCompositeTarget::Loop(BackendLoopId::from_raw(id))
                                    }
                                    WireCompositeTarget::Composite(id) => {
                                        BackendCompositeTarget::Composite(
                                            BackendCompositeId::from_raw(id),
                                        )
                                    }
                                },
                                delay: entry.delay,
                                n_cycles: entry.n_cycles,
                                mode: entry.mode.map(from_wire_loop_mode),
                            })
                            .collect()
                    })
                    .collect()
            })
            .collect(),
    }
}

fn to_wire_track_topology(topology: &BackendTrackTopology) -> WireTrackTopology {
    match topology {
        BackendTrackTopology::Direct {
            audio_channels,
            midi,
        } => WireTrackTopology::Direct {
            audio_channels: *audio_channels,
            midi: *midi,
        },
        BackendTrackTopology::DryWetProcessor {
            processor_type,
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } if processor_type == TrackProcessorTypeId::TINY_SYNTH_FX
            && dry_audio_channels == wet_audio_channels
            && *dry_midi =>
        {
            WireTrackTopology::TinySynthFx {
                audio_channels: *dry_audio_channels,
            }
        }
        _ => WireTrackTopology::Direct {
            audio_channels: 0,
            midi: false,
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

fn to_wire_data_type(value: BackendPortDataType) -> WirePortDataType {
    match value {
        BackendPortDataType::Audio => WirePortDataType::Audio,
        BackendPortDataType::Midi => WirePortDataType::Midi,
    }
}

fn to_wire_direction(value: BackendPortDirection) -> WirePortDirection {
    match value {
        BackendPortDirection::Input => WirePortDirection::Input,
        BackendPortDirection::Output => WirePortDirection::Output,
    }
}

fn to_wire_role(value: BackendPortRole) -> WirePortRole {
    match value {
        BackendPortRole::AudioInput => WirePortRole::AudioInput,
        BackendPortRole::AudioOutput => WirePortRole::AudioOutput,
        BackendPortRole::AudioSend => WirePortRole::AudioSend,
        BackendPortRole::AudioReturn => WirePortRole::AudioReturn,
        BackendPortRole::MidiInput => WirePortRole::MidiInput,
        BackendPortRole::MidiOutput => WirePortRole::MidiOutput,
        BackendPortRole::MidiSend => WirePortRole::MidiSend,
    }
}

fn to_wire_snapshot(snapshot: BackendSnapshot) -> WireSnapshot {
    let application_ports = snapshot
        .connections
        .application_ports
        .into_values()
        .map(|port| WireApplicationPort {
            id: port.id.raw(),
            owner: match port.owner {
                BackendPortOwner::Track => WireApplicationPortOwner::Track,
                BackendPortOwner::GlobalFxControl => WireApplicationPortOwner::GlobalFxControl,
            },
            name: port.name,
            data_type: to_wire_data_type(port.data_type),
            direction: to_wire_direction(port.direction),
            role: to_wire_role(port.role),
        })
        .collect();
    let host_ports = snapshot
        .connections
        .host_ports
        .into_values()
        .map(|port| WireHostPort {
            id: port.id,
            name: port.name,
            data_type: to_wire_data_type(port.data_type),
            direction: to_wire_direction(port.direction),
        })
        .collect();
    let confirmed_links = snapshot
        .connections
        .confirmed_links
        .into_iter()
        .map(|link| WireConfirmedLink {
            application_port_id: link.application_port_id.raw(),
            host_port_id: link.host_port_id,
        })
        .collect();
    WireSnapshot {
        sample_rate: snapshot.status.sample_rate,
        quantum: snapshot.status.buffer_size,
        callback_count: snapshot.status.callback_count,
        processed_frames: snapshot.status.processed_frames,
        input_peak: snapshot.status.input_peak,
        output_peak: snapshot.status.output_peak,
        xruns: snapshot.status.xruns,
        callback_budget_overruns: snapshot.status.callback_budget_overruns,
        render_discontinuities: snapshot.status.render_discontinuities,
        memory_growths: snapshot.status.memory_growths,
        render_memory_growths: snapshot.status.render_memory_growths,
        command_overflows: snapshot.status.command_overflows,
        storage_low_channels: snapshot.status.storage_low_channels,
        storage_exhaustions: snapshot.status.storage_exhaustions,
        tracks: snapshot
            .tracks
            .into_iter()
            .map(|(id, track)| WireTrackState {
                id: id.raw(),
                topology: to_wire_track_topology(&track.topology),
                fx: track.fx.and_then(|fx| {
                    let TrackProcessorEditorState::TinySynthFx(editor) = fx.editor?;
                    Some(WireTrackFxState {
                        active: fx.active,
                        visible: fx.visible,
                        tiny: WireTinySynthFxState {
                            selected_preset_id: editor.selected_preset_id,
                            master_gain_db: editor.master_gain_db,
                            reverb_enabled: editor.reverb_enabled,
                            reverb_amount: editor.reverb_amount,
                            distortion_enabled: editor.distortion_enabled,
                            distortion_drive: editor.distortion_drive,
                            compressor_enabled: editor.compressor_enabled,
                            compressor_amount: editor.compressor_amount,
                            eq_enabled: editor.eq_enabled,
                            eq_low_db: editor.eq_low_db,
                            eq_mid_db: editor.eq_mid_db,
                            eq_high_db: editor.eq_high_db,
                            midi_cc_assignments: editor
                                .midi_cc_assignments
                                .iter()
                                .map(|assignment| WireTinySynthFxMidiCcAssignment {
                                    parameter: to_wire_tiny_parameter(assignment.parameter),
                                    channel: assignment.channel,
                                    controller: assignment.controller,
                                })
                                .collect(),
                        },
                    })
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
                    WireLatestMidiMessage {
                        bytes: message.bytes,
                        len: message.len,
                    }
                }),
            })
            .collect(),
        loops: snapshot
            .loops
            .into_iter()
            .map(|(id, loop_)| WireLoopState {
                id: id.raw(),
                mode: to_wire_loop_mode(loop_.mode),
                length: loop_.length,
                position: loop_.position,
                next_mode: loop_.next_mode.map(to_wire_loop_mode),
                next_transition_delay: loop_.next_transition_delay,
                stereo: loop_.stereo,
                gain: loop_.gain,
                balance: loop_.balance,
                audio_peaks: loop_.audio_peaks,
                midi_activity: loop_.midi_activity,
            })
            .collect(),
        composites: snapshot
            .composites
            .into_iter()
            .map(|(id, composite)| WireCompositeState {
                id: id.raw(),
                mode: to_wire_loop_mode(composite.mode),
                next_mode: composite.next_mode.map(to_wire_loop_mode),
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
                    .map(|child| WireActiveCompositeChild {
                        target: match child.target {
                            BackendCompositeTarget::Loop(id) => WireCompositeTarget::Loop(id.raw()),
                            BackendCompositeTarget::Composite(id) => {
                                WireCompositeTarget::Composite(id.raw())
                            }
                        },
                        mode: to_wire_loop_mode(child.mode),
                        cycle_offset: child.cycle_offset,
                    })
                    .collect(),
            })
            .collect(),
        application_ports,
        host_ports,
        confirmed_links,
    }
}

#[no_mangle]
pub extern "C" fn shoop_worklet_create(sample_rate: u32, max_quantum: u32) -> *mut WorkletHost {
    WorkletHost::new(sample_rate, max_quantum)
        .map(Box::new)
        .map(Box::into_raw)
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_destroy(host: *mut WorkletHost) {
    if !host.is_null() {
        drop(Box::from_raw(host));
    }
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_input_ptr(host: *mut WorkletHost) -> *mut f32 {
    host.as_mut()
        .map(|host| host.input.as_mut_ptr())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_output_ptr(host: *const WorkletHost) -> *const f32 {
    host.as_ref()
        .map(|host| host.output.as_ptr())
        .unwrap_or(std::ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_command_ptr(host: *mut WorkletHost) -> *mut u8 {
    host.as_mut()
        .map(|host| host.command_buffer.as_mut_ptr())
        .unwrap_or(std::ptr::null_mut())
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_command(host: *mut WorkletHost, length: usize) -> bool {
    let Some(host) = host.as_mut() else {
        return false;
    };
    if length > host.command_buffer.len() {
        return false;
    }
    let command = host.command_buffer[..length].to_vec();
    host.handle_json(&command);
    true
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_response_ptr(host: *const WorkletHost) -> *const u8 {
    host.as_ref()
        .map(|host| host.response.as_ptr())
        .unwrap_or(std::ptr::null())
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_response_len(host: *const WorkletHost) -> usize {
    host.as_ref().map(|host| host.response.len()).unwrap_or(0)
}

#[no_mangle]
pub unsafe extern "C" fn shoop_worklet_process(
    host: *mut WorkletHost,
    input_channels: usize,
    output_channels: usize,
    n_frames: usize,
) -> bool {
    host.as_mut()
        .is_some_and(|host| host.process(input_channels, output_channels, n_frames))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shoop_audio_protocol::{Command, CommandEnvelope, Event};

    fn command(host: &mut WorkletHost, sequence: u64, command: Command) -> EventEnvelope {
        let json = serde_json::to_vec(&CommandEnvelope::new(sequence, command)).unwrap();
        serde_json::from_str(host.handle_json(&json)).unwrap()
    }

    #[shoop_wasm_test_support::shoop_test]
    fn worklet_removes_and_recreates_same_named_track_ports() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let create = |expected_track_id, expected_loop_id| Command::CreateTrack {
            expected_track_id,
            expected_loop_ids: vec![expected_loop_id],
            port_name_base: "reusable".to_owned(),
            topology: WireTrackTopology::Direct {
                audio_channels: 1,
                midi: true,
            },
        };
        assert!(matches!(
            command(&mut host, 1, create(1, 1)).event,
            Event::Ack
        ));
        let Event::Snapshot(first) = command(&mut host, 2, Command::Poll).event else {
            panic!("expected first snapshot");
        };
        let first_ports = first
            .application_ports
            .iter()
            .filter(|port| matches!(port.owner, WireApplicationPortOwner::Track))
            .map(|port| (port.id, port.name.clone()))
            .collect::<Vec<_>>();

        assert!(matches!(
            command(&mut host, 3, Command::RemoveTrack { track_id: 1 }).event,
            Event::Ack
        ));
        assert!(matches!(
            command(&mut host, 4, create(2, 2)).event,
            Event::Ack
        ));
        let Event::Snapshot(recreated) = command(&mut host, 5, Command::Poll).event else {
            panic!("expected recreated snapshot");
        };
        assert_eq!(
            recreated
                .tracks
                .iter()
                .map(|track| track.id)
                .collect::<Vec<_>>(),
            [2]
        );
        assert_eq!(
            recreated
                .loops
                .iter()
                .map(|loop_| loop_.id)
                .collect::<Vec<_>>(),
            [2]
        );
        let recreated_ports = recreated
            .application_ports
            .iter()
            .filter(|port| matches!(port.owner, WireApplicationPortOwner::Track))
            .map(|port| (port.id, port.name.clone()))
            .collect::<Vec<_>>();
        assert_eq!(
            recreated_ports
                .iter()
                .map(|(_, name)| name)
                .collect::<Vec<_>>(),
            first_ports.iter().map(|(_, name)| name).collect::<Vec<_>>()
        );
        assert!(recreated_ports
            .iter()
            .all(|(id, _)| first_ports.iter().all(|(old_id, _)| old_id != id)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn worklet_composite_contract_controls_and_publishes_independent_parent_state() {
        let mut host = WorkletHost::new(1_000, 8).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1, 2, 3, 4],
                    port_name_base: "composite".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 0,
                        midi: false,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        for (sequence, loop_id, length) in [(2, 1, 1), (3, 2, 4), (4, 3, 4), (5, 4, 4)] {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::SetLoopLength { loop_id, length },
                )
                .event,
                Event::Ack
            ));
        }
        let config = WireCompositeConfig {
            kind: WireCompositeKind::Regular,
            sync_source: 1,
            timelines: vec![vec![2, 3, 4]
                .into_iter()
                .map(|id| {
                    vec![shoop_audio_protocol::WireCompositeEntry {
                        target: WireCompositeTarget::Loop(id),
                        delay: 0,
                        n_cycles: None,
                        mode: None,
                    }]
                })
                .collect()],
        };
        assert!(matches!(
            command(
                &mut host,
                6,
                Command::CreateComposite {
                    expected_composite_id: 1,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                7,
                Command::ConfigureComposite {
                    composite_id: 1,
                    config: config.clone(),
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                8,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Playing,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                9,
                Command::TransitionComposite {
                    composite_id: 1,
                    mode: WireLoopMode::Playing,
                    cycles_delay: None,
                    align_to_iteration: None,
                },
            )
            .event,
            Event::Ack
        ));
        let Event::Snapshot(started) = command(&mut host, 10, Command::Poll).event else {
            panic!("expected composite snapshot");
        };
        assert_eq!(started.composites[0].mode, WireLoopMode::Playing);
        assert_eq!(
            started.composites[0].active_children[0].target,
            WireCompositeTarget::Loop(2)
        );

        for _ in 0..4 {
            assert!(host.process(0, 0, 1));
        }
        let Event::Snapshot(advanced) = command(&mut host, 11, Command::Poll).event else {
            panic!("expected composite snapshot");
        };
        assert_eq!(advanced.composites[0].iteration, 4);
        assert_eq!(advanced.composites[0].position, 4);
        assert_eq!(
            advanced.composites[0].active_children[0].target,
            WireCompositeTarget::Loop(3)
        );

        assert!(matches!(
            command(
                &mut host,
                12,
                Command::CreateComposite {
                    expected_composite_id: 2,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                13,
                Command::ConfigureComposite {
                    composite_id: 2,
                    config,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                14,
                Command::TransitionLoop {
                    loop_id: 4,
                    mode: WireLoopMode::Playing,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(host.process(0, 0, 1));
        let Event::Snapshot(isolated) = command(&mut host, 15, Command::Poll).event else {
            panic!("expected composite snapshot");
        };
        assert_eq!(isolated.composites[1].mode, WireLoopMode::Stopped);
        assert_eq!(isolated.composites[1].position, 0);
        assert!(isolated.composites[1].active_children.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn protocol_orders_commands_and_runs_non_silent_full_duplex_cycles() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::ConfigureDeviceChannels {
                    input_channels: 1,
                    output_channels: 2,
                },
            )
            .event,
            Event::Ack
        ));
        let created = command(
            &mut host,
            2,
            Command::CreateTrack {
                expected_track_id: 1,
                expected_loop_ids: vec![1],
                port_name_base: "direct".to_owned(),
                topology: WireTrackTopology::Direct {
                    audio_channels: 1,
                    midi: false,
                },
            },
        );
        assert!(matches!(created.event, Event::Ack));
        assert!(matches!(
            command(
                &mut host,
                3,
                Command::SetTrackControl {
                    track_id: 1,
                    control: WireTrackControl::InputMonitoring(true),
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                4,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Recording,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        host.input()[..128].fill(0.25);
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(1, 2, 128)));
        assert!(host.output()[..128].iter().any(|sample| *sample != 0.0));
        assert!(host.output()[128..256].iter().any(|sample| *sample != 0.0));
        assert!(matches!(
            command(
                &mut host,
                5,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Stopped,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        let waveform = command(
            &mut host,
            6,
            Command::RequestWaveform {
                loop_id: 1,
                revision: 1,
                channel: 0,
                offset: 0,
                max_samples: 512,
            },
        );
        let Event::Waveform(waveform) = waveform.event else {
            panic!("expected waveform");
        };
        assert_eq!(waveform.total_samples, 128);
        assert!(waveform.samples.iter().all(|sample| *sample == 0.25));
        let status = command(&mut host, 7, Command::Poll);
        let Event::Snapshot(snapshot) = status.event else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.callback_count, 1);
        assert_eq!(snapshot.processed_frames, 128);
        assert!(snapshot.input_peak > 0.0);
        assert!(snapshot.output_peak > 0.0);
        assert!(matches!(
            command(
                &mut host,
                8,
                Command::SetLoopBalance {
                    loop_id: 1,
                    balance: 0.5,
                },
            )
            .event,
            Event::Ack
        ));
        let Event::Snapshot(snapshot) = command(&mut host, 9, Command::Poll).event else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.loops[0].balance, 0.5);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn midi_details_are_bounded_and_chunked_without_session_capture() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "midi_details".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 0,
                        midi: true,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        let events = (0..200)
            .map(|index| BackendMidiEvent {
                time: index,
                data: vec![0x90, (index % 128) as u8, 100],
            })
            .collect::<Vec<_>>();
        host.backend
            .replace_loop_content(
                BackendLoopId::from_raw(1),
                &BackendLoopContentUpdate {
                    midi: vec![shoop_backend::BackendMidiChannelUpdate {
                        channel: 0,
                        length: 256,
                        start_state: Vec::new(),
                        events,
                        start_offset: Some(-2),
                        preplay: Some(3),
                    }],
                    length: Some(256),
                    ..Default::default()
                },
            )
            .unwrap();
        let Event::MidiData(first) = command(
            &mut host,
            2,
            Command::RequestMidiData {
                loop_id: 1,
                generation: 7,
                channel: 0,
                offset: 0,
                max_events: usize::MAX,
            },
        )
        .event
        else {
            panic!("expected MIDI data");
        };
        assert_eq!(first.events.len(), MIDI_DETAIL_CHUNK_EVENTS);
        assert_eq!(first.total_events, 200);
        assert!(!first.final_chunk);
        assert_eq!(first.start_offset, -2);
        let Event::MidiData(second) = command(
            &mut host,
            3,
            Command::RequestMidiData {
                loop_id: 1,
                generation: 7,
                channel: 0,
                offset: first.events.len(),
                max_events: MIDI_DETAIL_CHUNK_EVENTS,
            },
        )
        .event
        else {
            panic!("expected MIDI data");
        };
        assert_eq!(second.events.len(), MIDI_DETAIL_CHUNK_EVENTS);
        assert!(!second.final_chunk);
        let Event::MidiData(last) = command(
            &mut host,
            4,
            Command::RequestMidiData {
                loop_id: 1,
                generation: 7,
                channel: 0,
                offset: 192,
                max_events: MIDI_DETAIL_CHUNK_EVENTS,
            },
        )
        .event
        else {
            panic!("expected MIDI data");
        };
        assert_eq!(last.events.len(), 8);
        assert!(last.final_chunk);
        assert!(host.capture_bytes.is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn tiny_synth_fx_runs_all_shapes_and_controls_in_the_worklet() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::ConfigureDeviceChannels {
                    input_channels: 0,
                    output_channels: 2,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                2,
                Command::ConfigureMidiEndpoints {
                    endpoints: vec![WireHostPort {
                        id: "webmidi:source:tiny".to_owned(),
                        name: "Tiny MIDI".to_owned(),
                        data_type: WirePortDataType::Midi,
                        direction: WirePortDirection::Output,
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        for (sequence, track_id, channels) in [(3, 1, 1), (4, 2, 0), (5, 3, 2), (6, 4, 7)] {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::CreateTrack {
                        expected_track_id: track_id,
                        expected_loop_ids: vec![track_id],
                        port_name_base: format!("tiny_{channels}"),
                        topology: WireTrackTopology::TinySynthFx {
                            audio_channels: channels,
                        },
                    },
                )
                .event,
                Event::Ack
            ));
        }
        assert!(matches!(
            command(
                &mut host,
                7,
                Command::SetPortConnected {
                    application_port_id: 3,
                    host_port_id: "webmidi:source:tiny".to_owned(),
                    connected: true,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                8,
                Command::SetTrackControl {
                    track_id: 1,
                    control: WireTrackControl::InputMonitoring(true),
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(&mut host, 9, Command::Poll).event,
            Event::Snapshot(_)
        ));
        assert!(matches!(
            command(
                &mut host,
                10,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:tiny".to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0x90, 69, 127],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        assert!(host.output()[..256]
            .iter()
            .any(|sample| sample.abs() > 0.001));

        for (sequence, control) in [
            WireTrackFxControl::TinySelectPreset("pad".to_owned()),
            WireTrackFxControl::TinySetMasterGainDb(-12.0),
            WireTrackFxControl::TinySetReverbEnabled(true),
            WireTrackFxControl::TinySetReverbAmount(0.4),
            WireTrackFxControl::TinySetDistortionEnabled(true),
            WireTrackFxControl::TinySetDistortionDrive(7.0),
            WireTrackFxControl::TinySetCompressorEnabled(true),
            WireTrackFxControl::TinySetCompressorAmount(0.6),
            WireTrackFxControl::TinySetEqEnabled(true),
            WireTrackFxControl::TinySetEqLowDb(3.0),
            WireTrackFxControl::TinySetEqMidDb(-2.0),
            WireTrackFxControl::TinySetEqHighDb(1.5),
            WireTrackFxControl::TinyAssignMidiCc(WireTinySynthFxMidiCcAssignment {
                parameter: WireTinySynthFxParameter::ReverbAmount,
                channel: 2,
                controller: 17,
            }),
            WireTrackFxControl::SetVisible(true),
            WireTrackFxControl::TinyPanic,
        ]
        .into_iter()
        .enumerate()
        {
            assert!(matches!(
                command(
                    &mut host,
                    11 + sequence as u64,
                    Command::SetTrackFxControl {
                        track_id: 1,
                        control,
                    },
                )
                .event,
                Event::Ack
            ));
        }
        assert!(matches!(
            command(
                &mut host,
                26,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:tiny".to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0xb2, 17, 127],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        let Event::Snapshot(snapshot) = command(&mut host, 27, Command::Poll).event else {
            panic!("missing worklet snapshot");
        };
        assert_eq!(
            snapshot
                .tracks
                .iter()
                .map(|track| track.topology.clone())
                .collect::<Vec<_>>(),
            vec![
                WireTrackTopology::TinySynthFx { audio_channels: 1 },
                WireTrackTopology::TinySynthFx { audio_channels: 0 },
                WireTrackTopology::TinySynthFx { audio_channels: 2 },
                WireTrackTopology::TinySynthFx { audio_channels: 7 },
            ]
        );
        let fx = snapshot.tracks[0].fx.as_ref().unwrap();
        assert!(fx.visible);
        assert_eq!(fx.tiny.selected_preset_id, None);
        assert_eq!(fx.tiny.master_gain_db, -12.0);
        assert!(fx.tiny.reverb_enabled);
        assert_eq!(fx.tiny.reverb_amount, 1.0);
        assert_eq!(
            fx.tiny.midi_cc_assignments,
            [WireTinySynthFxMidiCcAssignment {
                parameter: WireTinySynthFxParameter::ReverbAmount,
                channel: 2,
                controller: 17,
            }]
        );
        assert_eq!(
            snapshot.tracks[0].latest_input_midi_message,
            Some(WireLatestMidiMessage {
                bytes: [0xb2, 17, 127, 0],
                len: 3,
            })
        );
        assert!(fx.tiny.distortion_enabled);
        assert_eq!(fx.tiny.distortion_drive, 7.0);
        assert!(fx.tiny.compressor_enabled);
        assert_eq!(fx.tiny.compressor_amount, 0.6);
        assert!(fx.tiny.eq_enabled);
        assert_eq!(fx.tiny.eq_low_db, 3.0);
        assert_eq!(fx.tiny.eq_mid_db, -2.0);
        assert_eq!(fx.tiny.eq_high_db, 1.5);

        let session = host.backend.capture_session().unwrap();
        host.backend.replace_session(&session).unwrap();
        assert!(host
            .backend
            .poll()
            .unwrap()
            .tracks
            .values()
            .all(|track| !track.fx.as_ref().unwrap().visible));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn normalized_routes_mutate_authoritatively_without_stopping_audio() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::ConfigureDeviceChannels {
                    input_channels: 2,
                    output_channels: 2,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                2,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "stereo".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 2,
                        midi: false,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                3,
                Command::SetTrackControl {
                    track_id: 1,
                    control: WireTrackControl::InputMonitoring(true),
                },
            )
            .event,
            Event::Ack
        ));
        let Event::Snapshot(snapshot) = command(&mut host, 4, Command::Poll).event else {
            panic!("expected snapshot");
        };
        assert_eq!(snapshot.application_ports.len(), 5);
        assert_eq!(snapshot.host_ports.len(), 4);
        assert_eq!(snapshot.confirmed_links.len(), 4);

        host.input()[..128].fill(0.2);
        host.input()[128..256].fill(0.4);
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(2, 2, 128)));
        assert!(host.output()[..128]
            .iter()
            .all(|sample| (*sample - 0.2).abs() < 1.0e-6));
        assert!(host.output()[128..256]
            .iter()
            .all(|sample| (*sample - 0.4).abs() < 1.0e-6));

        assert!(matches!(
            command(
                &mut host,
                5,
                Command::SetPortConnected {
                    application_port_id: 2,
                    host_port_id: "webaudio:destination_1".to_owned(),
                    connected: false,
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(2, 2, 128)));
        assert!(host.output()[..128].iter().all(|sample| *sample == 0.0));
        assert!(host.output()[128..256]
            .iter()
            .all(|sample| (*sample - 0.4).abs() < 1.0e-6));
        let Event::Snapshot(snapshot) = command(&mut host, 6, Command::Poll).event else {
            panic!("expected snapshot");
        };
        assert!(!snapshot.confirmed_links.iter().any(|link| {
            link.application_port_id == 2 && link.host_port_id == "webaudio:destination_1"
        }));

        assert!(matches!(
            command(
                &mut host,
                7,
                Command::SetPortConnected {
                    application_port_id: 2,
                    host_port_id: "webaudio:capture_1".to_owned(),
                    connected: true,
                },
            )
            .event,
            Event::ConnectionMutationFailed { .. }
        ));
        assert!(matches!(
            command(&mut host, 8, Command::Poll).event,
            Event::Snapshot(_)
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn track_midi_injection_needs_no_web_midi_endpoint() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "piano".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 0,
                        midi: true,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                2,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Recording,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                3,
                Command::InjectTrackMidiInput {
                    track_id: 1,
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0x90, 60, 100],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 0, 128)));
        assert!(matches!(
            command(
                &mut host,
                4,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Stopped,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        let session = host.backend.capture_session().unwrap();
        assert_eq!(
            session.tracks[0].loops[0].midi[0].events[0].data,
            [0x90, 60, 100]
        );
        assert!(matches!(
            command(
                &mut host,
                5,
                Command::InjectTrackMidiInput {
                    track_id: 999,
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0x90, 60, 100],
                    }],
                },
            )
            .event,
            Event::Error { .. }
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn web_midi_commands_route_record_monitor_and_playback() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let endpoints = vec![
            WireHostPort {
                id: "webmidi:source:controller".to_owned(),
                name: "Controller input".to_owned(),
                data_type: WirePortDataType::Midi,
                direction: WirePortDirection::Output,
            },
            WireHostPort {
                id: "webmidi:sink:controller".to_owned(),
                name: "Controller output".to_owned(),
                data_type: WirePortDataType::Midi,
                direction: WirePortDirection::Input,
            },
        ];
        assert!(matches!(
            command(&mut host, 1, Command::ConfigureMidiEndpoints { endpoints },).event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                2,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "midi".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 0,
                        midi: true,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        for (sequence, application_port_id, host_port_id) in [
            (3, 1, "webmidi:source:controller"),
            (4, 2, "webmidi:sink:controller"),
        ] {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::SetPortConnected {
                        application_port_id,
                        host_port_id: host_port_id.to_owned(),
                        connected: true,
                    },
                )
                .event,
                Event::Ack
            ));
        }
        assert!(matches!(
            command(
                &mut host,
                5,
                Command::SetTrackControl {
                    track_id: 1,
                    control: WireTrackControl::InputMonitoring(true),
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                6,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Recording,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                7,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:controller".to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0x90, 60, 100],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 0, 128)));
        let Event::MidiOutput {
            events,
            dropped,
            refused_input,
        } = command(&mut host, 8, Command::DrainMidiOutput { max_events: 16 }).event
        else {
            panic!("expected Web MIDI output")
        };
        assert_eq!((dropped, refused_input), (0, 0));
        assert!(events.iter().any(|event| event.data == [0x90, 60, 100]));
        assert!(matches!(
            command(
                &mut host,
                9,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Stopped,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                10,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Playing,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 0, 128)));
        let Event::MidiOutput {
            events,
            dropped,
            refused_input,
        } = command(&mut host, 11, Command::DrainMidiOutput { max_events: 16 }).event
        else {
            panic!("expected Web MIDI playback output")
        };
        assert_eq!((dropped, refused_input), (0, 0));
        assert!(events.iter().any(|event| {
            event.host_port_id == "webmidi:sink:controller" && event.data == [0x90, 60, 100]
        }));
        assert!(matches!(
            command(
                &mut host,
                12,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:controller".to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0xf0, 1, 2, 3, 0xf7],
                    }],
                },
            )
            .event,
            Event::Error { .. }
        ));
        assert!(matches!(
            command(
                &mut host,
                13,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:controller".to_owned(),
                    events: vec![
                        shoop_audio_protocol::WireMidiEvent {
                            frame: 0,
                            data: vec![0xf8],
                        };
                        MIDI_BATCH_CAPACITY + 1
                    ],
                },
            )
            .event,
            Event::Error { .. }
        ));
        assert!(matches!(
            command(
                &mut host,
                14,
                Command::ConfigureMidiEndpoints {
                    endpoints: Vec::new(),
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                15,
                Command::PushMidiInput {
                    host_port_id: "webmidi:source:controller".to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0xf8],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn global_web_midi_dual_route_survives_capture_replace_and_stays_allocation_free() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let endpoint = "webmidi:source:global-dual";
        assert!(matches!(
            command(
                &mut host,
                1,
                Command::ConfigureMidiEndpoints {
                    endpoints: vec![WireHostPort {
                        id: endpoint.to_owned(),
                        name: "Global dual".to_owned(),
                        data_type: WirePortDataType::Midi,
                        direction: WirePortDirection::Output,
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                2,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "global_tiny".to_owned(),
                    topology: WireTrackTopology::TinySynthFx { audio_channels: 0 },
                },
            )
            .event,
            Event::Ack
        ));
        let Event::Snapshot(snapshot) = command(&mut host, 3, Command::Poll).event else {
            panic!("expected snapshot");
        };
        let global = snapshot
            .application_ports
            .iter()
            .find(|port| port.owner == WireApplicationPortOwner::GlobalFxControl)
            .unwrap()
            .id;
        let track_input = snapshot
            .application_ports
            .iter()
            .find(|port| {
                port.owner == WireApplicationPortOwner::Track
                    && port.role == WirePortRole::MidiInput
            })
            .unwrap()
            .id;
        for (sequence, port) in [(4, track_input), (5, global)] {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::SetPortConnected {
                        application_port_id: port,
                        host_port_id: endpoint.to_owned(),
                        connected: true,
                    },
                )
                .event,
                Event::Ack
            ));
        }
        assert!(matches!(
            command(
                &mut host,
                6,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Recording,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        assert!(matches!(
            command(
                &mut host,
                7,
                Command::PushMidiInput {
                    host_port_id: endpoint.to_owned(),
                    events: vec![shoop_audio_protocol::WireMidiEvent {
                        frame: 0,
                        data: vec![0xb0, 7, 101],
                    }],
                },
            )
            .event,
            Event::Ack
        ));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 0, 128)));
        assert!(matches!(
            command(
                &mut host,
                8,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Stopped,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        let session = host.backend.capture_session().unwrap();
        assert_eq!(session.global_ports[0].external_connections, vec![endpoint]);
        assert_eq!(
            session.tracks[0].loops[0].midi[0]
                .events
                .iter()
                .filter(|event| event.data == [0xb0, 7, 101])
                .count(),
            1
        );
        host.backend.replace_session(&session).unwrap();
        let snapshot = host.backend.poll().unwrap();
        assert!(snapshot.connections.confirmed_links.iter().any(|link| {
            snapshot
                .connections
                .application_ports
                .get(&link.application_port_id)
                .is_some_and(|port| port.owner == BackendPortOwner::GlobalFxControl)
                && link.host_port_id == endpoint
        }));
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 0, 128)));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn session_capture_and_replacement_use_bounded_chunks_and_keep_processing() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let mut sequence = 1_u64;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "session".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 4,
                        midi: true,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        let Event::SessionCaptureReady { total_bytes, .. } = command(
            &mut host,
            sequence,
            Command::BeginSessionCapture { generation: 7 },
        )
        .event
        else {
            panic!("expected session capture metadata")
        };
        sequence += 1;
        let mut captured = Vec::new();
        while captured.len() < total_bytes {
            let offset = captured.len();
            let Event::SessionCaptureChunk {
                bytes, final_chunk, ..
            } = command(
                &mut host,
                sequence,
                Command::ReadSessionCapture {
                    generation: 7,
                    offset,
                    max_bytes: SESSION_TRANSFER_CHUNK_BYTES,
                },
            )
            .event
            else {
                panic!("expected session capture chunk")
            };
            sequence += 1;
            assert!(bytes.len() <= SESSION_TRANSFER_CHUNK_BYTES);
            captured.extend_from_slice(&bytes);
            assert_eq!(final_chunk, captured.len() == total_bytes);
            assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        }
        let mut session: BackendSessionData = serde_json::from_slice(&captured).unwrap();
        session.tracks[0].loops[0].length = 4;
        session.tracks[0].loops[0].audio[0].samples = vec![0.1, 0.2, 0.3, 0.4];
        session.tracks[0].loops[0].midi[0] = shoop_backend::BackendMidiContent {
            mode: shoop_backend::BackendChannelMode::Direct,
            length: 4,
            start_state: vec![vec![0xB0, 7, 100]],
            events: vec![shoop_backend::BackendMidiEvent {
                time: 2,
                data: vec![0x90, 60, 100],
            }],
            start_offset: 0,
            preplay: 0,
        };
        let replacement = serde_json::to_vec(&session).unwrap();
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::BeginSessionReplace {
                    generation: 8,
                    total_bytes: replacement.len(),
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        for (index, chunk) in replacement.chunks(SESSION_TRANSFER_CHUNK_BYTES).enumerate() {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::WriteSessionReplace {
                        generation: 8,
                        offset: index * SESSION_TRANSFER_CHUNK_BYTES,
                        bytes: chunk.to_vec(),
                    },
                )
                .event,
                Event::Ack
            ));
            sequence += 1;
            assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        }
        let Event::Snapshot(before_commit) = command(&mut host, sequence, Command::Poll).event
        else {
            panic!("expected pre-commit snapshot")
        };
        assert!(before_commit.callback_count > 1);
        sequence += 1;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::CommitSessionReplace { generation: 8 },
            )
            .event,
            Event::SessionReplaceComplete { generation: 8 }
        ));
        sequence += 1;
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        let Event::Snapshot(snapshot) = command(&mut host, sequence, Command::Poll).event else {
            panic!("expected snapshot")
        };
        assert_eq!(snapshot.loops[0].length, 4);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn targeted_loop_content_transfer_commits_once_without_stopping_callbacks() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let mut sequence = 1_u64;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::CreateTrack {
                    expected_track_id: 1,
                    expected_loop_ids: vec![1],
                    port_name_base: "targeted".to_owned(),
                    topology: WireTrackTopology::Direct {
                        audio_channels: 2,
                        midi: true,
                    },
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        let update = BackendLoopContentUpdate {
            audio: vec![
                shoop_backend::BackendAudioChannelUpdate {
                    channel: 0,
                    samples: vec![0.25; 1024],
                    start_offset: Some(-1),
                    preplay: Some(2),
                },
                shoop_backend::BackendAudioChannelUpdate {
                    channel: 1,
                    samples: vec![0.5; 1024],
                    start_offset: Some(-2),
                    preplay: Some(3),
                },
            ],
            midi: vec![shoop_backend::BackendMidiChannelUpdate {
                channel: 0,
                length: 1024,
                start_state: vec![vec![0xB0, 7, 99]],
                events: vec![BackendMidiEvent {
                    time: 512,
                    data: vec![0x90, 64, 127],
                }],
                start_offset: Some(-3),
                preplay: Some(4),
            }],
            length: Some(1024),
        };
        let bytes = serde_json::to_vec(&update).unwrap();
        assert!(bytes.len() > SESSION_TRANSFER_CHUNK_BYTES);
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::BeginLoopContentReplace {
                    generation: 9,
                    loop_id: 1,
                    total_bytes: bytes.len(),
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::WriteLoopContentReplace {
                    generation: 8,
                    offset: 0,
                    bytes: vec![0],
                },
            )
            .event,
            Event::Error { .. }
        ));
        sequence += 1;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::CommitLoopContentReplace { generation: 9 },
            )
            .event,
            Event::Error { .. }
        ));
        sequence += 1;
        for (index, chunk) in bytes.chunks(SESSION_TRANSFER_CHUNK_BYTES).enumerate() {
            assert!(matches!(
                command(
                    &mut host,
                    sequence,
                    Command::WriteLoopContentReplace {
                        generation: 9,
                        offset: index * SESSION_TRANSFER_CHUNK_BYTES,
                        bytes: chunk.to_vec(),
                    },
                )
                .event,
                Event::Ack
            ));
            sequence += 1;
            assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        }
        let Event::Snapshot(before) = command(&mut host, sequence, Command::Poll).event else {
            panic!("expected snapshot")
        };
        assert_eq!(before.loops[0].length, 0);
        sequence += 1;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::CommitLoopContentReplace { generation: 9 },
            )
            .event,
            Event::LoopContentReplaceComplete { generation: 9 }
        ));
        sequence += 1;
        let captured = host.backend.capture_session().unwrap();
        assert_eq!(captured.tracks[0].source_id, 1);
        assert_eq!(captured.tracks[0].loops[0].source_id, 1);
        assert_eq!(
            captured.tracks[0].loops[0].audio[0].samples,
            vec![0.25; 1024]
        );
        assert_eq!(
            captured.tracks[0].loops[0].audio[1].samples,
            vec![0.5; 1024]
        );
        assert_eq!(captured.tracks[0].loops[0].midi[0].events[0].time, 512);
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::TransitionLoop {
                    loop_id: 1,
                    mode: WireLoopMode::Playing,
                    cycles_delay: None,
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        assert!(matches!(
            command(
                &mut host,
                sequence,
                Command::SetLoopLength {
                    loop_id: 1,
                    length: 2048,
                },
            )
            .event,
            Event::Ack
        ));
        sequence += 1;
        assert_no_alloc::assert_no_alloc(|| assert!(host.process(0, 2, 128)));
        let Event::Snapshot(after) = command(&mut host, sequence, Command::Poll).event else {
            panic!("expected snapshot")
        };
        assert!(after.callback_count > before.callback_count);
        assert_eq!(after.loops[0].mode, WireLoopMode::Playing);
        assert_eq!(after.loops[0].length, 2048);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn command_capacity_and_shutdown_fail_visibly() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let oversized = vec![b'x'; COMMAND_MAX_BYTES + 1];
        let response: EventEnvelope = serde_json::from_str(host.handle_json(&oversized)).unwrap();
        assert!(matches!(response.event, Event::Error { .. }));
        assert!(matches!(
            command(&mut host, 1, Command::Shutdown).event,
            Event::Stopped
        ));
        assert!(!host.process(1, 2, 128));
        assert!(matches!(
            command(&mut host, 2, Command::ClearLoop { loop_id: 1 },).event,
            Event::Error { .. }
        ));
    }

    #[shoop_wasm_test_support::shoop_test]
    fn stale_duplicate_and_malformed_commands_are_rejected_observably() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
        let mismatched = serde_json::to_vec(&CommandEnvelope {
            version: PROTOCOL_VERSION.saturating_sub(1),
            sequence: 1,
            command: Command::Poll,
        })
        .unwrap();
        let response: EventEnvelope = serde_json::from_str(host.handle_json(&mismatched)).unwrap();
        assert!(matches!(response.event, Event::Error { .. }));
        assert!(matches!(
            command(&mut host, 2, Command::Poll).event,
            Event::Error { .. }
        ));
        let malformed: EventEnvelope = serde_json::from_str(host.handle_json(b"not json")).unwrap();
        assert!(matches!(malformed.event, Event::Error { .. }));
        assert!(matches!(
            command(&mut host, 1, Command::Poll).event,
            Event::Snapshot(_)
        ));
        assert!(matches!(
            command(&mut host, 1, Command::Poll).event,
            Event::Error { .. }
        ));
    }
}
