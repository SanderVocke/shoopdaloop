use super::*;
use shoop_app_api::CpalAudioDriverConfig;
use shoop_engine::app_backend::{
    AudioChannel, AudioDriver, AudioDriverSettings, AudioPort, BackendSession,
    CpalMidiAudioDriverSettings, DummyAudioDriverSettings, JackAudioDriverSettings, Loop,
    MidiChannel, MidiPort,
};
use shoop_engine::{
    cpal_host_names, cpal_input_device_names_for_host, cpal_output_device_names_for_host,
    midir_input_port_names, midir_output_port_names, AudioDriverType, ChannelMode, MidiEvent,
    PortDirection,
};

pub struct NativeBackend {
    runtime: Option<NativeRuntime>,
    catalog: Arc<[AudioDriverDescriptor]>,
    fatal_error: Option<String>,
}

struct NativeRuntime {
    tracks: BTreeMap<BackendTrackId, NativeTrack>,
    loops: BTreeMap<BackendLoopId, NativeLoop>,
    ports: BTreeMap<BackendPortId, NativePort>,
    next_track_id: u64,
    next_loop_id: u64,
    next_port_id: u64,
    connection_revision: u64,
    connection_failures: Vec<BackendConnectionFailure>,
    configured: AudioDriverConfig,
    resolved: ResolvedAudioDriverConfig,
    driver: AudioDriver,
    session: BackendSession,
}

struct NativeTrack {
    port_name_base: String,
    audio_inputs: Vec<AudioPort>,
    audio_outputs: Vec<AudioPort>,
    midi_input: Option<MidiPort>,
    midi_output: Option<MidiPort>,
    loops: Vec<BackendLoopId>,
    ports: Vec<BackendPortId>,
    state: BackendTrackState,
}

struct NativeLoop {
    handle: Loop,
    audio: Vec<AudioChannel>,
    midi: Vec<MidiChannel>,
    gain: f32,
    balance: f32,
}

struct NativePort {
    descriptor: BackendPortDescriptor,
    handle: NativePortHandle,
}

enum NativePortHandle {
    Audio(AudioPort),
    Midi(MidiPort),
}

impl NativePortHandle {
    fn connections(&self) -> std::collections::HashMap<String, bool> {
        match self {
            Self::Audio(port) => port.get_connections_state(),
            Self::Midi(port) => port.get_connections_state(),
        }
    }

    fn connect(&self, endpoint: &str) {
        match self {
            Self::Audio(port) => port.connect_external_port(endpoint),
            Self::Midi(port) => port.connect_external_port(endpoint),
        }
    }

    fn disconnect(&self, endpoint: &str) {
        match self {
            Self::Audio(port) => port.disconnect_external_port(endpoint),
            Self::Midi(port) => port.disconnect_external_port(endpoint),
        }
    }
}

impl NativeBackend {
    pub fn new(config: AudioDriverConfig) -> Result<Self> {
        let runtime = NativeRuntime::start(config)?;
        let catalog = discover_audio_drivers(&runtime.configured);
        Ok(Self {
            runtime: Some(runtime),
            catalog,
            fatal_error: None,
        })
    }

    pub fn new_with_fallback(config: AudioDriverConfig) -> Result<(Self, Option<String>)> {
        match Self::new(config.clone()) {
            Ok(backend) => Ok((backend, None)),
            Err(preferred_error) => {
                let fallback = AudioDriverConfig::Dummy(DummyAudioDriverConfig::default());
                Self::new(fallback).map(|backend| {
                    (
                        backend,
                        Some(format!(
                            "Could not start preferred {} driver: {preferred_error}; using dummy / offline",
                            config.kind().label()
                        )),
                    )
                })
            }
        }
    }

    fn runtime(&self) -> Result<&NativeRuntime> {
        self.runtime.as_ref().ok_or_else(|| {
            anyhow!(self
                .fatal_error
                .clone()
                .unwrap_or_else(|| "native audio backend is unavailable".to_owned()))
        })
    }

    fn runtime_mut(&mut self) -> Result<&mut NativeRuntime> {
        self.runtime.as_mut().ok_or_else(|| {
            anyhow!(self
                .fatal_error
                .clone()
                .unwrap_or_else(|| "native audio backend is unavailable".to_owned()))
        })
    }

    fn restore_runtime(
        &mut self,
        config: AudioDriverConfig,
        session: &BackendSessionData,
    ) -> Result<(NativeRuntime, BackendSessionReplacement)> {
        let mut runtime = NativeRuntime::start(config)?;
        if runtime.resolved.sample_rate != session.sample_rate {
            return Err(anyhow!(
                "resolved target sample rate changed from {} to {}",
                session.sample_rate,
                runtime.resolved.sample_rate
            ));
        }
        let replacement = runtime.restore_session(session)?;
        Ok((runtime, replacement))
    }
}

impl NativeRuntime {
    fn start(configured: AudioDriverConfig) -> Result<Self> {
        let (driver_type, settings) = engine_driver_settings(&configured)?;
        Self::start_with(configured, driver_type, settings)
    }

    fn start_with(
        configured: AudioDriverConfig,
        driver_type: AudioDriverType,
        settings: AudioDriverSettings,
    ) -> Result<Self> {
        let driver = AudioDriver::new(driver_type, None)?;
        driver.start(&settings)?;
        if configured.kind() == AudioDriverKind::Dummy {
            add_representative_dummy_ports(&driver);
        }
        let session = BackendSession::new()?;
        session.set_audio_driver(&driver)?;
        driver.wait_process();
        let deadline = std::time::Instant::now() + Duration::from_millis(500);
        let state = loop {
            let state = driver.get_state();
            if state.sample_rate > 0 && (state.buffer_size > 0 || state.last_processed > 0) {
                break state;
            }
            if std::time::Instant::now() >= deadline {
                return Err(anyhow!(
                    "audio driver did not resolve a sample rate and buffer size"
                ));
            }
            std::thread::sleep(Duration::from_millis(2));
        };
        let resolved = ResolvedAudioDriverConfig {
            configured: configured.clone(),
            sample_rate: state.sample_rate,
            buffer_size: state.buffer_size.max(state.last_processed),
            instance_name: state.maybe_instance_name,
        };
        Ok(Self {
            tracks: BTreeMap::new(),
            loops: BTreeMap::new(),
            ports: BTreeMap::new(),
            next_track_id: 1,
            next_loop_id: 1,
            next_port_id: 1,
            connection_revision: 1,
            connection_failures: Vec::new(),
            configured,
            resolved,
            driver,
            session,
        })
    }

