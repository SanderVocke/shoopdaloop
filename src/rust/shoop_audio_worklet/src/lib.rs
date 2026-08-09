use shoop_audio_protocol::{
    Command, CommandEnvelope, Event, EventEnvelope, WaveformChunk, WireApplicationPort,
    WireConfirmedLink, WireHostPort, WireLoopMode, WireLoopState, WireMidiOutputEvent,
    WirePortDataType, WirePortDirection, WirePortRole, WireSnapshot, WireTrackControl,
    WireTrackState, COMMAND_MAX_BYTES, MAX_DEVICE_AUDIO_CHANNELS, MIDI_BATCH_CAPACITY,
    PROTOCOL_VERSION, SESSION_TRANSFER_CHUNK_BYTES, SESSION_TRANSFER_MAX_BYTES,
    TRACK_MIDI_MESSAGE_BYTES, WAVEFORM_CHUNK_SAMPLES,
};
use shoop_backend::{
    Backend, BackendGrabRequest, BackendHostPortDescriptor, BackendLoopId, BackendLoopMode,
    BackendPortDataType, BackendPortDirection, BackendPortId, BackendPortRole, BackendSessionData,
    BackendSnapshot, BackendTrackControl, BackendTrackId, DirectTrackRequest, EngineBackend,
    MAX_WEB_AUDIO_QUANTUM,
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
            r#"{"version":4,"sequence":0,"event":{"kind":"error","message":"response serialization failed"}}"#
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
                audio_channels,
                midi,
            } => {
                let created = self
                    .backend
                    .create_direct_track(DirectTrackRequest {
                        port_name_base,
                        audio_channels,
                        midi,
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
            Command::SetTrackControl { track_id, control } => {
                self.backend
                    .set_track_control(
                        BackendTrackId::from_raw(track_id),
                        from_wire_track_control(control),
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
                    final_chunk: chunk.offset.saturating_add(chunk.samples.len())
                        >= chunk.total_samples,
                    samples: chunk.samples,
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
        command_overflows: snapshot.status.command_overflows,
        storage_low_channels: snapshot.status.storage_low_channels,
        storage_exhaustions: snapshot.status.storage_exhaustions,
        tracks: snapshot
            .tracks
            .into_iter()
            .map(|(id, track)| WireTrackState {
                id: id.raw(),
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

    #[test]
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
                audio_channels: 1,
                midi: false,
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

    #[test]
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
                    audio_channels: 2,
                    midi: false,
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
        assert_eq!(snapshot.application_ports.len(), 4);
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

    #[test]
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
                    audio_channels: 0,
                    midi: true,
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

    #[test]
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
                    audio_channels: 4,
                    midi: true,
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
        let Event::Snapshot(snapshot) = command(&mut host, sequence, Command::Poll).event else {
            panic!("expected snapshot")
        };
        assert_eq!(snapshot.loops[0].length, 4);
    }

    #[test]
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

    #[test]
    fn stale_duplicate_and_malformed_commands_are_rejected_observably() {
        let mut host = WorkletHost::new(48_000, 128).unwrap();
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