    fn wait(&self) {
        self.driver.wait_process();
    }

    fn next_port(
        &mut self,
        name: String,
        data_type: BackendPortDataType,
        direction: BackendPortDirection,
        role: BackendPortRole,
        handle: NativePortHandle,
    ) -> BackendPortDescriptor {
        let descriptor = BackendPortDescriptor {
            id: BackendPortId::from_raw(self.next_port_id),
            name,
            data_type,
            direction,
            role,
        };
        self.next_port_id = self.next_port_id.saturating_add(1);
        self.ports.insert(
            descriptor.id,
            NativePort {
                descriptor: descriptor.clone(),
                handle,
            },
        );
        self.connection_revision = self.connection_revision.wrapping_add(1);
        descriptor
    }

    fn create_track_loop(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        let (audio_inputs, audio_outputs, midi_input, midi_output) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?;
            (
                track.audio_inputs.clone(),
                track.audio_outputs.clone(),
                track.midi_input.clone(),
                track.midi_output.clone(),
            )
        };
        let handle = self.session.create_loop()?;
        let mut audio = Vec::with_capacity(audio_inputs.len());
        for (input, output) in audio_inputs.iter().zip(&audio_outputs) {
            let channel = handle.add_audio_channel(ChannelMode::Direct)?;
            channel.connect_input(input)?;
            channel.connect_output(output)?;
            audio.push(channel);
        }
        let mut midi = Vec::new();
        if let (Some(input), Some(output)) = (&midi_input, &midi_output) {
            let channel = handle.add_midi_channel(ChannelMode::Direct)?;
            channel.connect_input(input)?;
            channel.connect_output(output)?;
            midi.push(channel);
        }
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            NativeLoop {
                handle,
                audio,
                midi,
                gain: 1.0,
                balance: 0.0,
            },
        );
        self.tracks
            .get_mut(&track_id)
            .expect("track was checked before loop creation")
            .loops
            .push(id);
        self.wait();
        Ok(id)
    }

    fn connection_snapshot(&self) -> BackendConnectionSnapshot {
        let mut host_ports = self
            .driver
            .find_external_ports(
                None,
                PortDirection::Any as u32,
                shoop_engine::PortDataType::Any as u32,
            )
            .into_iter()
            .filter_map(|port| {
                let direction = match port.direction {
                    PortDirection::Input => BackendPortDirection::Input,
                    PortDirection::Output => BackendPortDirection::Output,
                    PortDirection::Any => return None,
                };
                let data_type = match port.data_type {
                    shoop_engine::PortDataType::Audio => BackendPortDataType::Audio,
                    shoop_engine::PortDataType::Midi => BackendPortDataType::Midi,
                    shoop_engine::PortDataType::Any => return None,
                };
                Some((
                    port.name.clone(),
                    BackendHostPortDescriptor {
                        id: port.name.clone(),
                        name: port.name,
                        data_type,
                        direction,
                    },
                ))
            })
            .collect::<BTreeMap<_, _>>();
        let mut confirmed_links = BTreeSet::new();
        for (id, port) in &self.ports {
            for (endpoint, connected) in port.handle.connections() {
                host_ports
                    .entry(endpoint.clone())
                    .or_insert_with(|| BackendHostPortDescriptor {
                        id: endpoint.clone(),
                        name: endpoint.clone(),
                        data_type: port.descriptor.data_type,
                        direction: opposite_backend_direction(port.descriptor.direction),
                    });
                if connected {
                    confirmed_links.insert(BackendConfirmedLink {
                        application_port_id: *id,
                        host_port_id: endpoint,
                    });
                }
            }
        }
        BackendConnectionSnapshot {
            revision: self.connection_revision,
            available: true,
            application_ports: self
                .ports
                .iter()
                .map(|(id, port)| (*id, port.descriptor.clone()))
                .collect(),
            host_ports,
            confirmed_links,
            failures: self.connection_failures.clone(),
        }
    }

    fn take_connection_snapshot(&mut self) -> BackendConnectionSnapshot {
        let mut snapshot = self.connection_snapshot();
        snapshot.failures = std::mem::take(&mut self.connection_failures);
        snapshot
    }

    fn capture_session(&self) -> Result<BackendSessionData> {
        self.wait();
        let connections = self.connection_snapshot();
        let mut tracks = Vec::with_capacity(self.tracks.len());
        for (track_id, track) in &self.tracks {
            let mut loops = Vec::with_capacity(track.loops.len());
            for loop_id in &track.loops {
                let loop_ = self
                    .loops
                    .get(loop_id)
                    .ok_or_else(|| anyhow!("missing native loop"))?;
                let loop_state = loop_.handle.get_state()?;
                if matches!(
                    loop_state.mode,
                    shoop_engine::LoopMode::Recording
                        | shoop_engine::LoopMode::Replacing
                        | shoop_engine::LoopMode::RecordingDryIntoWet
                ) {
                    return Err(anyhow!("loop content is changing"));
                }
                let audio = loop_
                    .audio
                    .iter()
                    .map(|channel| {
                        let state = channel.get_state()?;
                        Ok(BackendAudioContent {
                            samples: channel.get_data(),
                            gain: state.gain,
                            start_offset: state.start_offset,
                            preplay: state.n_preplay_samples,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                let midi = loop_
                    .midi
                    .iter()
                    .map(|channel| {
                        let state = channel.get_state()?;
                        let data = channel.get_all_midi_data();
                        Ok(BackendMidiContent {
                            length: state.length,
                            start_state: data
                                .iter()
                                .filter(|event| event.time < 0)
                                .map(|event| event.data.clone())
                                .collect(),
                            events: data
                                .iter()
                                .filter(|event| event.time >= 0)
                                .map(|event| BackendMidiEvent {
                                    time: event.time as u32,
                                    data: event.data.clone(),
                                })
                                .collect(),
                            start_offset: state.start_offset,
                            preplay: state.n_preplay_samples,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?;
                loops.push(BackendLoopContent {
                    source_id: loop_id.raw(),
                    length: loop_state.length,
                    gain: loop_.gain,
                    balance: loop_.balance,
                    audio,
                    midi,
                });
            }
            let ports = track
                .ports
                .iter()
                .map(|port_id| {
                    let descriptor = self
                        .ports
                        .get(port_id)
                        .ok_or_else(|| anyhow!("missing native port"))?
                        .descriptor
                        .clone();
                    Ok(BackendSessionPort {
                        source_id: port_id.raw(),
                        descriptor,
                        external_connections: connections
                            .confirmed_links
                            .iter()
                            .filter(|link| link.application_port_id == *port_id)
                            .map(|link| link.host_port_id.clone())
                            .collect(),
                    })
                })
                .collect::<Result<Vec<_>>>()?;
            tracks.push(BackendSessionTrack {
                source_id: track_id.raw(),
                port_name_base: track.port_name_base.clone(),
                state: track.state.clone(),
                loops,
                ports,
                carla_state: None,
            });
        }
        Ok(BackendSessionData {
            sample_rate: self.resolved.sample_rate,
            tracks,
            use_legacy_browser_default_routes: false,
        })
    }

    fn restore_session(&mut self, data: &BackendSessionData) -> Result<BackendSessionReplacement> {
        if !self.tracks.is_empty() {
            return Err(anyhow!("target native session is not empty"));
        }
        if data.tracks.iter().any(|track| track.carla_state.is_some()) {
            return Err(anyhow!("Carla topology is unavailable in this backend"));
        }
        let mut replacement = BackendSessionReplacement::default();
        for source_track in &data.tracks {
            let created = self.create_direct_track(DirectTrackRequest {
                port_name_base: source_track.port_name_base.clone(),
                audio_channels: source_track.state.audio_channels,
                midi: source_track.state.midi,
                initial_loops: source_track.loops.len(),
            })?;
            if created.ports.len() != source_track.ports.len() {
                return Err(anyhow!("prepared native port shape changed"));
            }
            for control in [
                BackendTrackControl::OutputGainDb(source_track.state.output_gain_db),
                BackendTrackControl::OutputBalance(source_track.state.output_balance),
                BackendTrackControl::OutputMute(source_track.state.output_muted),
                BackendTrackControl::InputGainDb(source_track.state.input_gain_db),
                BackendTrackControl::InputBalance(source_track.state.input_balance),
                BackendTrackControl::InputMonitoring(source_track.state.input_monitoring),
            ] {
                self.set_track_control(created.track_id, control)?;
            }
            for (source_loop, loop_id) in source_track.loops.iter().zip(&created.loops) {
                let target = self
                    .loops
                    .get_mut(loop_id)
                    .ok_or_else(|| anyhow!("missing restored native loop"))?;
                if target.audio.len() != source_loop.audio.len()
                    || target.midi.len() != source_loop.midi.len()
                {
                    return Err(anyhow!("prepared native channel shape changed"));
                }
                for (channel, content) in target.audio.iter().zip(&source_loop.audio) {
                    channel.load_data(&content.samples)?;
                    channel.set_gain(content.gain)?;
                    channel.set_start_offset(content.start_offset)?;
                    channel.set_n_preplay_samples(content.preplay)?;
                }
                for (channel, content) in target.midi.iter().zip(&source_loop.midi) {
                    let mut events = content
                        .start_state
                        .iter()
                        .map(|data| MidiEvent {
                            time: -1,
                            data: data.clone(),
                        })
                        .collect::<Vec<_>>();
                    events.extend(content.events.iter().map(|event| MidiEvent {
                        time: event.time as i32,
                        data: event.data.clone(),
                    }));
                    channel.load_midi_data(&events, content.length)?;
                    channel.set_start_offset(content.start_offset)?;
                    channel.set_n_preplay_samples(content.preplay)?;
                }
                target.handle.set_length(source_loop.length)?;
                target.gain = source_loop.gain;
                target.balance = source_loop.balance;
                replacement.loops.insert(source_loop.source_id, *loop_id);
            }
            self.set_loop_controls_for_track(created.track_id)?;
            self.wait();
            let current_connections = self.connection_snapshot();
            for (source_port, created_port) in source_track.ports.iter().zip(&created.ports) {
                replacement
                    .ports
                    .insert(source_port.source_id, created_port.id);
                for external in &source_port.external_connections {
                    if !current_connections.host_ports.contains_key(external) {
                        self.connection_failures.push(BackendConnectionFailure {
                            port_id: created_port.id,
                            external_port: external.clone(),
                            desired_connected: true,
                            message: format!(
                                "external endpoint {external} is unavailable after the audio-driver switch"
                            ),
                        });
                        self.connection_revision = self.connection_revision.wrapping_add(1);
                        continue;
                    }
                    if let Err(error) = self.set_port_connected(created_port.id, external, true) {
                        self.connection_failures.push(BackendConnectionFailure {
                            port_id: created_port.id,
                            external_port: external.clone(),
                            desired_connected: true,
                            message: format!(
                                "could not restore external endpoint {external} after the audio-driver switch: {error}"
                            ),
                        });
                        self.connection_revision = self.connection_revision.wrapping_add(1);
                    }
                }
            }
            replacement.tracks.insert(source_track.source_id, created);
        }
        self.wait();
        Ok(replacement)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        let audio_channels = request.audio_channels as usize;
        let mut audio_inputs = Vec::with_capacity(audio_channels);
        let mut audio_outputs = Vec::with_capacity(audio_channels);
        let mut descriptors = Vec::with_capacity(audio_channels.saturating_mul(2) + 2);
        let ring = self
            .resolved
            .sample_rate
            .saturating_mul(RECORDING_CAPACITY_SECONDS);
        for index in 0..request.audio_channels {
            let suffix = if request.audio_channels == 1 {
                String::new()
            } else {
                format!("_{}", index + 1)
            };
            let input_name = format!("{}_direct_in{suffix}", request.port_name_base);
            let output_name = format!("{}_direct_out{suffix}", request.port_name_base);
            let input = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                ring,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let output = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &output_name,
                &PortDirection::Output,
                self.resolved.buffer_size,
            )?;
            input.connect_internal(&output)?;
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
                NativePortHandle::Audio(input.clone()),
            ));
            descriptors.push(self.next_port(
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
                NativePortHandle::Audio(output.clone()),
            ));
            audio_inputs.push(input);
            audio_outputs.push(output);
        }
        let (midi_input, midi_output) = if request.midi {
            let input_name = format!("{}_direct_midi_in", request.port_name_base);
            let output_name = format!("{}_direct_midi_out", request.port_name_base);
            let input = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                ring,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let output = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &output_name,
                &PortDirection::Output,
                ring,
            )?;
            input.connect_internal(&output)?;
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
                NativePortHandle::Midi(input.clone()),
            ));
            descriptors.push(self.next_port(
                output_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Output,
                BackendPortRole::MidiOutput,
                NativePortHandle::Midi(output.clone()),
            ));
            (Some(input), Some(output))
        } else {
            (None, None)
        };
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            NativeTrack {
                port_name_base: request.port_name_base,
                audio_inputs,
                audio_outputs,
                midi_input,
                midi_output,
                loops: Vec::new(),
                ports: descriptors.iter().map(|port| port.id).collect(),
                state: BackendTrackState {
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                    ..Default::default()
                },
            },
        );
        let mut loops = Vec::with_capacity(request.initial_loops);
        for _ in 0..request.initial_loops {
            loops.push(self.create_track_loop(track_id)?);
        }
        self.wait();
        Ok(BackendTrackCreation {
            track_id,
            loops,
            ports: descriptors,
        })
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?;
        match control {
            BackendTrackControl::OutputGainDb(value) => track.state.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => {
                track.state.output_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::OutputMute(value) => {
                track.state.output_muted = value;
                for port in &track.audio_outputs {
                    port.set_muted(value)?;
                }
                if let Some(port) = &track.midi_output {
                    port.set_muted(value)?;
                }
            }
            BackendTrackControl::InputGainDb(value) => track.state.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => {
                track.state.input_balance = value.clamp(-1.0, 1.0)
            }
            BackendTrackControl::InputMonitoring(value) => {
                track.state.input_monitoring = value;
                for port in &track.audio_inputs {
                    port.set_passthrough_muted(!value)?;
                }
                if let Some(port) = &track.midi_input {
                    port.set_passthrough_muted(!value)?;
                }
            }
        }
        let (left, right) = balance_factors(track.state.output_balance);
        let output_gain = db_gain(track.state.output_gain_db);
        let stereo = track.audio_outputs.len() == 2;
        for (index, port) in track.audio_outputs.iter().enumerate() {
            port.set_gain(output_gain * stereo_factor(stereo, index, left, right))?;
        }
        let (left, right) = balance_factors(track.state.input_balance);
        let input_gain = db_gain(track.state.input_gain_db);
        let stereo = track.audio_inputs.len() == 2;
        for (index, port) in track.audio_inputs.iter().enumerate() {
            port.set_gain(input_gain * stereo_factor(stereo, index, left, right))?;
        }
        Ok(())
    }

    fn set_loop_controls_for_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        let loop_ids = self
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown native track"))?
            .loops
            .clone();
        for loop_id in loop_ids {
            self.apply_loop_controls(loop_id)?;
        }
        Ok(())
    }

    fn apply_loop_controls(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let loop_ = self
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        let (left, right) = balance_factors(loop_.balance);
        let stereo = loop_.audio.len() == 2;
        for (index, channel) in loop_.audio.iter().enumerate() {
            channel.set_gain(loop_.gain * stereo_factor(stereo, index, left, right))?;
        }
        Ok(())
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        endpoint: &str,
        connected: bool,
    ) -> Result<()> {
        if !self.connection_snapshot().host_ports.contains_key(endpoint) {
            return Err(anyhow!("external port disappeared: {endpoint}"));
        }
        let port = self
            .ports
            .get(&port_id)
            .ok_or_else(|| anyhow!("unknown native port {port_id:?}"))?;
        if connected {
            port.handle.connect(endpoint);
        } else {
            port.handle.disconnect(endpoint);
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn audio_driver_state(&mut self) -> Result<AudioDriverRuntimeState> {
        let active = self
            .runtime
            .as_ref()
            .map(|runtime| runtime.resolved.clone());
        Ok(AudioDriverRuntimeState {
            supported: true,
            catalog: Arc::clone(&self.catalog),
            active,
            ..Default::default()
        })
    }

    fn refresh_audio_driver_discovery(
        &mut self,
        config: &AudioDriverConfig,
    ) -> Result<AudioDriverRuntimeState> {
        self.catalog = discover_audio_drivers(config);
        self.audio_driver_state()
    }

    fn preflight_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
    ) -> Result<ResolvedAudioDriverConfig> {
        if config.kind() == AudioDriverKind::WebAudio {
            return Err(anyhow!(
                "Web Audio is selected automatically in browser builds"
            ));
        }
        if let Some(active) = self.runtime.as_ref() {
            match (&active.configured, config) {
                (AudioDriverConfig::Dummy(_), AudioDriverConfig::Dummy(config)) => {
                    if config.sample_rate == 0 || config.buffer_size == 0 {
                        return Err(anyhow!(
                            "dummy sample rate and buffer size must be non-zero"
                        ));
                    }
                    return Ok(ResolvedAudioDriverConfig {
                        configured: AudioDriverConfig::Dummy(config.clone()),
                        sample_rate: config.sample_rate,
                        buffer_size: config.buffer_size,
                        instance_name: "ShoopDaLoop".to_owned(),
                    });
                }
                (AudioDriverConfig::Jack(_), AudioDriverConfig::Jack(config)) => {
                    return Ok(ResolvedAudioDriverConfig {
                        configured: AudioDriverConfig::Jack(config.clone()),
                        sample_rate: active.resolved.sample_rate,
                        buffer_size: active.resolved.buffer_size,
                        instance_name: config.client_name.clone(),
                    });
                }
                (AudioDriverConfig::Cpal(current), AudioDriverConfig::Cpal(config))
                    if current.host == config.host
                        && current.output_device == config.output_device
                        && current.input_device == config.input_device =>
                {
                    validate_cpal(config)?;
                    return Ok(ResolvedAudioDriverConfig {
                        configured: AudioDriverConfig::Cpal(config.clone()),
                        sample_rate: if config.sample_rate == 0 {
                            active.resolved.sample_rate
                        } else {
                            config.sample_rate
                        },
                        buffer_size: if config.buffer_size == 0 {
                            active.resolved.buffer_size
                        } else {
                            config.buffer_size
                        },
                        instance_name: config.output_device.clone(),
                    });
                }
                _ => {}
            }
        }
        let runtime = NativeRuntime::start(config.clone())?;
        Ok(runtime.resolved)
    }

    fn switch_audio_driver(
        &mut self,
        config: &AudioDriverConfig,
        confirmed_sample_rate: u32,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let old_config = self.runtime()?.configured.clone();
        let rollback_session = self.runtime()?.capture_session()?;
        self.runtime.take();
        match self.restore_runtime(config.clone(), session) {
            Ok((runtime, replacement)) => {
                if runtime.resolved.sample_rate != confirmed_sample_rate {
                    let actual = runtime.resolved.sample_rate;
                    drop(runtime);
                    let rollback = self.restore_runtime(old_config, &rollback_session);
                    return match rollback {
                        Ok((runtime, _)) => {
                            self.runtime = Some(runtime);
                            Err(anyhow!(
                                "resolved target sample rate changed from {confirmed_sample_rate} to {actual}"
                            ))
                        }
                        Err(rollback_error) => {
                            self.fatal_error = Some(format!(
                                "target rate changed and restoring the prior driver failed: {rollback_error}"
                            ));
                            Err(anyhow!(self.fatal_error.clone().unwrap()))
                        }
                    };
                }
                self.runtime = Some(runtime);
                self.catalog = discover_audio_drivers(config);
                Ok(replacement)
            }
            Err(target_error) => match self.restore_runtime(old_config, &rollback_session) {
                Ok((runtime, _)) => {
                    self.runtime = Some(runtime);
                    Err(anyhow!("audio-driver switch failed: {target_error}"))
                }
                Err(rollback_error) => {
                    self.fatal_error = Some(format!(
                        "audio-driver switch failed: {target_error}; restoring the prior driver failed: {rollback_error}"
                    ));
                    Err(anyhow!(self.fatal_error.clone().unwrap()))
                }
            },
        }
    }

    fn create_loop(&mut self) -> Result<BackendLoopId> {
        let runtime = self.runtime_mut()?;
        let handle = runtime.session.create_loop()?;
        let id = BackendLoopId::from_raw(runtime.next_loop_id);
        runtime.next_loop_id = runtime.next_loop_id.saturating_add(1);
        runtime.loops.insert(
            id,
            NativeLoop {
                handle,
                audio: Vec::new(),
                midi: Vec::new(),
                gain: 1.0,
                balance: 0.0,
            },
        );
        runtime.wait();
        Ok(id)
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        self.runtime_mut()?.create_direct_track(request)
    }

    fn add_loop_to_track(&mut self, track_id: BackendTrackId) -> Result<BackendLoopId> {
        self.runtime_mut()?.create_track_loop(track_id)
    }

    fn set_track_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackControl,
    ) -> Result<()> {
        self.runtime_mut()?.set_track_control(track_id, control)
    }

    fn set_loop_gain(&mut self, loop_id: BackendLoopId, gain: f32) -> Result<()> {
        let runtime = self.runtime_mut()?;
        runtime
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .gain = gain.clamp(0.0, 1.0);
        runtime.apply_loop_controls(loop_id)
    }

    fn set_loop_balance(&mut self, loop_id: BackendLoopId, balance: f32) -> Result<()> {
        let runtime = self.runtime_mut()?;
        runtime
            .loops
            .get_mut(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .balance = balance.clamp(-1.0, 1.0);
        runtime.apply_loop_controls(loop_id)
    }

    fn grab_loops(&mut self, requests: &[BackendGrabRequest]) -> Result<()> {
        let runtime = self.runtime_mut()?;
        for request in requests {
            if !runtime.loops.contains_key(&request.loop_id) {
                return Err(anyhow!("unknown native loop {:?}", request.loop_id));
            }
        }
        for request in requests {
            let input = runtime
                .tracks
                .values()
                .find(|track| track.loops.contains(&request.loop_id))
                .and_then(|track| track.midi_input.clone());
            let loop_ = &runtime.loops[&request.loop_id];
            if let Some(input) = &input {
                for channel in &loop_.midi {
                    channel.adopt_ringbuffer_contents(
                        input,
                        &loop_.handle,
                        request.reverse_start_cycle,
                        request.cycles_length,
                        request.go_to_cycle,
                        to_native_mode(request.go_to_mode),
                    )?;
                }
            }
            loop_.handle.adopt_ringbuffer_contents(
                request.reverse_start_cycle,
                request.cycles_length,
                request.go_to_cycle,
                to_native_mode(request.go_to_mode),
            )?;
        }
        Ok(())
    }

    fn loop_audio_data(&mut self, loop_id: BackendLoopId) -> Result<Option<Vec<Arc<[f32]>>>> {
        let runtime = self.runtime()?;
        let loop_ = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        Ok(Some(
            loop_
                .audio
                .iter()
                .map(|channel| Arc::from(channel.get_data()))
                .collect(),
        ))
    }

    fn set_loop_sync_source(
        &mut self,
        loop_id: BackendLoopId,
        source: Option<BackendLoopId>,
    ) -> Result<()> {
        let runtime = self.runtime_mut()?;
        let source = source
            .map(|source| {
                runtime
                    .loops
                    .get(&source)
                    .map(|loop_| loop_.handle.clone())
                    .ok_or_else(|| anyhow!("unknown native sync source {source:?}"))
            })
            .transpose()?;
        runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .set_sync_source(source.as_ref())?;
        Ok(())
    }

    fn transition_loop(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
    ) -> Result<()> {
        self.transition_loop_aligned(loop_id, mode, cycles_delay, None)
    }

    fn transition_loop_aligned(
        &mut self,
        loop_id: BackendLoopId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_sync_at: Option<u32>,
    ) -> Result<()> {
        self.runtime()?
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .transition(
                to_native_mode(mode),
                cycles_delay.map(|value| value as i32).unwrap_or(-1),
                align_to_sync_at.map(|value| value as i32).unwrap_or(-1),
            )?;
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        self.runtime()?
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .clear(0)?;
        Ok(())
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        self.runtime()?.capture_session()
    }

    fn replace_session(
        &mut self,
        session: &BackendSessionData,
    ) -> Result<BackendSessionReplacement> {
        let config = self.runtime()?.configured.clone();
        self.switch_audio_driver(&config, session.sample_rate, session)
    }

    fn set_port_connected(
        &mut self,
        port_id: BackendPortId,
        external_port: &str,
        connected: bool,
    ) -> Result<()> {
        self.runtime_mut()?
            .set_port_connected(port_id, external_port, connected)
    }

    fn advance(&mut self, _elapsed: Duration) {}

    fn poll(&mut self) -> Result<BackendSnapshot> {
        let audio_drivers = self.audio_driver_state()?;
        let runtime = self.runtime_mut()?;
        runtime.wait();
        let driver = runtime.driver.get_state();
        let session = runtime.session.get_state();
        let mut tracks = BTreeMap::new();
        for (id, track) in &runtime.tracks {
            let mut state = track.state.clone();
            state.input_peaks = track
                .audio_inputs
                .iter()
                .map(|port| {
                    port.poll_state()
                        .map(|state| amplitude_db(state.input_peak))
                        .unwrap_or(-200.0)
                })
                .collect();
            state.output_peaks = track
                .audio_outputs
                .iter()
                .map(|port| {
                    port.poll_state()
                        .map(|state| amplitude_db(state.output_peak))
                        .unwrap_or(-200.0)
                })
                .collect();
            state.input_midi_activity = track
                .midi_input
                .as_ref()
                .and_then(MidiPort::poll_state)
                .is_some_and(|state| state.n_input_events > 0 || state.n_input_notes_active > 0);
            state.output_midi_activity = track
                .midi_output
                .as_ref()
                .and_then(MidiPort::poll_state)
                .is_some_and(|state| state.n_output_events > 0 || state.n_output_notes_active > 0);
            tracks.insert(*id, state);
        }
        let mut loops = BTreeMap::new();
        for (id, loop_) in &runtime.loops {
            let state = loop_.handle.get_state()?;
            loops.insert(
                *id,
                BackendLoopState {
                    mode: from_native_mode(state.mode),
                    length: state.length,
                    position: state.position,
                    next_mode: state.maybe_next_mode.map(from_native_mode),
                    next_transition_delay: state.maybe_next_mode_delay,
                    stereo: loop_.audio.len() == 2,
                    gain: loop_.gain,
                    balance: loop_.balance,
                    audio_peaks: loop_
                        .audio
                        .iter()
                        .map(|channel| {
                            channel
                                .poll_state()
                                .map(|state| amplitude_db(state.output_peak))
                                .unwrap_or(-200.0)
                        })
                        .collect(),
                    midi_activity: loop_.midi.iter().any(|channel| {
                        channel.poll_state().is_some_and(|state| {
                            state.n_events_triggered > 0 || state.n_notes_active > 0
                        })
                    }),
                },
            );
        }
        Ok(BackendSnapshot {
            status: BackendStatus {
                dsp_load_percent: driver.dsp_load_percent,
                xruns: driver.xruns_since_last,
                buffer_size: driver.buffer_size.max(driver.last_processed),
                sample_rate: driver.sample_rate,
                driver_state: if driver.active == 0 {
                    BackendDriverState::Stopped
                } else if runtime.configured.kind() == AudioDriverKind::Dummy {
                    BackendDriverState::Dummy
                } else {
                    BackendDriverState::Running
                },
                callback_count: u64::from(session.cycles),
                processed_frames: u64::from(session.frames),
                callback_budget_overruns: session.callback_budget_overruns,
                ..Default::default()
            },
            audio_drivers,
            tracks,
            loops,
            connections: runtime.take_connection_snapshot(),
        })
    }

    fn wait_idle(&mut self) {
        if let Some(runtime) = &self.runtime {
            runtime.wait();
        }
    }
}

fn add_representative_dummy_ports(driver: &AudioDriver) {
    for (name, direction, data_type) in [
        (
            "system:capture_1",
            PortDirection::Output,
            shoop_engine::PortDataType::Audio,
        ),
        (
            "system:capture_2",
            PortDirection::Output,
            shoop_engine::PortDataType::Audio,
        ),
        (
            "system:playback_1",
            PortDirection::Input,
            shoop_engine::PortDataType::Audio,
        ),
        (
            "system:playback_2",
            PortDirection::Input,
            shoop_engine::PortDataType::Audio,
        ),
        (
            "controller:midi_out",
            PortDirection::Output,
            shoop_engine::PortDataType::Midi,
        ),
        (
            "synth:midi_in",
            PortDirection::Input,
            shoop_engine::PortDataType::Midi,
        ),
    ] {
        driver.dummy_add_external_mock_port(name, direction as u32, data_type as u32);
    }
}

fn engine_driver_settings(
    config: &AudioDriverConfig,
) -> Result<(AudioDriverType, AudioDriverSettings)> {
    match config {
        AudioDriverConfig::Dummy(config) => {
            if config.sample_rate == 0 || config.buffer_size == 0 {
                return Err(anyhow!(
                    "dummy sample rate and buffer size must be non-zero"
                ));
            }
            Ok((
                AudioDriverType::Dummy,
                AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                    client_name: "ShoopDaLoop".to_owned(),
                    sample_rate: config.sample_rate,
                    buffer_size: config.buffer_size,
                }),
            ))
        }
        AudioDriverConfig::Jack(config) => {
            if config.client_name.trim().is_empty() {
                return Err(anyhow!("JACK client name must not be empty"));
            }
            Ok((
                AudioDriverType::Jack,
                AudioDriverSettings::Jack(JackAudioDriverSettings {
                    client_name_hint: config.client_name.clone(),
                    maybe_server_name: None,
                }),
            ))
        }
        AudioDriverConfig::Cpal(config) => {
            validate_cpal(config)?;
            Ok((
                AudioDriverType::Cpal,
                AudioDriverSettings::Cpal(CpalMidiAudioDriverSettings {
                    client_name: config.client_name.clone(),
                    host: config.host.clone(),
                    output_device: config.output_device.clone(),
                    input_device: config.input_device.clone(),
                    sample_rate: config.sample_rate,
                    buffer_size: config.buffer_size,
                    output_channels: config.output_channels.clone(),
                    input_channels: config.input_channels.clone(),
                    capture_ring_frames: config.capture_ring_frames,
                    midi_inputs: config.midi_inputs.clone(),
                    midi_outputs: config.midi_outputs.clone(),
                }),
            ))
        }
        AudioDriverConfig::WebAudio => Err(anyhow!("Web Audio is unavailable in native builds")),
    }
}

fn validate_cpal(config: &CpalAudioDriverConfig) -> Result<()> {
    if config.client_name.trim().is_empty() {
        return Err(anyhow!("CPAL client name must not be empty"));
    }
    if config.host.trim().is_empty()
        || config.output_device.trim().is_empty()
        || config.input_device.trim().is_empty()
        || config.output_channels.trim().is_empty()
        || config.input_channels.trim().is_empty()
        || config.capture_ring_frames == 0
    {
        return Err(anyhow!("CPAL selectors and capture ring must be non-empty"));
    }
    Ok(())
}

fn discover_audio_drivers(active: &AudioDriverConfig) -> Arc<[AudioDriverDescriptor]> {
    let jack = match probe_jack(active) {
        Ok(()) => AudioDriverDescriptor {
            kind: AudioDriverKind::Jack,
            available: true,
            ..Default::default()
        },
        Err(error) => AudioDriverDescriptor {
            kind: AudioDriverKind::Jack,
            available: false,
            unavailable_reason: Some(error.to_string()),
            ..Default::default()
        },
    };
    let hosts = cpal_host_names();
    let host = match active {
        AudioDriverConfig::Cpal(config) if config.host != "default" => config.host.as_str(),
        _ => "default",
    };
    let input_devices = cpal_input_device_names_for_host(host);
    let output_devices = cpal_output_device_names_for_host(host);
    let cpal_available = !hosts.is_empty() && !output_devices.is_empty();
    let cpal = AudioDriverDescriptor {
        kind: AudioDriverKind::Cpal,
        available: cpal_available,
        unavailable_reason: (!cpal_available)
            .then(|| "No CPAL output device is currently available".to_owned()),
        hosts,
        input_devices,
        output_devices,
        midi_inputs: midir_input_port_names(),
        midi_outputs: midir_output_port_names(),
    };
    Arc::from([
        AudioDriverDescriptor {
            kind: AudioDriverKind::Dummy,
            available: true,
            ..Default::default()
        },
        jack,
        cpal,
    ])
}

fn probe_jack(active: &AudioDriverConfig) -> Result<()> {
    if matches!(active, AudioDriverConfig::Jack(_)) {
        return Ok(());
    }
    let driver = AudioDriver::new(AudioDriverType::Jack, None)?;
    driver.start(&AudioDriverSettings::Jack(JackAudioDriverSettings {
        client_name_hint: "ShoopDaLoop-probe".to_owned(),
        maybe_server_name: None,
    }))
}

fn stereo_factor(stereo: bool, index: usize, left: f32, right: f32) -> f32 {
    if !stereo {
        1.0
    } else if index == 0 {
        left
    } else {
        right
    }
}

fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude > 0.0 {
        20.0 * amplitude.log10()
    } else {
        -200.0
    }
}

fn to_native_mode(mode: BackendLoopMode) -> shoop_engine::LoopMode {
    match mode {
        BackendLoopMode::Unknown => shoop_engine::LoopMode::Unknown,
        BackendLoopMode::Stopped => shoop_engine::LoopMode::Stopped,
        BackendLoopMode::Playing => shoop_engine::LoopMode::Playing,
        BackendLoopMode::Recording => shoop_engine::LoopMode::Recording,
        BackendLoopMode::Replacing => shoop_engine::LoopMode::Replacing,
        BackendLoopMode::PlayingDryThroughWet => shoop_engine::LoopMode::PlayingDryThroughWet,
        BackendLoopMode::RecordingDryIntoWet => shoop_engine::LoopMode::RecordingDryIntoWet,
    }
}

fn from_native_mode(mode: shoop_engine::LoopMode) -> BackendLoopMode {
    match mode {
        shoop_engine::LoopMode::Unknown => BackendLoopMode::Unknown,
        shoop_engine::LoopMode::Stopped => BackendLoopMode::Stopped,
        shoop_engine::LoopMode::Playing => BackendLoopMode::Playing,
        shoop_engine::LoopMode::Recording => BackendLoopMode::Recording,
        shoop_engine::LoopMode::Replacing => BackendLoopMode::Replacing,
        shoop_engine::LoopMode::PlayingDryThroughWet => BackendLoopMode::PlayingDryThroughWet,
        shoop_engine::LoopMode::RecordingDryIntoWet => BackendLoopMode::RecordingDryIntoWet,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_dummy_satisfies_topology_capture_and_same_driver_switch() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        });
        let mut backend = NativeBackend::new(config.clone()).unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "native".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        backend.set_loop_gain(created.loops[0], 0.5).unwrap();
        backend.set_loop_balance(created.loops[0], -0.25).unwrap();
        let input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap();
        assert!(backend
            .poll()
            .unwrap()
            .connections
            .host_ports
            .contains_key("system:capture_1"));
        backend
            .set_port_connected(input.id, "system:capture_1", true)
            .unwrap();
        let mut captured = backend.capture_session().unwrap();
        captured.tracks[0].loops[0].length = 512;
        captured.tracks[0].loops[0].audio[0].samples = vec![0.25, -0.5, 0.75];
        captured.tracks[0].loops[0].midi[0] = BackendMidiContent {
            length: 512,
            start_state: vec![vec![0xB0, 7, 99]],
            events: vec![BackendMidiEvent {
                time: 100,
                data: vec![0x90, 60, 100],
            }],
            start_offset: -4,
            preplay: 8,
        };
        captured.tracks[0]
            .ports
            .iter_mut()
            .find(|port| port.source_id == input.id.raw())
            .unwrap()
            .external_connections
            .push("removed:stale_capture".to_owned());
        let mapping = backend
            .switch_audio_driver(&config, 48_000, &captured)
            .unwrap();
        assert_eq!(mapping.tracks.len(), captured.tracks.len());
        assert_eq!(mapping.loops.len(), 2);
        let restored = backend.capture_session().unwrap();
        assert_eq!(restored.sample_rate, 48_000);
        assert_eq!(restored.tracks.len(), captured.tracks.len());
        assert_eq!(restored.tracks[0].loops[0].gain, 0.5);
        assert_eq!(restored.tracks[0].loops[0].balance, -0.25);
        assert_eq!(
            restored.tracks[0].loops[0].audio[0].samples,
            vec![0.25, -0.5, 0.75]
        );
        assert_eq!(restored.tracks[0].loops[0].midi[0].length, 512);
        assert_eq!(restored.tracks[0].loops[0].midi[0].events[0].time, 100);
        assert_eq!(restored.tracks[0].loops[0].midi[0].start_offset, -4);
        assert!(restored.tracks[0]
            .ports
            .iter()
            .any(|port| port.external_connections == ["system:capture_1"]));
        let failures = backend.poll().unwrap().connections.failures;
        assert_eq!(failures.len(), 1);
        assert_eq!(failures[0].external_port, "removed:stale_capture");
        assert!(failures[0].desired_connected);
        assert!(failures[0].message.contains("unavailable after"));
        assert!(backend.poll().unwrap().connections.failures.is_empty());
    }

    #[test]
    fn native_jack_test_adapter_publishes_driver_ports() {
        let configured = AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig {
            client_name: "ShoopDaLoop-test".to_owned(),
        });
        let runtime = NativeRuntime::start_with(
            configured,
            AudioDriverType::JackTest,
            AudioDriverSettings::Dummy(DummyAudioDriverSettings {
                client_name: "ShoopDaLoop-test".to_owned(),
                sample_rate: 48_000,
                buffer_size: 128,
            }),
        )
        .unwrap();
        let mut backend = NativeBackend {
            runtime: Some(runtime),
            catalog: Arc::from([]),
            fatal_error: None,
        };
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "jack_test".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        assert!(backend
            .poll()
            .unwrap()
            .connections
            .host_ports
            .keys()
            .any(|name| name.starts_with("test_client_1:")));
    }

    #[test]
    fn native_cpal_test_adapter_satisfies_topology_and_status_contract() {
        let configured = AudioDriverConfig::Cpal(CpalAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
            input_device: "none".to_owned(),
            midi_inputs: vec!["none".to_owned()],
            midi_outputs: vec!["none".to_owned()],
            ..Default::default()
        });
        let (_, settings) = engine_driver_settings(&configured).unwrap();
        let runtime =
            NativeRuntime::start_with(configured, AudioDriverType::CpalTest, settings).unwrap();
        let mut backend = NativeBackend {
            runtime: Some(runtime),
            catalog: Arc::from([]),
            fatal_error: None,
        };
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "cpal_test".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        assert_eq!(created.ports.len(), 6);
        assert_eq!(backend.capture_session().unwrap().tracks.len(), 1);
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.status.sample_rate, 48_000);
        assert_eq!(snapshot.status.buffer_size, 128);
        assert!(snapshot.tracks[&created.track_id].midi);
    }

    #[test]
    fn repeated_same_driver_switches_release_each_previous_runtime() {
        let mut backend = NativeBackend::new(AudioDriverConfig::default()).unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "repeated".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        for index in 0..8 {
            let target = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                sample_rate: 48_000,
                buffer_size: 64 + index * 16,
            });
            let session = backend.capture_session().unwrap();
            backend
                .switch_audio_driver(&target, 48_000, &session)
                .unwrap();
        }
        assert_eq!(backend.capture_session().unwrap().tracks.len(), 1);
    }

    #[test]
    fn native_dummy_rejects_changed_rate_without_converted_session() {
        let mut backend = NativeBackend::new(AudioDriverConfig::default()).unwrap();
        let captured = backend.capture_session().unwrap();
        let target = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 44_100,
            buffer_size: 256,
        });
        assert!(backend
            .switch_audio_driver(&target, 44_100, &captured)
            .is_err());
        assert_eq!(backend.capture_session().unwrap().sample_rate, 48_000);
    }

    #[test]
    fn optional_real_cpal_smoke_reports_environment_skip() {
        if std::env::var_os("SHOOP_RUN_REAL_AUDIO_SMOKE").is_none() {
            eprintln!("skipped real CPAL smoke: set SHOOP_RUN_REAL_AUDIO_SMOKE=1");
            return;
        }
        let config = AudioDriverConfig::Cpal(CpalAudioDriverConfig {
            input_device: "none".to_owned(),
            midi_inputs: vec!["none".to_owned()],
            midi_outputs: vec!["none".to_owned()],
            ..Default::default()
        });
        match NativeBackend::new(config) {
            Ok(mut backend) => {
                let snapshot = backend.poll().unwrap();
                assert!(snapshot.status.sample_rate > 0);
                assert!(snapshot.status.buffer_size > 0);
            }
            Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
                eprintln!("skipped real CPAL smoke: {error}");
            }
            Err(error) => panic!("real CPAL smoke failed: {error}"),
        }
    }

    #[test]
    fn optional_real_cross_driver_switch_reports_each_environment_skip() {
        if std::env::var_os("SHOOP_RUN_REAL_AUDIO_SMOKE").is_none() {
            eprintln!("skipped real cross-driver switch: set SHOOP_RUN_REAL_AUDIO_SMOKE=1");
            return;
        }
        let mut backend = NativeBackend::new(AudioDriverConfig::default()).unwrap();
        backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "real_switch".to_owned(),
                audio_channels: 1,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let targets = [
            AudioDriverConfig::Cpal(CpalAudioDriverConfig {
                input_device: "none".to_owned(),
                midi_inputs: vec!["none".to_owned()],
                midi_outputs: vec!["none".to_owned()],
                ..Default::default()
            }),
            AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig::default()),
            AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig {
                client_name: "ShoopDaLoop-variant".to_owned(),
            }),
        ];
        for target in targets {
            let resolved = match backend.preflight_audio_driver(&target) {
                Ok(resolved) => resolved,
                Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
                    eprintln!("skipped {} switch: {error}", target.kind().label());
                    continue;
                }
                Err(error) => panic!("{} preflight failed: {error}", target.kind().label()),
            };
            let mut session = backend.capture_session().unwrap();
            session.sample_rate = resolved.sample_rate;
            match backend.switch_audio_driver(&target, resolved.sample_rate, &session) {
                Ok(_) => {
                    assert_eq!(
                        backend
                            .audio_driver_state()
                            .unwrap()
                            .active
                            .unwrap()
                            .configured
                            .kind(),
                        target.kind()
                    );
                    eprintln!("switched to {}", target.kind().label());
                }
                Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
                    eprintln!("skipped {} switch: {error}", target.kind().label());
                }
                Err(error) => panic!("{} switch failed: {error}", target.kind().label()),
            }
        }
    }

    #[test]
    fn optional_real_jack_smoke_reports_environment_skip() {
        if std::env::var_os("SHOOP_RUN_REAL_AUDIO_SMOKE").is_none() {
            eprintln!("skipped real JACK smoke: set SHOOP_RUN_REAL_AUDIO_SMOKE=1");
            return;
        }
        let config = AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig::default());
        match NativeBackend::new(config) {
            Ok(mut backend) => {
                let snapshot = backend.poll().unwrap();
                assert!(snapshot.status.sample_rate > 0);
                assert!(snapshot.status.buffer_size > 0);
            }
            Err(error) if std::env::var_os("SHOOP_ALLOW_MISSING_BACKENDS").is_some() => {
                eprintln!("skipped real JACK smoke: {error}");
            }
            Err(error) => panic!("real JACK smoke failed: {error}"),
        }
    }

    #[test]
    fn invalid_preferred_driver_falls_back_without_changing_the_preference_value() {
        let preferred = AudioDriverConfig::Jack(shoop_app_api::JackAudioDriverConfig {
            client_name: String::new(),
        });
        let (mut backend, warning) = NativeBackend::new_with_fallback(preferred.clone()).unwrap();
        assert!(warning.unwrap().contains("preferred JACK"));
        assert_eq!(
            backend
                .audio_driver_state()
                .unwrap()
                .active
                .unwrap()
                .configured
                .kind(),
            AudioDriverKind::Dummy
        );
        assert_eq!(preferred.kind(), AudioDriverKind::Jack);
    }

    #[test]
    fn production_catalog_never_exposes_test_drivers() {
        let backend = NativeBackend::new(AudioDriverConfig::default()).unwrap();
        assert_eq!(backend.catalog.len(), 3);
        assert_eq!(backend.catalog[0].kind, AudioDriverKind::Dummy);
        assert_eq!(backend.catalog[1].kind, AudioDriverKind::Jack);
        assert_eq!(backend.catalog[2].kind, AudioDriverKind::Cpal);
    }
}
