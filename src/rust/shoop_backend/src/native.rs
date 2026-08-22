use super::*;
use shoop_app_api::{
    CpalAudioDriverConfig, FxGenerationLogState, TrackProcessorConstraints, TrackProcessorFeatures,
    TrackProcessorMidiPolicy,
};
use shoop_engine::app_backend::{
    AudioChannel, AudioDriver, AudioDriverSettings, AudioPort, BackendSession, CompositeLoop,
    CpalMidiAudioDriverSettings, DummyAudioDriverSettings, FXChain, JackAudioDriverSettings, Loop,
    MidiChannel, MidiPort,
};
use shoop_engine::{
    cpal_host_names, cpal_input_device_names_for_host, cpal_output_device_names_for_host,
    midir_input_port_names, midir_output_port_names, AudioDriverType, ChannelMode, FXChainType,
    MidiEvent, PortDirection,
};

fn app_oxisynth_state(
    snapshot: shoop_engine::oxisynth::OxiSynthSnapshot,
    metadata: shoop_engine::app_backend::OxiSynthAssetMetadata,
    available_soundfonts: Arc<[shoop_app_api::SoundFontAssetDescriptor]>,
) -> TrackProcessorEditorState {
    TrackProcessorEditorState::OxiSynth(shoop_app_api::OxiSynthState {
        available_soundfonts,
        soundfont_sha256: metadata.sha256.into(),
        soundfont_name: metadata.name.into(),
        presets: metadata
            .presets
            .iter()
            .map(|preset| shoop_app_api::OxiSynthPresetDescriptor {
                bank: preset.bank,
                program: preset.program,
                name: preset.name.clone().into(),
            })
            .collect::<Vec<_>>()
            .into(),
        revision: snapshot.revision,
        midi_activity_revision: snapshot.midi_activity_revision,
        master_gain: snapshot.master_gain,
        reverb: shoop_app_api::OxiSynthReverbState {
            room_size: snapshot.reverb.room_size,
            damp: snapshot.reverb.damp,
            width: snapshot.reverb.width,
            level: snapshot.reverb.level,
        },
        chorus: shoop_app_api::OxiSynthChorusState {
            voices: snapshot.chorus.voices,
            level: snapshot.chorus.level,
            speed_hz: snapshot.chorus.speed_hz,
            depth_ms: snapshot.chorus.depth_ms,
        },
        channels: snapshot
            .channels
            .map(|channel| shoop_app_api::OxiSynthChannelState {
                baseline_bank: channel.baseline_bank,
                baseline_program: channel.baseline_program,
                current_bank: channel.bank,
                current_program: channel.program,
                volume: channel.controllers[7],
                pan: channel.controllers[10],
                expression: channel.controllers[11],
                pitch_bend: channel.pitch_bend,
                channel_pressure: channel.channel_pressure,
            }),
    })
}

#[cfg(feature = "native-fx")]
pub fn configure_carla_hosting_mode(mode: shoop_settings::CarlaHostingMode) {
    shoop_engine::app_backend::set_carla_hosting_mode(mode);
}

#[cfg(feature = "native-fx")]
pub fn configured_carla_hosting_mode() -> shoop_settings::CarlaHostingMode {
    shoop_engine::app_backend::carla_hosting_mode()
}

#[cfg(feature = "native-fx")]
pub fn smoke_test_carla_runtime() -> Result<()> {
    shoop_engine::carla_native::smoke_test_carla_runtime()
}

#[cfg(feature = "native-fx")]
pub fn smoke_test_carla_ui() -> Result<()> {
    shoop_engine::carla_native::smoke_test_carla_ui()
}

#[cfg(feature = "native-fx")]
pub fn carla_runtime_path() -> Result<std::path::PathBuf> {
    shoop_engine::carla_native::carla_runtime_path()
}

#[cfg(feature = "native-fx")]
pub fn run_carla_worker_if_requested<I, S>(args: I) -> Result<bool>
where
    I: IntoIterator<Item = S>,
    S: AsRef<std::ffi::OsStr>,
{
    let args = args
        .into_iter()
        .map(|value| {
            value
                .as_ref()
                .to_str()
                .map(str::to_owned)
                .ok_or_else(|| anyhow!("Carla worker arguments must be valid UTF-8"))
        })
        .collect::<Result<Vec<_>>>()?;
    if !args.iter().any(|argument| argument == "--carla-worker") {
        return Ok(false);
    }
    let value = |name: &str| -> Result<String> {
        let prefix = format!("{name}=");
        for (index, argument) in args.iter().enumerate() {
            if let Some(value) = argument.strip_prefix(&prefix) {
                return Ok(value.to_owned());
            }
            if argument == name {
                return args
                    .get(index + 1)
                    .cloned()
                    .ok_or_else(|| anyhow!("missing value for {name}"));
            }
        }
        Err(anyhow!("missing Carla worker argument {name}"))
    };
    let nonce = value("--carla-worker-nonce")?;
    if nonce.len() != 64 {
        return Err(anyhow!("Carla worker nonce must contain 64 hex digits"));
    }
    let mut decoded_nonce = [0_u8; 32];
    for (index, byte) in decoded_nonce.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&nonce[index * 2..index * 2 + 2], 16)
            .map_err(|_| anyhow!("Carla worker nonce is not hexadecimal"))?;
    }
    let test_mode = args
        .iter()
        .position(|argument| {
            argument == "--carla-worker-test-mode"
                || argument.starts_with("--carla-worker-test-mode=")
        })
        .map(|_| value("--carla-worker-test-mode"))
        .transpose()?
        .map(|value| value.parse())
        .transpose()?;
    shoop_engine::carla_subprocess::run_carla_worker(
        shoop_engine::carla_subprocess::CarlaWorkerOptions {
            address: value("--carla-worker-address")?
                .parse()
                .map_err(|error| anyhow!("invalid Carla worker address: {error}"))?,
            nonce: decoded_nonce,
            chain_id: shoop_plugin_protocol::ChainId(value("--carla-worker-chain-id")?.parse()?),
            generation: shoop_plugin_protocol::ProcessGeneration(
                value("--carla-worker-generation")?.parse()?,
            ),
            shared_memory_path: value("--carla-worker-shared-memory")?.into(),
            test_mode,
        },
    )?;
    Ok(true)
}

pub struct NativeBackend {
    runtime: Option<NativeRuntime>,
    catalog: Arc<[AudioDriverDescriptor]>,
    fatal_error: Option<String>,
    soundfonts: soundfont_library::SoundFontLibrary,
    soundfont_directory: std::path::PathBuf,
}

struct NativeRuntime {
    tracks: BTreeMap<BackendTrackId, NativeTrack>,
    global_fx_port: BackendPortId,
    loops: BTreeMap<BackendLoopId, NativeLoop>,
    composites: BTreeMap<BackendCompositeId, NativeComposite>,
    ports: BTreeMap<BackendPortId, NativePort>,
    next_track_id: u64,
    next_loop_id: u64,
    next_composite_id: u64,
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
    audio_sends: Vec<Option<AudioPort>>,
    audio_returns: Vec<Option<AudioPort>>,
    midi_input: Option<MidiPort>,
    midi_output: Option<MidiPort>,
    loops: Vec<BackendLoopId>,
    ports: Vec<BackendPortId>,
    state: BackendTrackState,
    dry_passthrough_muted: Option<bool>,
    wet_passthrough_muted: Option<bool>,
    fx: Option<NativeFx>,
}

struct NativeFx {
    processor_type: TrackProcessorTypeId,
    chain: FXChain,
    active: bool,
    last_confirmed_state: Option<String>,
}

struct NativeComposite {
    handle: CompositeLoop,
    config: Option<BackendCompositeConfig>,
}

struct NativeLoop {
    handle: Loop,
    audio: Vec<AudioChannel>,
    audio_modes: Vec<BackendChannelMode>,
    midi: Vec<MidiChannel>,
    midi_modes: Vec<BackendChannelMode>,
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

    fn connections_now(&self) -> std::collections::HashMap<String, bool> {
        match self {
            Self::Audio(port) => port.get_connections_state_now(),
            Self::Midi(port) => port.get_connections_state_now(),
        }
    }

    fn wait_ready(&self, session: &BackendSession) -> Result<()> {
        let sequence = match self {
            Self::Audio(port) => port.creation_sequence(),
            Self::Midi(port) => port.creation_sequence(),
        };
        session.wait_for_command(sequence, shoop_engine::DEFAULT_WAIT_TIMEOUT)?;
        let (lifecycle, error, kind) = match self {
            Self::Audio(port) => (port.lifecycle(), port.creation_error(), "audio"),
            Self::Midi(port) => (port.lifecycle(), port.creation_error(), "MIDI"),
        };
        match lifecycle {
            shoop_engine::app_backend::ObjectLifecycle::Ready => Ok(()),
            shoop_engine::app_backend::ObjectLifecycle::Failed => Err(anyhow!(
                "{kind} port creation failed: {}",
                error.unwrap_or_else(|| "unknown error".to_owned())
            )),
            shoop_engine::app_backend::ObjectLifecycle::Pending => {
                Err(anyhow!("{kind} port is still pending creation"))
            }
            shoop_engine::app_backend::ObjectLifecycle::Closed => {
                Err(anyhow!("{kind} port is closed"))
            }
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
        let soundfont_directory = shoop_settings::default_settings_path()?
            .parent()
            .ok_or_else(|| anyhow!("settings path has no parent directory"))?
            .join("soundfonts");
        let soundfonts = soundfont_library::SoundFontLibrary::load_directory(&soundfont_directory)?;
        Ok(Self {
            runtime: Some(runtime),
            catalog,
            fatal_error: None,
            soundfonts,
            soundfont_directory,
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
        for (digest, asset) in &session.soundfonts {
            let imported = self
                .soundfonts
                .import(asset.bytes.clone().into(), asset.original_filename.clone())?;
            if imported.sha256.as_ref() != digest {
                return Err(anyhow!("session SoundFont digest mismatch"));
            }
        }
        if runtime.resolved.sample_rate != session.sample_rate {
            return Err(anyhow!(
                "resolved target sample rate changed from {} to {}",
                session.sample_rate,
                runtime.resolved.sample_rate
            ));
        }
        let replacement = runtime.restore_session(session, &self.soundfonts)?;
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
        let global_fx_midi = MidiPort::new_driver_port(
            &session,
            &driver,
            "global_fx_control_midi_in",
            &PortDirection::Input,
            0,
        )?;
        session.set_global_fx_midi_input(&global_fx_midi)?;
        let global_fx_port = BackendPortId::from_raw(9_007_199_254_740_991);
        let global_descriptor = BackendPortDescriptor {
            id: global_fx_port,
            owner: BackendPortOwner::GlobalFxControl,
            name: "Global FX Control MIDI In".to_owned(),
            data_type: BackendPortDataType::Midi,
            direction: BackendPortDirection::Input,
            role: BackendPortRole::MidiInput,
        };
        Ok(Self {
            tracks: BTreeMap::new(),
            global_fx_port,
            loops: BTreeMap::new(),
            composites: BTreeMap::new(),
            ports: BTreeMap::from([(
                global_fx_port,
                NativePort {
                    descriptor: global_descriptor,
                    handle: NativePortHandle::Midi(global_fx_midi),
                },
            )]),
            next_track_id: 1,
            next_loop_id: 1,
            next_composite_id: 1,
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

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        let Some(track) = self.tracks.remove(&track_id) else {
            return Ok(());
        };
        for loop_id in &track.loops {
            if let Some(loop_) = self.loops.remove(loop_id) {
                self.session.remove_loop(&loop_.handle)?;
            }
        }
        if let Some(fx) = &track.fx {
            self.session.remove_fx_chain(&fx.chain)?;
        } else {
            self.session.remove_processor(&track.port_name_base)?;
        }
        for port_id in &track.ports {
            let Some(port) = self.ports.remove(port_id) else {
                continue;
            };
            match port.handle {
                NativePortHandle::Audio(port) => {
                    self.driver.unregister_audio_port(&port)?;
                    self.session.remove_audio_port(&port)?;
                }
                NativePortHandle::Midi(port) => {
                    self.driver.unregister_midi_port(&port)?;
                    self.session.remove_midi_port(&port)?;
                }
            }
        }
        self.connection_failures
            .retain(|failure| !track.ports.contains(&failure.port_id));
        self.connection_revision = self.connection_revision.wrapping_add(1);
        self.wait();
        Ok(())
    }

    fn composite_target_identity(
        &self,
        target: BackendCompositeTarget,
    ) -> Result<shoop_engine::LoopIdentity> {
        match target {
            BackendCompositeTarget::Loop(id) => self
                .loops
                .get(&id)
                .map(|loop_| loop_.handle.identity())
                .ok_or_else(|| anyhow!("stale composite loop target {id:?}")),
            BackendCompositeTarget::Composite(id) => self
                .composites
                .get(&id)
                .map(|composite| composite.handle.identity())
                .ok_or_else(|| anyhow!("stale composite target {id:?}")),
        }
    }

    fn backend_composite_target(
        &self,
        identity: shoop_engine::LoopIdentity,
    ) -> Option<BackendCompositeTarget> {
        match identity.kind {
            shoop_engine::LoopTargetKind::Basic => self.loops.iter().find_map(|(id, loop_)| {
                (loop_.handle.identity() == identity).then_some(BackendCompositeTarget::Loop(*id))
            }),
            shoop_engine::LoopTargetKind::Composite => {
                self.composites.iter().find_map(|(id, composite)| {
                    (composite.handle.identity() == identity)
                        .then_some(BackendCompositeTarget::Composite(*id))
                })
            }
        }
    }

    fn configure_composite(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        let composite = self
            .composites
            .get(&composite_id)
            .ok_or_else(|| anyhow!("unknown native composite {composite_id:?}"))?;
        let source = composite.handle.identity();
        let sync = self
            .loops
            .get(&config.sync_source)
            .ok_or_else(|| anyhow!("stale composite sync source"))?;
        let sync_identity = sync.handle.identity();
        let sync_length = u64::from(sync.handle.get_state()?.length).max(1);
        let timelines = config
            .timelines
            .iter()
            .map(|sections| {
                Ok(shoop_engine::CompositeTimeline {
                    sections: sections
                        .iter()
                        .map(|entries| {
                            Ok(shoop_engine::CompositeSection {
                                entries: entries
                                    .iter()
                                    .map(|entry| {
                                        Ok(shoop_engine::CompositeEntry {
                                            target: self.composite_target_identity(entry.target)?,
                                            delay: entry.delay,
                                            n_cycles: entry.n_cycles,
                                            mode: entry.mode.map(to_native_mode),
                                        })
                                    })
                                    .collect::<Result<Vec<_>>>()?,
                            })
                        })
                        .collect::<Result<Vec<_>>>()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let descriptor = shoop_engine::CompositePlanDescriptor {
            source,
            sync_length,
            timelines,
        };
        let mut metadata = Vec::with_capacity(self.loops.len() + self.composites.len());
        for loop_ in self.loops.values() {
            let state = loop_.handle.get_state()?;
            metadata.push(shoop_engine::LoopTargetMetadata {
                identity: loop_.handle.identity(),
                length_samples: u64::from(state.length),
            });
        }
        for candidate in self.composites.values() {
            metadata.push(shoop_engine::LoopTargetMetadata {
                identity: candidate.handle.identity(),
                length_samples: candidate
                    .handle
                    .poll_state()
                    .map(|state| state.length)
                    .unwrap_or(0),
            });
        }
        let primitive_sync_sources = self.session.primitive_sync_sources();
        self.session.configure_composite_loop(
            &composite.handle,
            descriptor,
            sync_identity,
            metadata,
            &primitive_sync_sources,
        )?;
        self.composites.get_mut(&composite_id).unwrap().config = Some(config.clone());
        Ok(())
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
            owner: BackendPortOwner::Track,
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
        let (
            topology,
            audio_inputs,
            audio_outputs,
            audio_sends,
            audio_returns,
            midi_input,
            midi_output,
        ) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?;
            (
                track.state.topology.clone(),
                track.audio_inputs.clone(),
                track.audio_outputs.clone(),
                track.audio_sends.clone(),
                track.audio_returns.clone(),
                track.midi_input.clone(),
                track.midi_output.clone(),
            )
        };
        let handle = self.session.create_loop()?;
        let mut audio = Vec::new();
        let mut audio_modes = Vec::new();
        let mut midi = Vec::new();
        let mut midi_modes = Vec::new();
        match topology {
            BackendTrackTopology::Direct { .. } => {
                for (input, output) in audio_inputs.iter().zip(&audio_outputs) {
                    let channel = handle.add_audio_channel(ChannelMode::Direct)?;
                    channel.connect_input(input)?;
                    channel.connect_output(output)?;
                    audio.push(channel);
                    audio_modes.push(BackendChannelMode::Direct);
                }
                if let (Some(input), Some(output)) = (&midi_input, &midi_output) {
                    let channel = handle.add_midi_channel(ChannelMode::Direct)?;
                    channel.connect_input(input)?;
                    channel.connect_output(output)?;
                    midi.push(channel);
                    midi_modes.push(BackendChannelMode::Direct);
                }
            }
            BackendTrackTopology::DryWetExternal { .. }
            | BackendTrackTopology::DryWetProcessor { .. } => {
                for (index, input) in audio_inputs.iter().enumerate() {
                    let channel = handle.add_audio_channel(ChannelMode::Dry)?;
                    channel.connect_input(input)?;
                    if let Some(send) = audio_sends.get(index).and_then(Option::as_ref) {
                        channel.connect_output(send)?;
                    }
                    audio.push(channel);
                    audio_modes.push(BackendChannelMode::Dry);
                }
                for (index, output) in audio_outputs.iter().enumerate() {
                    let channel = handle.add_audio_channel(ChannelMode::Wet)?;
                    if let Some(return_) = audio_returns.get(index).and_then(Option::as_ref) {
                        channel.connect_input(return_)?;
                    }
                    channel.connect_output(output)?;
                    audio.push(channel);
                    audio_modes.push(BackendChannelMode::Wet);
                }
                if let (Some(input), Some(send)) = (&midi_input, &midi_output) {
                    let channel = handle.add_midi_channel(ChannelMode::Dry)?;
                    channel.connect_input(input)?;
                    channel.connect_output(send)?;
                    midi.push(channel);
                    midi_modes.push(BackendChannelMode::Dry);
                }
            }
        }
        let id = BackendLoopId::from_raw(self.next_loop_id);
        self.next_loop_id = self.next_loop_id.saturating_add(1);
        self.loops.insert(
            id,
            NativeLoop {
                handle,
                audio,
                audio_modes,
                midi,
                midi_modes,
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
        self.apply_track_routing(track_id)?;
        Ok(id)
    }

    fn connection_snapshot(&self) -> BackendConnectionSnapshot {
        self.connection_snapshot_with(false)
    }

    fn connection_snapshot_now(&self) -> BackendConnectionSnapshot {
        self.connection_snapshot_with(true)
    }

    fn connection_snapshot_with(&self, authoritative: bool) -> BackendConnectionSnapshot {
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
            let connections = if authoritative {
                port.handle.connections_now()
            } else {
                port.handle.connections()
            };
            for (endpoint, connected) in connections {
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

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        self.wait();
        let connections = self.connection_snapshot_now();
        let mut tracks = Vec::with_capacity(self.tracks.len());
        for (track_id, track) in &mut self.tracks {
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
                    .zip(&loop_.audio_modes)
                    .map(|(channel, mode)| {
                        let state = channel.get_state()?;
                        Ok(BackendAudioContent {
                            mode: *mode,
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
                    .zip(&loop_.midi_modes)
                    .map(|(channel, mode)| {
                        let state = channel.get_state()?;
                        let data = channel.get_all_midi_data();
                        Ok(BackendMidiContent {
                            mode: *mode,
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
            let processor_state = if let Some(fx) = track.fx.as_mut() {
                match fx.chain.try_get_state_str() {
                    Ok(state) => {
                        fx.last_confirmed_state = Some(state.clone());
                        Some(state)
                    }
                    Err(error) => Some(fx.last_confirmed_state.clone().ok_or_else(|| {
                        anyhow!("processor state is unavailable and no checkpoint exists: {error}")
                    })?),
                }
            } else {
                None
            };
            let tiny_synth_midi_cc_assignments = track
                .fx
                .as_ref()
                .and_then(|fx| fx.chain.tiny_editor_state())
                .into_iter()
                .flat_map(|editor| editor.midi_cc_assignments)
                .map(app_midi_cc_assignment)
                .map(backend_midi_cc_assignment)
                .collect();
            tracks.push(BackendSessionTrack {
                source_id: track_id.raw(),
                port_name_base: track.port_name_base.clone(),
                topology: track.state.topology.clone(),
                state: track.state.clone(),
                loops,
                ports,
                processor_state,
                tiny_synth_midi_cc_assignments,
            });
        }
        let global_ports = vec![BackendSessionPort {
            source_id: self.global_fx_port.raw(),
            descriptor: self.ports[&self.global_fx_port].descriptor.clone(),
            external_connections: connections
                .confirmed_links
                .iter()
                .filter(|link| link.application_port_id == self.global_fx_port)
                .map(|link| link.host_port_id.clone())
                .collect(),
        }];
        Ok(BackendSessionData {
            sample_rate: self.resolved.sample_rate,
            tracks,
            global_ports,
            use_legacy_browser_default_routes: false,
            soundfonts: BTreeMap::new(),
        })
    }

    fn restore_session(
        &mut self,
        data: &BackendSessionData,
        soundfonts: &soundfont_library::SoundFontLibrary,
    ) -> Result<BackendSessionReplacement> {
        if !self.tracks.is_empty() {
            return Err(anyhow!("target native session is not empty"));
        }
        for track in &data.tracks {
            validate_backend_midi_cc_assignments(track)?;
        }
        let mut replacement = BackendSessionReplacement::default();
        let source_global = data
            .global_ports
            .first()
            .ok_or_else(|| anyhow!("prepared session has no global FX control port"))?;
        if data.global_ports.len() != 1
            || source_global.descriptor.owner != BackendPortOwner::GlobalFxControl
            || source_global.descriptor.data_type != BackendPortDataType::Midi
            || source_global.descriptor.direction != BackendPortDirection::Input
        {
            return Err(anyhow!("prepared global FX control port is invalid"));
        }
        replacement
            .global_ports
            .insert(source_global.source_id, self.global_fx_port);
        for external in &source_global.external_connections {
            self.set_port_connected(self.global_fx_port, external, true)?;
        }
        for source_track in &data.tracks {
            if source_track.state.topology != source_track.topology {
                return Err(anyhow!("prepared native topology state is inconsistent"));
            }
            let request = TrackRequest {
                port_name_base: source_track.port_name_base.clone(),
                topology: source_track.topology.clone(),
                initial_loops: source_track.loops.len(),
            };
            let created = match &request.topology {
                BackendTrackTopology::Direct {
                    audio_channels,
                    midi,
                } => self.create_direct_track(DirectTrackRequest {
                    port_name_base: request.port_name_base,
                    audio_channels: *audio_channels,
                    midi: *midi,
                    initial_loops: request.initial_loops,
                })?,
                BackendTrackTopology::DryWetExternal { .. } => {
                    self.create_external_track(request)?
                }
                BackendTrackTopology::DryWetProcessor { processor_type, .. }
                    if processor_type == TrackProcessorTypeId::OXISYNTH =>
                {
                    if !source_track.tiny_synth_midi_cc_assignments.is_empty() {
                        return Err(anyhow!("OxiSynth track contains Tiny Synth MIDI mappings"));
                    }
                    self.create_processed_track(request)?
                }
                BackendTrackTopology::DryWetProcessor { .. } => {
                    self.create_processed_track(request)?
                }
            };
            match &source_track.topology {
                BackendTrackTopology::DryWetProcessor { .. } => {
                    let state = source_track
                        .processor_state
                        .as_deref()
                        .ok_or_else(|| anyhow!("processed track has no saved processor state"))?;
                    let fx = self
                        .tracks
                        .get_mut(&created.track_id)
                        .and_then(|track| track.fx.as_mut())
                        .ok_or_else(|| anyhow!("restored track has no processor"))?;
                    if fx.processor_type.as_str() == TrackProcessorTypeId::OXISYNTH {
                        let configuration =
                            shoop_engine::oxisynth::OxiSynthProcessor::decode_configuration(state)?;
                        let asset = soundfonts
                            .asset(&configuration.soundfont_sha256)
                            .ok_or_else(|| {
                                anyhow!("missing SoundFont {}", configuration.soundfont_sha256)
                            })?;
                        fx.chain.replace_oxisynth_asset(&asset)?;
                    }
                    fx.chain.try_restore_state(state)?;
                    for assignment in &source_track.tiny_synth_midi_cc_assignments {
                        fx.chain.tiny_assign_midi_cc(engine_midi_cc_assignment(
                            app_backend_midi_cc_assignment(*assignment),
                        ))?;
                    }
                    fx.last_confirmed_state = Some(state.to_owned());
                }
                _ if source_track.processor_state.is_some() => {
                    return Err(anyhow!("unprocessed track has processor state"));
                }
                _ => {}
            }
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
                    || target.audio_modes
                        != source_loop
                            .audio
                            .iter()
                            .map(|channel| channel.mode)
                            .collect::<Vec<_>>()
                    || target.midi_modes
                        != source_loop
                            .midi
                            .iter()
                            .map(|channel| channel.mode)
                            .collect::<Vec<_>>()
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
            .saturating_mul(INPUT_CAPTURE_CAPACITY_SECONDS);
        let capture_block_size = ring.div_ceil(32).max(self.resolved.buffer_size);
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
                capture_block_size,
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
                audio_sends: Vec::new(),
                audio_returns: Vec::new(),
                midi_input,
                midi_output,
                loops: Vec::new(),
                ports: descriptors.iter().map(|port| port.id).collect(),
                state: BackendTrackState {
                    topology: BackendTrackTopology::Direct {
                        audio_channels: request.audio_channels,
                        midi: request.midi,
                    },
                    audio_channels: request.audio_channels,
                    midi: request.midi,
                    ..Default::default()
                },
                dry_passthrough_muted: Some(true),
                wet_passthrough_muted: None,
                fx: None,
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

    fn create_external_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetExternal {
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected External dry/wet topology"));
        };
        let ring = self
            .resolved
            .sample_rate
            .saturating_mul(INPUT_CAPTURE_CAPACITY_SECONDS);
        let capture_block_size = ring.div_ceil(32).max(self.resolved.buffer_size);
        let mut audio_inputs = Vec::with_capacity(dry_audio_channels as usize);
        let mut audio_sends = Vec::with_capacity(dry_audio_channels as usize);
        let mut audio_returns = Vec::with_capacity(wet_audio_channels as usize);
        let mut audio_outputs = Vec::with_capacity(wet_audio_channels as usize);
        let mut descriptors = Vec::new();
        for index in 0..dry_audio_channels {
            let input_name = format!("{}_audio_dry_in_{}", request.port_name_base, index + 1);
            let send_name = format!("{}_audio_dry_send_{}", request.port_name_base, index + 1);
            let input = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                capture_block_size,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let send = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &send_name,
                &PortDirection::Output,
                self.resolved.buffer_size,
            )?;
            input.connect_internal(&send)?;
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
                NativePortHandle::Audio(input.clone()),
            ));
            descriptors.push(self.next_port(
                send_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioSend,
                NativePortHandle::Audio(send.clone()),
            ));
            audio_inputs.push(input);
            audio_sends.push(Some(send));
        }
        for index in 0..wet_audio_channels {
            let return_name = format!("{}_audio_wet_return_{}", request.port_name_base, index + 1);
            let output_name = format!("{}_audio_wet_out_{}", request.port_name_base, index + 1);
            let return_ = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &return_name,
                &PortDirection::Input,
                capture_block_size,
            )?;
            return_.set_passthrough_muted(true)?;
            return_.set_ringbuffer_n_samples(ring)?;
            let output = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &output_name,
                &PortDirection::Output,
                self.resolved.buffer_size,
            )?;
            return_.connect_internal(&output)?;
            descriptors.push(self.next_port(
                return_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioReturn,
                NativePortHandle::Audio(return_.clone()),
            ));
            descriptors.push(self.next_port(
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
                NativePortHandle::Audio(output.clone()),
            ));
            audio_returns.push(Some(return_));
            audio_outputs.push(output);
        }
        let (midi_input, midi_output) = if dry_midi {
            let input_name = format!("{}_dry_midi_in", request.port_name_base);
            let send_name = format!("{}_dry_midi_send", request.port_name_base);
            let input = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                ring,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let send = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &send_name,
                &PortDirection::Output,
                ring,
            )?;
            input.connect_internal(&send)?;
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
                NativePortHandle::Midi(input.clone()),
            ));
            descriptors.push(self.next_port(
                send_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Output,
                BackendPortRole::MidiSend,
                NativePortHandle::Midi(send.clone()),
            ));
            (Some(input), Some(send))
        } else {
            (None, None)
        };
        let processor_audio_sends = audio_sends.iter().flatten().cloned().collect::<Vec<_>>();
        let processor_audio_returns = audio_returns.iter().flatten().cloned().collect::<Vec<_>>();
        let processor_midi_sends = midi_output.iter().cloned().collect::<Vec<_>>();
        self.session.register_external_processor(
            &request.port_name_base,
            &processor_audio_sends,
            &processor_audio_returns,
            &processor_midi_sends,
        )?;
        let track_id = BackendTrackId::from_raw(self.next_track_id);
        self.next_track_id = self.next_track_id.saturating_add(1);
        self.tracks.insert(
            track_id,
            NativeTrack {
                port_name_base: request.port_name_base,
                audio_inputs,
                audio_outputs,
                audio_sends,
                audio_returns,
                midi_input,
                midi_output,
                loops: Vec::new(),
                ports: descriptors.iter().map(|port| port.id).collect(),
                state: BackendTrackState {
                    topology: request.topology,
                    audio_channels: wet_audio_channels,
                    midi: dry_midi,
                    ..Default::default()
                },
                dry_passthrough_muted: Some(true),
                wet_passthrough_muted: Some(true),
                fx: None,
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

    fn create_processed_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        let BackendTrackTopology::DryWetProcessor {
            processor_type,
            dry_audio_channels,
            wet_audio_channels,
            dry_midi,
        } = request.topology.clone()
        else {
            return Err(anyhow!("expected processed dry/wet topology"));
        };
        let chain_type = processor_chain_type(&processor_type)
            .ok_or_else(|| anyhow!("unknown track processor {processor_type}"))?;
        if chain_type == FXChainType::TinySynthFx
            && (dry_audio_channels != wet_audio_channels || !dry_midi)
        {
            return Err(anyhow!(
                "Tiny Synth/FX requires matched audio channels and one MIDI input"
            ));
        }
        if chain_type == FXChainType::OxiSynth
            && (dry_audio_channels != 2 || wet_audio_channels != 2 || !dry_midi)
        {
            return Err(anyhow!(
                "OxiSynth requires two dry audio channels, two wet audio channels, and one MIDI input"
            ));
        }
        let ring = self
            .resolved
            .sample_rate
            .saturating_mul(INPUT_CAPTURE_CAPACITY_SECONDS);
        let capture_block_size = ring.div_ceil(32).max(self.resolved.buffer_size);
        let chain = match chain_type {
            FXChainType::TinySynthFx => self.session.create_tiny_synth_fx_chain(
                &request.port_name_base,
                dry_audio_channels as usize,
                ring,
            )?,
            FXChainType::OxiSynth => self
                .session
                .create_oxisynth_chain(&request.port_name_base, ring)?,
            _ => self
                .session
                .create_fx_chain(chain_type, &request.port_name_base, ring)?,
        };
        let last_confirmed_state = chain.try_get_state_str().ok();
        let mut audio_inputs = Vec::with_capacity(dry_audio_channels as usize);
        let mut audio_sends = Vec::with_capacity(dry_audio_channels as usize);
        let mut audio_returns = Vec::with_capacity(wet_audio_channels as usize);
        let mut audio_outputs = Vec::with_capacity(wet_audio_channels as usize);
        let mut descriptors = Vec::new();
        for index in 0..dry_audio_channels {
            let input_name = format!("{}_audio_dry_in_{}", request.port_name_base, index + 1);
            let input = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                capture_block_size,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let target = chain.get_audio_input_port(index);
            if let Some(target) = &target {
                input.connect_internal(target)?;
            }
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Input,
                BackendPortRole::AudioInput,
                NativePortHandle::Audio(input.clone()),
            ));
            audio_inputs.push(input);
            audio_sends.push(target);
        }
        for index in 0..wet_audio_channels {
            let output_name = format!("{}_audio_wet_out_{}", request.port_name_base, index + 1);
            let output = AudioPort::new_driver_port(
                &self.session,
                &self.driver,
                &output_name,
                &PortDirection::Output,
                self.resolved.buffer_size,
            )?;
            let source = chain.get_audio_output_port(index);
            if let Some(source) = &source {
                source.set_passthrough_muted(true)?;
                source.connect_internal(&output)?;
            }
            descriptors.push(self.next_port(
                output_name,
                BackendPortDataType::Audio,
                BackendPortDirection::Output,
                BackendPortRole::AudioOutput,
                NativePortHandle::Audio(output.clone()),
            ));
            audio_returns.push(source);
            audio_outputs.push(output);
        }
        let (midi_input, midi_output) = if dry_midi {
            let input_name = format!("{}_dry_midi_in", request.port_name_base);
            let input = MidiPort::new_driver_port(
                &self.session,
                &self.driver,
                &input_name,
                &PortDirection::Input,
                ring,
            )?;
            input.set_passthrough_muted(true)?;
            input.set_ringbuffer_n_samples(ring)?;
            let target = chain.get_midi_input_port(0);
            if let Some(target) = &target {
                input.connect_internal(target)?;
            }
            descriptors.push(self.next_port(
                input_name,
                BackendPortDataType::Midi,
                BackendPortDirection::Input,
                BackendPortRole::MidiInput,
                NativePortHandle::Midi(input.clone()),
            ));
            (Some(input), target)
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
                audio_sends,
                audio_returns,
                midi_input,
                midi_output,
                loops: Vec::new(),
                ports: descriptors.iter().map(|port| port.id).collect(),
                state: BackendTrackState {
                    topology: request.topology,
                    audio_channels: wet_audio_channels,
                    midi: dry_midi,
                    ..Default::default()
                },
                dry_passthrough_muted: Some(true),
                wet_passthrough_muted: Some(true),
                fx: Some(NativeFx {
                    processor_type: TrackProcessorTypeId::new(processor_type),
                    chain,
                    active: false,
                    last_confirmed_state,
                }),
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
        {
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
                    if matches!(track.state.topology, BackendTrackTopology::Direct { .. }) {
                        if let Some(port) = &track.midi_output {
                            port.set_muted(value)?;
                        }
                    }
                }
                BackendTrackControl::InputGainDb(value) => track.state.input_gain_db = value,
                BackendTrackControl::InputBalance(value) => {
                    track.state.input_balance = value.clamp(-1.0, 1.0)
                }
                BackendTrackControl::InputMonitoring(value) => {
                    track.state.input_monitoring = value;
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
        }
        self.apply_track_routing(track_id)
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        let fx = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?
            .fx
            .as_mut()
            .ok_or_else(|| anyhow!("track has no processor"))?;
        match control {
            BackendTrackFxControl::SetActive(active) => {
                fx.chain.set_active(active);
                fx.active = active;
            }
            BackendTrackFxControl::SetVisible(visible) => fx.chain.set_visible(visible),
            BackendTrackFxControl::ToggleOrRecover => fx.chain.toggle_or_recover()?,
            BackendTrackFxControl::RestoreState(state) => {
                fx.chain.try_restore_state(&state)?;
                fx.last_confirmed_state = Some(state);
            }
            BackendTrackFxControl::ClearLogs => fx.chain.clear_logs(),
            BackendTrackFxControl::TinySynthFx(control) => {
                if fx.processor_type.as_str() != TrackProcessorTypeId::TINY_SYNTH_FX {
                    return Err(anyhow!("track is not a Tiny Synth/FX processor"));
                }
                match control {
                    TinySynthFxControl::SelectPreset(id) => fx.chain.tiny_select_preset(&id)?,
                    TinySynthFxControl::SetMasterGainDb(value) => {
                        fx.chain.tiny_set_master_gain_db(value)?
                    }
                    TinySynthFxControl::SetReverbEnabled(value) => {
                        fx.chain.tiny_set_reverb_enabled(value)?
                    }
                    TinySynthFxControl::SetReverbAmount(value) => {
                        fx.chain.tiny_set_reverb_amount(value)?
                    }
                    TinySynthFxControl::SetDistortionEnabled(value) => {
                        fx.chain.tiny_set_distortion_enabled(value)?
                    }
                    TinySynthFxControl::SetDistortionDrive(value) => {
                        fx.chain.tiny_set_distortion_drive(value)?
                    }
                    TinySynthFxControl::SetCompressorEnabled(value) => {
                        fx.chain.tiny_set_compressor_enabled(value)?
                    }
                    TinySynthFxControl::SetCompressorAmount(value) => {
                        fx.chain.tiny_set_compressor_amount(value)?
                    }
                    TinySynthFxControl::SetEqEnabled(value) => {
                        fx.chain.tiny_set_eq_enabled(value)?
                    }
                    TinySynthFxControl::SetEqLowDb(value) => fx.chain.tiny_set_eq_low_db(value)?,
                    TinySynthFxControl::SetEqMidDb(value) => fx.chain.tiny_set_eq_mid_db(value)?,
                    TinySynthFxControl::SetEqHighDb(value) => {
                        fx.chain.tiny_set_eq_high_db(value)?
                    }
                    TinySynthFxControl::AssignMidiCc(assignment) => fx
                        .chain
                        .tiny_assign_midi_cc(engine_midi_cc_assignment(assignment))?,
                    TinySynthFxControl::RemoveMidiCc(parameter) => fx
                        .chain
                        .tiny_remove_midi_cc(engine_tiny_synth_parameter(parameter))?,
                    TinySynthFxControl::ClearMidiCcAssignments => {
                        fx.chain.tiny_clear_midi_cc_assignments()?
                    }
                    TinySynthFxControl::Panic => fx.chain.tiny_panic()?,
                }
            }
            BackendTrackFxControl::OxiSynth(control) => {
                if fx.processor_type.as_str() != TrackProcessorTypeId::OXISYNTH {
                    return Err(anyhow!("track is not an OxiSynth processor"));
                }
                let control = match control {
                    OxiSynthControl::SetMasterGain(value) => {
                        shoop_engine::oxisynth::OxiSynthControl::SetMasterGain(value)
                    }
                    OxiSynthControl::SetReverb(value) => {
                        shoop_engine::oxisynth::OxiSynthControl::SetReverb(
                            shoop_engine::oxisynth::OxiSynthReverbConfiguration {
                                room_size: value.room_size,
                                damp: value.damp,
                                width: value.width,
                                level: value.level,
                            },
                        )
                    }
                    OxiSynthControl::SetChorus(value) => {
                        shoop_engine::oxisynth::OxiSynthControl::SetChorus(
                            shoop_engine::oxisynth::OxiSynthChorusConfiguration {
                                voices: value.voices,
                                level: value.level,
                                speed_hz: value.speed_hz,
                                depth_ms: value.depth_ms,
                            },
                        )
                    }
                    OxiSynthControl::SelectSoundFont(_) => {
                        return Err(anyhow!("SoundFont selection was not prepared"));
                    }
                    OxiSynthControl::SelectProgram {
                        channel,
                        bank,
                        program,
                    } => shoop_engine::oxisynth::OxiSynthControl::SelectProgram {
                        channel,
                        bank,
                        program,
                    },
                    OxiSynthControl::Audition {
                        channel,
                        key,
                        velocity,
                        pressed,
                    } => shoop_engine::oxisynth::OxiSynthControl::Audition {
                        channel,
                        key,
                        velocity,
                        pressed,
                    },
                    OxiSynthControl::Panic => shoop_engine::oxisynth::OxiSynthControl::Panic,
                };
                fx.chain.set_oxisynth_control(control)?;
            }
        }
        Ok(())
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        let Some(fx) = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?
            .fx
            .as_mut()
        else {
            return Ok(None);
        };
        match fx.chain.try_get_state_str() {
            Ok(state) => {
                fx.last_confirmed_state = Some(state.clone());
                Ok(Some(state))
            }
            Err(error) => fx
                .last_confirmed_state
                .clone()
                .map(Some)
                .ok_or_else(|| anyhow!("processor state unavailable: {error}")),
        }
    }

    fn apply_track_routing(&mut self, track_id: BackendTrackId) -> Result<()> {
        let (topology, loop_ids, monitoring) = {
            let track = self
                .tracks
                .get(&track_id)
                .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?;
            (
                track.state.topology.clone(),
                track.loops.clone(),
                track.state.input_monitoring,
            )
        };
        let (dry_muted, wet_muted, processor_active, force_monitoring_off) = match topology {
            BackendTrackTopology::Direct { .. } => (!monitoring, true, false, false),
            BackendTrackTopology::DryWetExternal { .. }
            | BackendTrackTopology::DryWetProcessor { .. } => {
                let mut current = Vec::with_capacity(loop_ids.len());
                let mut next = Vec::with_capacity(loop_ids.len());
                for loop_id in loop_ids {
                    let state = self
                        .loops
                        .get(&loop_id)
                        .ok_or_else(|| anyhow!("missing native loop {loop_id:?}"))?
                        .handle
                        .get_state()?;
                    current.push(from_native_mode(state.mode));
                    if state.maybe_next_mode_delay == Some(1) {
                        if let Some(mode) = state.maybe_next_mode {
                            next.push(from_native_mode(mode));
                        }
                    }
                }
                let routing = dry_wet_routing_state(monitoring, &current, &next);
                (
                    routing.dry_input_passthrough_muted,
                    routing.wet_output_passthrough_muted,
                    routing.processor_active,
                    routing.force_monitoring_off,
                )
            }
        };
        let track = self
            .tracks
            .get_mut(&track_id)
            .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?;
        if force_monitoring_off {
            track.state.input_monitoring = false;
        }
        if track.dry_passthrough_muted != Some(dry_muted) {
            for port in &track.audio_inputs {
                port.set_passthrough_muted(dry_muted)?;
            }
            if let Some(port) = &track.midi_input {
                port.set_passthrough_muted(dry_muted)?;
            }
            track.dry_passthrough_muted = Some(dry_muted);
        }
        if !matches!(track.state.topology, BackendTrackTopology::Direct { .. })
            && track.wet_passthrough_muted != Some(wet_muted)
        {
            for port in track.audio_returns.iter().flatten() {
                port.set_passthrough_muted(wet_muted)?;
            }
            track.wet_passthrough_muted = Some(wet_muted);
        }
        if let Some(fx) = track.fx.as_mut() {
            if fx.active != processor_active {
                fx.chain.set_active(processor_active);
                fx.active = processor_active;
            }
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
        for mode in [
            BackendChannelMode::Direct,
            BackendChannelMode::Dry,
            BackendChannelMode::Wet,
        ] {
            let count = loop_
                .audio_modes
                .iter()
                .filter(|candidate| **candidate == mode)
                .count();
            let mut role_index = 0;
            for (channel, channel_mode) in loop_.audio.iter().zip(&loop_.audio_modes) {
                if *channel_mode == mode {
                    channel.set_gain(
                        loop_.gain * stereo_factor(count == 2, role_index, left, right),
                    )?;
                    role_index += 1;
                }
            }
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
        port.handle.wait_ready(&self.session)?;
        if connected {
            port.handle.connect(endpoint);
        } else {
            port.handle.disconnect(endpoint);
        }
        self.wait();
        let observed = port
            .handle
            .connections_now()
            .get(endpoint)
            .copied()
            .unwrap_or(false);
        if observed != connected {
            return Err(anyhow!(
                "external port {endpoint} did not reach requested connection state {connected}"
            ));
        }
        self.connection_revision = self.connection_revision.wrapping_add(1);
        Ok(())
    }
}

impl Backend for NativeBackend {
    fn supports_composite_loops(&self) -> bool {
        true
    }

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
        let rollback_session = self.runtime_mut()?.capture_session()?;
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
                audio_modes: Vec::new(),
                midi: Vec::new(),
                midi_modes: Vec::new(),
                gain: 1.0,
                balance: 0.0,
            },
        );
        runtime.wait();
        Ok(id)
    }

    fn create_composite_loop(&mut self) -> Result<BackendCompositeId> {
        let runtime = self.runtime_mut()?;
        let handle = runtime.session.create_composite_loop()?;
        let id = BackendCompositeId::from_raw(runtime.next_composite_id);
        runtime.next_composite_id = runtime.next_composite_id.saturating_add(1);
        runtime.composites.insert(
            id,
            NativeComposite {
                handle,
                config: None,
            },
        );
        runtime.wait();
        Ok(id)
    }

    fn configure_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        config: &BackendCompositeConfig,
    ) -> Result<()> {
        self.runtime_mut()?
            .configure_composite(composite_id, config)
    }

    fn transition_composite_loop(
        &mut self,
        composite_id: BackendCompositeId,
        mode: BackendLoopMode,
        cycles_delay: Option<u32>,
        align_to_iteration: Option<i64>,
    ) -> Result<()> {
        let runtime = self.runtime_mut()?;
        let composite = runtime
            .composites
            .get(&composite_id)
            .ok_or_else(|| anyhow!("unknown native composite {composite_id:?}"))?;
        if let Some(iteration) = align_to_iteration {
            composite
                .handle
                .transition_immediate(to_native_mode(mode), iteration)?;
        } else if let Some(delay) = cycles_delay {
            composite.handle.transition(to_native_mode(mode), delay)?;
        } else {
            composite
                .handle
                .transition_immediate(to_native_mode(mode), 0)?;
        }
        runtime.wait();
        Ok(())
    }

    fn set_composite_play_after_record(
        &mut self,
        composite_id: BackendCompositeId,
        enabled: bool,
    ) -> Result<()> {
        let runtime = self.runtime_mut()?;
        runtime
            .composites
            .get(&composite_id)
            .ok_or_else(|| anyhow!("unknown native composite {composite_id:?}"))?
            .handle
            .set_play_after_record(enabled)?;
        runtime.wait();
        Ok(())
    }

    fn remove_composite_loop(&mut self, composite_id: BackendCompositeId) -> Result<()> {
        let runtime = self.runtime_mut()?;
        let Some(composite) = runtime.composites.remove(&composite_id) else {
            return Ok(());
        };
        let primitive_sync_sources = runtime.session.primitive_sync_sources();
        if let Err(error) = runtime
            .session
            .remove_composite_loop(&composite.handle, &primitive_sync_sources)
        {
            runtime.composites.insert(composite_id, composite);
            return Err(error);
        }
        runtime.wait();
        Ok(())
    }

    fn track_processor_catalog(&mut self) -> Result<Arc<[TrackProcessorDescriptor]>> {
        let catalog = vec![
            TrackProcessorDescriptor {
                id: TrackProcessorTypeId::new(TrackProcessorTypeId::EXTERNAL),
                label: "External".to_owned(),
                available: true,
                unavailable_reason: None,
                constraints: TrackProcessorConstraints {
                    min_dry_audio_channels: None,
                    max_dry_audio_channels: None,
                    min_wet_audio_channels: None,
                    max_wet_audio_channels: None,
                    matching_audio_channels: false,
                    midi: TrackProcessorMidiPolicy::Optional,
                },
                features: TrackProcessorFeatures::default(),
                editor: None,
            },
            tiny_synth_fx_descriptor(),
            oxisynth_descriptor(),
        ];
        #[cfg(feature = "native-fx")]
        let catalog = {
            let mut catalog = catalog;
            let carla_availability = shoop_engine::carla_native::carla_runtime_availability();
            for (id, label, max_channels) in [
                (TrackProcessorTypeId::CARLA_RACK, "Carla Rack", 2),
                (TrackProcessorTypeId::CARLA_PATCHBAY, "Carla Patchbay", 2),
                (
                    TrackProcessorTypeId::CARLA_PATCHBAY_16X,
                    "Carla Patchbay 16x",
                    16,
                ),
            ] {
                catalog.push(TrackProcessorDescriptor {
                    id: TrackProcessorTypeId::new(id),
                    label: label.to_owned(),
                    available: carla_availability.is_ok(),
                    unavailable_reason: carla_availability.as_ref().err().cloned(),
                    constraints: TrackProcessorConstraints {
                        min_dry_audio_channels: None,
                        max_dry_audio_channels: Some(max_channels),
                        min_wet_audio_channels: None,
                        max_wet_audio_channels: Some(max_channels),
                        matching_audio_channels: false,
                        midi: TrackProcessorMidiPolicy::Optional,
                    },
                    features: TrackProcessorFeatures {
                        state: true,
                        external_ui: true,
                        embedded_ui: false,
                        recovery: true,
                        logs: true,
                    },
                    editor: None,
                });
            }
            catalog
        };
        Ok(catalog.into())
    }

    fn soundfont_catalog(&mut self) -> Result<Arc<[soundfont_library::SoundFontAssetDescriptor]>> {
        Ok(self.soundfonts.descriptors())
    }

    fn import_soundfont(
        &mut self,
        original_filename: String,
        bytes: Arc<[u8]>,
    ) -> Result<soundfont_library::SoundFontAssetDescriptor> {
        self.soundfonts
            .import_persistent(&self.soundfont_directory, bytes, original_filename)
    }

    fn remove_soundfont(&mut self, sha256: &str) -> Result<bool> {
        let referenced = self.runtime.as_ref().is_some_and(|runtime| {
            runtime.tracks.values().any(|track| {
                track.fx.as_ref().is_some_and(|fx| {
                    fx.chain.oxisynth_snapshot().is_some()
                        && fx
                            .chain
                            .try_get_state_str()
                            .is_ok_and(|state| state.contains(sha256))
                })
            })
        });
        let removed = self.soundfonts.remove(sha256, referenced)?;
        if removed {
            for extension in ["sf2", "json"] {
                let path = self
                    .soundfont_directory
                    .join(format!("{sha256}.{extension}"));
                match std::fs::remove_file(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                }
            }
        }
        Ok(removed)
    }

    fn create_track(&mut self, request: TrackRequest) -> Result<BackendTrackCreation> {
        match &request.topology {
            BackendTrackTopology::Direct {
                audio_channels,
                midi,
            } => self.runtime_mut()?.create_direct_track(DirectTrackRequest {
                port_name_base: request.port_name_base,
                audio_channels: *audio_channels,
                midi: *midi,
                initial_loops: request.initial_loops,
            }),
            BackendTrackTopology::DryWetExternal { .. } => {
                self.runtime_mut()?.create_external_track(request)
            }
            BackendTrackTopology::DryWetProcessor { .. } => {
                self.runtime_mut()?.create_processed_track(request)
            }
        }
    }

    fn create_direct_track(&mut self, request: DirectTrackRequest) -> Result<BackendTrackCreation> {
        self.runtime_mut()?.create_direct_track(request)
    }

    fn remove_track(&mut self, track_id: BackendTrackId) -> Result<()> {
        self.runtime_mut()?.remove_track(track_id)
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

    fn inject_midi_input(
        &mut self,
        track_id: BackendTrackId,
        events: &[BackendMidiEvent],
    ) -> Result<()> {
        validate_midi_input_events(events)?;
        let input = self
            .runtime()?
            .tracks
            .get(&track_id)
            .ok_or_else(|| anyhow!("unknown native track {track_id:?}"))?
            .midi_input
            .clone()
            .ok_or_else(|| anyhow!("native track has no MIDI input {track_id:?}"))?;
        input
            .queue_incoming_msgs(
                events
                    .iter()
                    .map(|event| MidiEvent::new(event.time as i32, event.data.clone()))
                    .collect(),
            )
            .map(|_| ())
            .map_err(|error| anyhow!("could not queue native MIDI input: {error}"))
    }

    fn set_track_fx_control(
        &mut self,
        track_id: BackendTrackId,
        control: BackendTrackFxControl,
    ) -> Result<()> {
        if let BackendTrackFxControl::OxiSynth(OxiSynthControl::SelectSoundFont(sha256)) = &control
        {
            let asset = self
                .soundfonts
                .asset(sha256)
                .ok_or_else(|| anyhow!("unknown SoundFont {sha256}"))?;
            let fx = self
                .runtime_mut()?
                .tracks
                .get_mut(&track_id)
                .and_then(|track| track.fx.as_mut())
                .ok_or_else(|| anyhow!("track has no processor"))?;
            if fx.processor_type.as_str() != TrackProcessorTypeId::OXISYNTH {
                return Err(anyhow!("track is not an OxiSynth processor"));
            }
            return fx.chain.replace_oxisynth_asset(&asset);
        }
        if let BackendTrackFxControl::RestoreState(state) = &control {
            if let Ok(configuration) =
                shoop_engine::oxisynth::OxiSynthProcessor::decode_configuration(state)
            {
                let asset = self
                    .soundfonts
                    .asset(&configuration.soundfont_sha256)
                    .ok_or_else(|| {
                        anyhow!("unknown SoundFont {}", configuration.soundfont_sha256)
                    })?;
                let fx = self
                    .runtime_mut()?
                    .tracks
                    .get_mut(&track_id)
                    .and_then(|track| track.fx.as_mut())
                    .ok_or_else(|| anyhow!("track has no processor"))?;
                if fx.processor_type.as_str() == TrackProcessorTypeId::OXISYNTH
                    && fx
                        .chain
                        .oxisynth_asset_metadata()
                        .is_some_and(|metadata| metadata.sha256 != configuration.soundfont_sha256)
                {
                    fx.chain.replace_oxisynth_asset(&asset)?;
                }
            }
        }
        self.runtime_mut()?.set_track_fx_control(track_id, control)
    }

    fn track_fx_state_string(&mut self, track_id: BackendTrackId) -> Result<Option<String>> {
        self.runtime_mut()?.track_fx_state_string(track_id)
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

    fn loop_audio_data_with_metadata(
        &mut self,
        loop_id: BackendLoopId,
    ) -> Result<Option<BackendAudioData>> {
        let runtime = self.runtime()?;
        let loop_ = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        let channels = loop_
            .audio
            .iter()
            .map(|channel| {
                let state = channel.get_state()?;
                Ok(BackendAudioChannelData {
                    samples: Arc::from(channel.get_data()),
                    start_offset: state.start_offset,
                    preplay: state.n_preplay_samples,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(BackendAudioData { channels }))
    }

    fn loop_midi_data(&mut self, loop_id: BackendLoopId) -> Result<Option<BackendMidiData>> {
        let runtime = self.runtime()?;
        let loop_ = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        let channels = loop_
            .midi
            .iter()
            .zip(&loop_.midi_modes)
            .map(|(channel, mode)| {
                let state = channel.get_state()?;
                let data = channel.get_all_midi_data();
                let revision = channel.get_latest_data_snapshot().snapshot.revision.0;
                Ok(BackendMidiChannelData {
                    content_revision: revision,
                    mode: *mode,
                    length: state.length,
                    events: data
                        .into_iter()
                        .filter(|event| event.time >= 0)
                        .map(|event| BackendMidiEvent {
                            time: event.time as u32,
                            data: event.data,
                        })
                        .collect(),
                    start_offset: state.start_offset,
                    preplay: state.n_preplay_samples,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(BackendMidiData { channels }))
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
        let runtime = self.runtime_mut()?;
        runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .transition(
                to_native_mode(mode),
                cycles_delay.map(|value| value as i32).unwrap_or(-1),
                align_to_sync_at.map(|value| value as i32).unwrap_or(-1),
            )?;
        runtime.wait();
        if let Some(track_id) = runtime
            .tracks
            .iter()
            .find_map(|(track_id, track)| track.loops.contains(&loop_id).then_some(*track_id))
        {
            runtime.apply_track_routing(track_id)?;
        }
        Ok(())
    }

    fn clear_loop(&mut self, loop_id: BackendLoopId) -> Result<()> {
        let runtime = self.runtime_mut()?;
        runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .clear(0)?;
        runtime.wait();
        if let Some(track_id) = runtime
            .tracks
            .iter()
            .find_map(|(track_id, track)| track.loops.contains(&loop_id).then_some(*track_id))
        {
            runtime.apply_track_routing(track_id)?;
        }
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
        let runtime = self.runtime_mut()?;
        let target = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        if matches!(
            target.handle.get_state()?.mode,
            shoop_engine::LoopMode::Recording
                | shoop_engine::LoopMode::Replacing
                | shoop_engine::LoopMode::RecordingDryIntoWet
        ) {
            return Err(anyhow!("loop content is changing"));
        }
        let audio = update
            .audio
            .iter()
            .map(|item| {
                Ok(shoop_engine::app_backend::LoopAudioContentUpdate {
                    channel: target
                        .audio
                        .get(item.channel)
                        .ok_or_else(|| anyhow!("unknown audio channel {}", item.channel))?,
                    samples: &item.samples,
                    start_offset: item.start_offset,
                    preplay: item.preplay,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let midi_messages = update
            .midi
            .iter()
            .map(|item| {
                let mut messages = item
                    .start_state
                    .iter()
                    .map(|data| MidiEvent {
                        time: -1,
                        data: data.clone(),
                    })
                    .collect::<Vec<_>>();
                for event in &item.events {
                    messages.push(MidiEvent {
                        time: i32::try_from(event.time)
                            .map_err(|_| anyhow!("MIDI event time exceeds native range"))?,
                        data: event.data.clone(),
                    });
                }
                Ok(messages)
            })
            .collect::<Result<Vec<_>>>()?;
        let midi = update
            .midi
            .iter()
            .zip(&midi_messages)
            .map(|(item, messages)| {
                Ok(shoop_engine::app_backend::LoopMidiContentUpdate {
                    channel: target
                        .midi
                        .get(item.channel)
                        .ok_or_else(|| anyhow!("unknown MIDI channel {}", item.channel))?,
                    messages,
                    length: item.length,
                    start_offset: item.start_offset,
                    preplay: item.preplay,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let sequence = shoop_engine::app_backend::replace_loop_content(
            &target.handle,
            &audio,
            &midi,
            update.length,
        )?;
        runtime
            .session
            .wait_for_command(sequence, shoop_engine::DEFAULT_WAIT_TIMEOUT)?;
        let track_id = runtime
            .tracks
            .iter()
            .find_map(|(track_id, track)| track.loops.contains(&loop_id).then_some(*track_id));
        if let Some(track_id) = track_id {
            runtime.apply_track_routing(track_id)?;
            runtime.wait();
        }
        Ok(())
    }

    fn set_loop_length(&mut self, loop_id: BackendLoopId, length: u32) -> Result<()> {
        let runtime = self.runtime_mut()?;
        let sequence = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?
            .handle
            .set_length(length)?;
        runtime
            .session
            .wait_for_command(sequence, shoop_engine::DEFAULT_WAIT_TIMEOUT)?;
        Ok(())
    }

    fn set_loop_timing(
        &mut self,
        loop_id: BackendLoopId,
        start_offset: Option<i32>,
        preplay: Option<u32>,
        length: Option<u32>,
    ) -> Result<()> {
        let runtime = self.runtime_mut()?;
        let target = runtime
            .loops
            .get(&loop_id)
            .ok_or_else(|| anyhow!("unknown native loop {loop_id:?}"))?;
        let mut sequences = Vec::new();
        for channel in &target.audio {
            if let Some(offset) = start_offset {
                sequences.push(channel.set_start_offset(offset)?);
            }
            if let Some(samples) = preplay {
                sequences.push(channel.set_n_preplay_samples(samples)?);
            }
        }
        for channel in &target.midi {
            if let Some(offset) = start_offset {
                sequences.push(channel.set_start_offset(offset)?);
            }
            if let Some(samples) = preplay {
                sequences.push(channel.set_n_preplay_samples(samples)?);
            }
        }
        if let Some(length) = length {
            sequences.push(target.handle.set_length(length)?);
        }
        for sequence in sequences {
            runtime
                .session
                .wait_for_command(sequence, shoop_engine::DEFAULT_WAIT_TIMEOUT)?;
        }
        Ok(())
    }

    fn capture_session(&mut self) -> Result<BackendSessionData> {
        let mut session = self.runtime_mut()?.capture_session()?;
        session.soundfonts = self
            .soundfonts
            .user_assets()
            .into_iter()
            .map(|asset| {
                (
                    asset.sha256.clone(),
                    BackendSoundFontAsset {
                        original_filename: asset.original_filename.clone(),
                        bytes: asset.bytes.to_vec(),
                    },
                )
            })
            .collect();
        Ok(session)
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
        let available_soundfonts = self.soundfonts.descriptors();
        let runtime = self.runtime_mut()?;
        runtime.wait();
        let track_ids = runtime.tracks.keys().copied().collect::<Vec<_>>();
        for track_id in track_ids {
            runtime.apply_track_routing(track_id)?;
        }
        runtime.wait();
        let driver = runtime.driver.get_state();
        let session = runtime.session.get_state();
        let mut tracks = BTreeMap::new();
        for (id, track) in &runtime.tracks {
            let mut state = track.state.clone();
            state.fx = track.fx.as_ref().map(|fx| {
                let chain_state = fx.chain.get_state().unwrap_or_default();
                TrackFxState {
                    processor_type: fx.processor_type.clone(),
                    active: chain_state.active != 0,
                    visible: chain_state.visible != 0,
                    lifecycle: fx_lifecycle(fx.chain.lifecycle()),
                    generation: fx.chain.generation(),
                    crash_summary: fx.chain.crash_summary(),
                    logs: fx
                        .chain
                        .generation_logs()
                        .into_iter()
                        .map(|log| FxGenerationLogState {
                            generation: log.generation,
                            stdout: Arc::from(String::from_utf8_lossy(&log.stdout).into_owned()),
                            stderr: Arc::from(String::from_utf8_lossy(&log.stderr).into_owned()),
                            dropped_stdout_bytes: log.stdout_dropped_bytes,
                            dropped_stderr_bytes: log.stderr_dropped_bytes,
                        })
                        .collect::<Vec<_>>()
                        .into(),
                    editor: fx
                        .chain
                        .tiny_editor_state()
                        .map(|editor| {
                            TrackProcessorEditorState::TinySynthFx(TinySynthFxState {
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
                                    .into_iter()
                                    .map(app_midi_cc_assignment)
                                    .collect::<Vec<_>>()
                                    .into(),
                            })
                        })
                        .or_else(|| {
                            fx.chain.oxisynth_snapshot().and_then(|snapshot| {
                                fx.chain.oxisynth_asset_metadata().map(|metadata| {
                                    app_oxisynth_state(
                                        snapshot,
                                        metadata,
                                        Arc::clone(&available_soundfonts),
                                    )
                                })
                            })
                        }),
                }
            });
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
            let input_midi_state = track.midi_input.as_ref().and_then(MidiPort::poll_state);
            state.input_midi_activity = input_midi_state
                .as_ref()
                .is_some_and(|state| state.n_input_events > 0 || state.n_input_notes_active > 0);
            state.latest_input_midi_message = input_midi_state
                .and_then(|state| state.latest_input_message)
                .map(Into::into);
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
                    stereo: loop_
                        .audio_modes
                        .iter()
                        .filter(|mode| {
                            matches!(mode, BackendChannelMode::Direct | BackendChannelMode::Wet)
                        })
                        .count()
                        == 2,
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
        let composites = runtime
            .composites
            .iter()
            .filter_map(|(id, composite)| {
                let state = composite.handle.poll_state()?;
                let active_children = state
                    .active_children
                    .iter()
                    .filter_map(|child| {
                        Some(BackendActiveCompositeChild {
                            target: runtime.backend_composite_target(child.identity)?,
                            mode: from_native_mode(child.mode),
                            cycle_offset: child.cycle_offset,
                        })
                    })
                    .collect();
                Some((
                    *id,
                    BackendCompositeState {
                        mode: from_native_mode(state.mode),
                        next_mode: state.maybe_next_mode.map(from_native_mode),
                        next_transition_delay: state.maybe_next_mode_delay,
                        iteration: state.iteration,
                        cycle_count: state.cycle_count,
                        length: state.length,
                        position: state.position,
                        active_plan_version: state.active_plan_version,
                        pending_plan_version: state.pending_plan_version,
                        active_children,
                    },
                ))
            })
            .collect();
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
            composites,
            connections: runtime.take_connection_snapshot(),
            mutation_failures: Vec::new(),
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

fn fx_lifecycle(lifecycle: shoop_engine::carla_processor::CarlaProcessorLifecycle) -> FxLifecycle {
    match lifecycle {
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Stopped => FxLifecycle::Stopped,
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Starting => FxLifecycle::Starting,
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Running => FxLifecycle::Running,
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Crashed => FxLifecycle::Crashed,
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Restarting => {
            FxLifecycle::Restarting
        }
        shoop_engine::carla_processor::CarlaProcessorLifecycle::Unavailable => {
            FxLifecycle::Unavailable
        }
    }
}

fn processor_chain_type(processor_type: &str) -> Option<FXChainType> {
    match processor_type {
        TrackProcessorTypeId::TINY_SYNTH_FX => Some(FXChainType::TinySynthFx),
        TrackProcessorTypeId::OXISYNTH => Some(FXChainType::OxiSynth),
        #[cfg(feature = "native-fx")]
        TrackProcessorTypeId::CARLA_RACK => Some(FXChainType::CarlaRack),
        #[cfg(feature = "native-fx")]
        TrackProcessorTypeId::CARLA_PATCHBAY => Some(FXChainType::CarlaPatchbay),
        #[cfg(feature = "native-fx")]
        TrackProcessorTypeId::CARLA_PATCHBAY_16X => Some(FXChainType::CarlaPatchbay16x),
        #[cfg(test)]
        "test_2x2x1" => Some(FXChainType::Test2x2x1),
        _ => None,
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

    fn assert_injected_note_reaches_output(
        backend: &mut NativeBackend,
        created: &BackendTrackCreation,
        note: u8,
    ) {
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_enter_controlled_mode();
            runtime.tracks[&created.track_id]
                .midi_output
                .as_ref()
                .unwrap()
                .dummy_request_data(128)
                .unwrap();
        }
        backend
            .inject_midi_input(
                created.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, note, 100],
                }],
            )
            .unwrap();
        let runtime = backend.runtime_mut().unwrap();
        runtime.driver.dummy_request_controlled_frames(128);
        runtime.driver.dummy_run_requested_frames();
        assert_eq!(
            runtime.tracks[&created.track_id]
                .midi_output
                .as_ref()
                .unwrap()
                .dummy_dequeue_data(),
            [MidiEvent::new(0, vec![0x90, note, 100])]
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_dummy_exposes_engine_owned_composite_state_and_advancement() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 1_000,
            buffer_size: 1,
        });
        let mut backend = NativeBackend::new(config).unwrap();
        backend
            .runtime_mut()
            .unwrap()
            .driver
            .dummy_enter_controlled_mode();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "composite".to_owned(),
                audio_channels: 0,
                midi: false,
                initial_loops: 4,
            })
            .unwrap();
        let sync = created.loops[0];
        let children = [created.loops[1], created.loops[2], created.loops[3]];
        backend.set_loop_length(sync, 1).unwrap();
        for child in children {
            backend.set_loop_length(child, 4).unwrap();
        }
        let composite = backend.create_composite_loop().unwrap();
        backend
            .configure_composite_loop(
                composite,
                &BackendCompositeConfig {
                    kind: BackendCompositeKind::Regular,
                    sync_source: sync,
                    timelines: vec![children
                        .into_iter()
                        .map(|child| {
                            vec![BackendCompositeEntry {
                                target: BackendCompositeTarget::Loop(child),
                                delay: 0,
                                n_cycles: None,
                                mode: None,
                            }]
                        })
                        .collect()],
                },
            )
            .unwrap();
        backend
            .transition_loop(sync, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_composite_loop(composite, BackendLoopMode::Playing, None, None)
            .unwrap();
        let started = backend.poll().unwrap();
        assert_eq!(
            started.composites[&composite].active_children[0].target,
            BackendCompositeTarget::Loop(children[0])
        );

        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_request_controlled_frames(4);
            runtime.driver.dummy_run_requested_frames();
        }
        backend.wait_idle();
        let advanced = backend.poll().unwrap();
        assert_eq!(advanced.composites[&composite].iteration, 4);
        assert_eq!(
            advanced.composites[&composite].active_children[0].target,
            BackendCompositeTarget::Loop(children[1])
        );

        backend.remove_composite_loop(composite).unwrap();
        assert!(!backend.poll().unwrap().composites.contains_key(&composite));
    }

    #[shoop_wasm_test_support::shoop_test]
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
        let global = backend
            .poll()
            .unwrap()
            .connections
            .application_ports
            .values()
            .find(|port| port.owner == BackendPortOwner::GlobalFxControl)
            .unwrap()
            .id;
        assert!(backend
            .poll()
            .unwrap()
            .connections
            .host_ports
            .contains_key("system:capture_1"));
        backend
            .set_port_connected(input.id, "system:capture_1", true)
            .unwrap();
        backend
            .set_port_connected(global, "controller:midi_out", true)
            .unwrap();
        let mut captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.global_ports[0].external_connections,
            ["controller:midi_out"]
        );
        captured.tracks[0].loops[0].length = 512;
        captured.tracks[0].loops[0].audio[0].samples = vec![0.25, -0.5, 0.75];
        captured.tracks[0].loops[0].midi[0] = BackendMidiContent {
            mode: BackendChannelMode::Direct,
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
        assert_eq!(mapping.global_ports.len(), 1);
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
        assert_eq!(
            restored.global_ports[0].external_connections,
            ["controller:midi_out"]
        );
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

    #[shoop_wasm_test_support::shoop_test]
    fn native_track_midi_injection_uses_the_driver_independent_input_port() {
        let mut backend = NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        }))
        .unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "piano".to_owned(),
                audio_channels: 0,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        assert_injected_note_reaches_output(&mut backend, &created, 60);
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_oxisynth_track_has_fixed_ports_and_rejects_invalid_shapes() {
        let mut backend = NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        }))
        .unwrap();
        let request = |dry_audio_channels, wet_audio_channels| TrackRequest {
            port_name_base: format!("oxisynth-{dry_audio_channels}-{wet_audio_channels}"),
            topology: BackendTrackTopology::DryWetProcessor {
                processor_type: TrackProcessorTypeId::OXISYNTH.to_owned(),
                dry_audio_channels,
                wet_audio_channels,
                dry_midi: true,
            },
            initial_loops: 1,
        };
        assert!(backend.create_track(request(2, 1)).is_err());
        assert!(backend.create_track(request(0, 2)).is_err());
        let created = backend.create_track(request(2, 2)).unwrap();
        assert_eq!(created.ports.len(), 5);
        assert_eq!(
            created
                .ports
                .iter()
                .filter(|port| port.role == BackendPortRole::AudioInput)
                .count(),
            2
        );
        assert_eq!(
            created
                .ports
                .iter()
                .filter(|port| port.role == BackendPortRole::AudioOutput)
                .count(),
            2
        );
        assert_eq!(
            created
                .ports
                .iter()
                .filter(|port| port.role == BackendPortRole::MidiInput)
                .count(),
            1
        );
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetActive(true))
            .unwrap();
        backend.remove_track(created.track_id).unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn targeted_content_update_preserves_native_session_callbacks_sync_and_graph() {
        let mut backend = NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 64,
        }))
        .unwrap();
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "targeted-content".to_owned(),
                audio_channels: 2,
                midi: true,
                initial_loops: 2,
            })
            .unwrap();
        let sync = created.loops[0];
        let target = created.loops[1];
        backend.set_loop_length(sync, 1024).unwrap();
        backend.set_loop_sync_source(target, Some(sync)).unwrap();
        backend
            .transition_loop(sync, BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .transition_loop(target, BackendLoopMode::Playing, None)
            .unwrap();
        let (
            session_id,
            callbacks_before,
            graph_arms,
            graph_applies,
            schedule_request_id,
            schedule_applied_id,
        ) = {
            let runtime = backend.runtime_mut().unwrap();
            runtime.wait();
            let state = runtime.session.get_state();
            (
                runtime.session.session_id(),
                u64::from(state.cycles),
                state.graph_arms,
                state.graph_applies,
                state.schedule_request_id,
                state.schedule_applied_id,
            )
        };

        backend
            .replace_loop_content(
                target,
                &BackendLoopContentUpdate {
                    audio: vec![
                        BackendAudioChannelUpdate {
                            channel: 0,
                            samples: vec![1.0, 2.0, 3.0, 4.0],
                            start_offset: Some(-1),
                            preplay: Some(2),
                        },
                        BackendAudioChannelUpdate {
                            channel: 1,
                            samples: vec![5.0, 6.0, 7.0, 8.0],
                            start_offset: Some(-2),
                            preplay: Some(3),
                        },
                    ],
                    midi: vec![BackendMidiChannelUpdate {
                        channel: 0,
                        length: 4,
                        start_state: vec![vec![0xB0, 7, 100]],
                        events: vec![BackendMidiEvent {
                            time: 1,
                            data: vec![0x90, 64, 127],
                        }],
                        start_offset: Some(-3),
                        preplay: Some(4),
                    }],
                    length: Some(4),
                },
            )
            .unwrap();

        let cycles_after_update = {
            let runtime = backend.runtime_mut().unwrap();
            runtime.wait();
            let state = runtime.session.get_state();
            assert_eq!(runtime.session.session_id(), session_id);
            assert!(u64::from(state.cycles) > callbacks_before);
            assert_eq!(state.graph_arms, graph_arms);
            assert_eq!(state.graph_applies, graph_applies);
            assert_eq!(state.schedule_request_id, schedule_request_id);
            assert_eq!(state.schedule_applied_id, schedule_applied_id);
            assert_eq!(
                runtime.loops[&sync].handle.get_state().unwrap().mode,
                shoop_engine::LoopMode::Playing
            );
            assert_eq!(
                runtime.loops[&target].handle.get_state().unwrap().mode,
                shoop_engine::LoopMode::Stopped
            );
            assert_eq!(
                runtime.loops[&target].audio[0].get_data(),
                [1.0, 2.0, 3.0, 4.0]
            );
            assert_eq!(
                runtime.loops[&target].audio[1].get_data(),
                [5.0, 6.0, 7.0, 8.0]
            );
            assert_eq!(
                runtime.loops[&target].midi[0].get_all_midi_data()[0].time,
                -1
            );
            state.cycles
        };

        let midi_details = backend.loop_midi_data(target).unwrap().unwrap();
        assert_eq!(midi_details.channels.len(), 1);
        assert_eq!(midi_details.channels[0].start_offset, -3);
        assert_eq!(midi_details.channels[0].preplay, 4);
        assert_eq!(midi_details.channels[0].events[0].data, [0x90, 64, 127]);
        assert!(midi_details.channels[0].content_revision > 0);

        let audio_details = backend
            .loop_audio_data_with_metadata(target)
            .unwrap()
            .unwrap();
        assert_eq!(audio_details.channels.len(), 2);
        assert_eq!(
            audio_details.channels[0].samples.as_ref(),
            [1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(audio_details.channels[0].start_offset, -1);
        assert_eq!(audio_details.channels[0].preplay, 2);
        backend
            .set_loop_timing(target, Some(-8), Some(9), Some(12))
            .unwrap();
        let edited_audio = backend
            .loop_audio_data_with_metadata(target)
            .unwrap()
            .unwrap();
        assert!(edited_audio
            .channels
            .iter()
            .all(|channel| channel.start_offset == -8 && channel.preplay == 9));
        let edited_midi = backend.loop_midi_data(target).unwrap().unwrap();
        assert!(edited_midi
            .channels
            .iter()
            .all(|channel| channel.start_offset == -8 && channel.preplay == 9));
        assert_eq!(backend.poll().unwrap().loops[&target].length, 12);

        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].loops[1].audio[0].samples,
            [1.0, 2.0, 3.0, 4.0]
        );
        let cycles_after_capture = {
            let runtime = backend.runtime_mut().unwrap();
            runtime.wait();
            let state = runtime.session.get_state();
            assert_eq!(runtime.session.session_id(), session_id);
            assert!(state.cycles > cycles_after_update);
            assert_eq!(state.graph_arms, graph_arms);
            assert_eq!(state.graph_applies, graph_applies);
            assert_eq!(state.schedule_request_id, schedule_request_id);
            assert_eq!(state.schedule_applied_id, schedule_applied_id);
            assert_eq!(
                runtime.loops[&sync].handle.get_state().unwrap().mode,
                shoop_engine::LoopMode::Playing
            );
            state.cycles
        };

        for generation in 0_u8..8 {
            let sample = f32::from(generation) / 8.0;
            backend
                .replace_loop_content(
                    target,
                    &BackendLoopContentUpdate {
                        audio: vec![
                            BackendAudioChannelUpdate {
                                channel: 0,
                                samples: vec![sample; 16_384],
                                start_offset: None,
                                preplay: None,
                            },
                            BackendAudioChannelUpdate {
                                channel: 1,
                                samples: vec![-sample; 16_384],
                                start_offset: None,
                                preplay: None,
                            },
                        ],
                        midi: vec![BackendMidiChannelUpdate {
                            channel: 0,
                            length: 16_384,
                            start_state: vec![vec![0xB0, 7, generation]],
                            events: (0..1_024)
                                .map(|index| BackendMidiEvent {
                                    time: index * 8,
                                    data: vec![0x90, 64, generation],
                                })
                                .collect(),
                            start_offset: None,
                            preplay: None,
                        }],
                        length: Some(16_384),
                    },
                )
                .unwrap();
            backend.set_loop_length(target, 8_192).unwrap();
        }

        let runtime = backend.runtime_mut().unwrap();
        runtime.wait();
        let state = runtime.session.get_state();
        assert_eq!(runtime.session.session_id(), session_id);
        assert!(state.cycles > cycles_after_capture);
        assert_eq!(state.graph_arms, graph_arms);
        assert_eq!(state.graph_applies, graph_applies);
        assert_eq!(state.schedule_request_id, schedule_request_id);
        assert_eq!(state.schedule_applied_id, schedule_applied_id);
        assert_eq!(
            runtime.loops[&sync].handle.get_state().unwrap().mode,
            shoop_engine::LoopMode::Playing
        );
        assert_eq!(
            runtime.loops[&target].audio[0].get_data()[0],
            f32::from(7_u8) / 8.0
        );
        assert_eq!(
            runtime.loops[&target].handle.get_state().unwrap().length,
            8_192
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_dummy_external_track_preserves_roles_media_and_routing() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        });
        let mut backend = NativeBackend::new(config.clone()).unwrap();
        assert_eq!(
            backend.track_processor_catalog().unwrap()[0].id.as_str(),
            TrackProcessorTypeId::EXTERNAL
        );
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "external".to_owned(),
                topology: BackendTrackTopology::DryWetExternal {
                    dry_audio_channels: 2,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        assert_eq!(created.ports.len(), 8);
        let dry_input = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioInput)
            .unwrap();
        let wet_output = created
            .ports
            .iter()
            .find(|port| port.role == BackendPortRole::AudioOutput)
            .unwrap();
        backend
            .set_port_connected(dry_input.id, "system:capture_1", true)
            .unwrap();
        backend
            .set_port_connected(wet_output.id, "system:playback_1", true)
            .unwrap();
        let mut captured = backend.capture_session().unwrap();
        captured.tracks[0].loops[0].length = 2;
        captured.tracks[0].loops[0].audio[0].samples = vec![0.25, -0.5];
        captured.tracks[0].loops[0].audio[2].samples = vec![0.75, -1.0];
        assert_eq!(
            captured.tracks[0].loops[0]
                .audio
                .iter()
                .map(|channel| channel.mode)
                .collect::<Vec<_>>(),
            vec![
                BackendChannelMode::Dry,
                BackendChannelMode::Dry,
                BackendChannelMode::Wet,
            ]
        );
        {
            let track = &backend.runtime().unwrap().tracks[&created.track_id];
            assert!(track.audio_inputs[0].get_state().unwrap().passthrough_muted);
            assert!(
                track.audio_returns[0]
                    .as_ref()
                    .unwrap()
                    .get_state()
                    .unwrap()
                    .passthrough_muted
            );
        }
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend.wait_idle();
        {
            let track = &backend.runtime().unwrap().tracks[&created.track_id];
            assert!(!track.audio_inputs[0].get_state().unwrap().passthrough_muted);
            assert!(
                !track.audio_returns[0]
                    .as_ref()
                    .unwrap()
                    .get_state()
                    .unwrap()
                    .passthrough_muted
            );
        }
        let snapshot = backend.poll().unwrap();
        assert!(snapshot.tracks[&created.track_id].input_monitoring);

        backend
            .switch_audio_driver(&config, 48_000, &captured)
            .unwrap();
        let restored = backend.capture_session().unwrap();
        assert_eq!(restored.tracks[0].topology, captured.tracks[0].topology);
        assert_eq!(
            restored.tracks[0].loops[0].audio[0].samples,
            vec![0.25, -0.5]
        );
        assert_eq!(
            restored.tracks[0].loops[0].audio[2].samples,
            vec![0.75, -1.0]
        );
        assert!(restored.tracks[0]
            .ports
            .iter()
            .any(|port| port.external_connections == ["system:capture_1"]));
        assert!(restored.tracks[0]
            .ports
            .iter()
            .any(|port| port.external_connections == ["system:playback_1"]));
    }

    #[cfg(feature = "native-fx")]
    #[shoop_wasm_test_support::shoop_test]
    fn worker_entry_ignores_gui_arguments_and_validates_hidden_identity() {
        assert!(!run_carla_worker_if_requested(["app", "--fullscreen"]).unwrap());
        let error =
            run_carla_worker_if_requested(["app", "--carla-worker", "--carla-worker-nonce", "bad"])
                .unwrap_err();
        assert!(error.to_string().contains("64 hex digits"));
    }

    #[cfg(feature = "native-fx")]
    #[shoop_wasm_test_support::shoop_test]
    fn native_fx_catalog_advertises_carla_facets_and_constraints() {
        let mut backend = NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        }))
        .unwrap();
        let catalog = backend.track_processor_catalog().unwrap();
        assert_eq!(catalog.len(), 6);
        assert_eq!(catalog[1].id.as_str(), TrackProcessorTypeId::TINY_SYNTH_FX);
        assert_eq!(catalog[2].id.as_str(), TrackProcessorTypeId::OXISYNTH);
        let runtime_available = shoop_engine::carla_native::carla_runtime_availability().is_ok();
        for descriptor in &catalog[3..] {
            assert_eq!(descriptor.available, runtime_available);
            assert_eq!(descriptor.unavailable_reason.is_none(), runtime_available);
            assert!(descriptor.features.state);
            assert!(descriptor.features.external_ui);
            assert!(descriptor.features.recovery);
            assert!(descriptor.features.logs);
            assert_eq!(
                descriptor.constraints.midi,
                TrackProcessorMidiPolicy::Optional
            );
            assert!(descriptor.constraints.max_dry_audio_channels.is_some());
            assert!(descriptor.constraints.max_wet_audio_channels.is_some());
        }
    }

    #[cfg(feature = "native-fx")]
    #[shoop_wasm_test_support::shoop_test]
    fn missing_carla_runtime_disables_only_carla_catalog_entries() {
        let original_library = std::env::var_os("SHOOP_CARLA_NATIVE_LIBRARY");
        let original_resources = std::env::var_os("SHOOP_CARLA_RESOURCE_DIR");
        unsafe {
            std::env::set_var(
                "SHOOP_CARLA_NATIVE_LIBRARY",
                std::env::temp_dir().join("shoop-certainly-missing-carla.so"),
            );
            std::env::remove_var("SHOOP_CARLA_RESOURCE_DIR");
        }
        let result = (|| {
            let mut backend =
                NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: 48_000,
                    buffer_size: 128,
                }))?;
            let catalog = backend.track_processor_catalog()?;
            assert!(catalog[..3].iter().all(|descriptor| {
                descriptor.available && !descriptor.id.as_str().starts_with("carla_")
            }));
            assert!(catalog[3..].iter().all(|descriptor| {
                !descriptor.available && descriptor.unavailable_reason.is_some()
            }));
            Ok::<_, anyhow::Error>(())
        })();
        unsafe {
            match original_library {
                Some(value) => std::env::set_var("SHOOP_CARLA_NATIVE_LIBRARY", value),
                None => std::env::remove_var("SHOOP_CARLA_NATIVE_LIBRARY"),
            }
            match original_resources {
                Some(value) => std::env::set_var("SHOOP_CARLA_RESOURCE_DIR", value),
                None => std::env::remove_var("SHOOP_CARLA_RESOURCE_DIR"),
            }
        }
        result.unwrap();
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_dummy_processed_track_wires_fake_fx_without_public_internal_ports() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 4,
        });
        let mut backend = NativeBackend::new(config.clone()).unwrap();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "processed".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: "test_2x2x1".to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        assert_eq!(
            created
                .ports
                .iter()
                .map(|port| port.role)
                .collect::<Vec<_>>(),
            vec![
                BackendPortRole::AudioInput,
                BackendPortRole::AudioInput,
                BackendPortRole::AudioOutput,
                BackendPortRole::AudioOutput,
                BackendPortRole::MidiInput,
            ]
        );
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        let snapshot = backend.poll().unwrap();
        let fx = snapshot.tracks[&created.track_id].fx.as_ref().unwrap();
        assert!(fx.active);
        assert_eq!(fx.lifecycle, FxLifecycle::Running);
        assert_eq!(fx.processor_type.as_str(), "test_2x2x1");
        backend
            .set_track_fx_control(created.track_id, BackendTrackFxControl::SetVisible(true))
            .unwrap();
        assert!(
            backend.poll().unwrap().tracks[&created.track_id]
                .fx
                .as_ref()
                .unwrap()
                .visible
        );

        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_enter_controlled_mode();
            let input = runtime.tracks[&created.track_id].audio_inputs[0].clone();
            let output = runtime.tracks[&created.track_id].audio_outputs[0].clone();
            input.dummy_queue_data(&[1.0, -0.5, 0.25, 0.0]).unwrap();
            output.dummy_request_data(4).unwrap();
            runtime.driver.dummy_request_controlled_frames(4);
            runtime.driver.dummy_run_requested_frames();
            assert_eq!(output.dummy_dequeue_data(4), vec![0.5, -0.25, 0.125, 0.0]);
        }

        backend
            .transition_loop(created.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        backend.poll().unwrap();
        {
            let runtime = backend.runtime_mut().unwrap();
            let input = runtime.tracks[&created.track_id].audio_inputs[0].clone();
            input.dummy_queue_data(&[2.0, 4.0, 6.0, 8.0]).unwrap();
            runtime.driver.dummy_request_controlled_frames(4);
            runtime.driver.dummy_run_requested_frames();
        }
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();
        {
            let runtime = backend.runtime_mut().unwrap();
            let wet = runtime.loops[&created.loops[0]].audio[2].clone();
            assert!(matches!(
                wet.try_get_current_data_snapshot(),
                Err(
                    shoop_engine::content_snapshot::CurrentDataError::MutationActive(
                        shoop_engine::content_snapshot::ContentMutation::Recording
                    )
                )
            ));
            runtime.driver.dummy_request_controlled_frames(1);
            runtime.driver.dummy_run_requested_frames();
            let deadline = std::time::Instant::now() + Duration::from_secs(1);
            while wet.try_get_current_data_snapshot().is_err() {
                assert!(std::time::Instant::now() < deadline);
                std::thread::yield_now();
            }
        }
        let captured = backend.capture_session().unwrap();
        assert_eq!(
            captured.tracks[0].loops[0].audio[2].samples,
            vec![1.0, 2.0, 3.0, 4.0]
        );
        assert_eq!(captured.tracks[0].processor_state.as_deref(), Some(""));
        let mut restored = NativeBackend::new(config).unwrap();
        restored.replace_session(&captured).unwrap();
        assert_eq!(
            restored.capture_session().unwrap().tracks[0].topology,
            captured.tracks[0].topology
        );
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_dummy_grab_captures_processed_dry_wet_audio_and_midi() {
        const FRAMES: u32 = 128;
        const NOTE_FRAME: usize = 16;
        const NOTE_VELOCITY: u8 = 102;

        let mut backend = NativeBackend::new(AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: FRAMES,
        }))
        .unwrap();
        let sync = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "grab_sync".to_owned(),
                audio_channels: 0,
                midi: false,
                initial_loops: 1,
            })
            .unwrap();
        let target = backend
            .create_track(TrackRequest {
                port_name_base: "grab_processed".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: "test_2x2x1".to_owned(),
                    dry_audio_channels: 2,
                    wet_audio_channels: 2,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend.set_loop_length(sync.loops[0], FRAMES).unwrap();
        backend
            .transition_loop(sync.loops[0], BackendLoopMode::Playing, None)
            .unwrap();
        backend
            .set_loop_sync_source(target.loops[0], Some(sync.loops[0]))
            .unwrap();
        backend
            .set_track_control(target.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();

        let dry_left = vec![0.25; FRAMES as usize];
        let dry_right = vec![-0.5; FRAMES as usize];
        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_enter_controlled_mode();
            let track = &runtime.tracks[&target.track_id];
            for input in &track.audio_inputs {
                input.set_ringbuffer_n_samples(FRAMES).unwrap();
            }
            track
                .midi_input
                .as_ref()
                .unwrap()
                .set_ringbuffer_n_samples(FRAMES)
                .unwrap();
            runtime.loops[&sync.loops[0]]
                .handle
                .set_position(0)
                .unwrap();
            runtime.wait();

            let track = &runtime.tracks[&target.track_id];
            let left_input = track.audio_inputs[0].clone();
            let right_input = track.audio_inputs[1].clone();
            let midi_input = track.midi_input.as_ref().unwrap().clone();
            for sequence in [
                left_input.dummy_queue_data(&dry_left).unwrap(),
                right_input.dummy_queue_data(&dry_right).unwrap(),
                midi_input
                    .dummy_queue_msgs(vec![
                        MidiEvent::new(NOTE_FRAME as i32, vec![0x90, 64, NOTE_VELOCITY]),
                        MidiEvent::new(64, vec![0x80, 64, 0]),
                    ])
                    .unwrap(),
            ] {
                runtime
                    .session
                    .wait_for_command(sequence, shoop_engine::DEFAULT_WAIT_TIMEOUT)
                    .unwrap();
            }
            runtime.driver.dummy_request_controlled_frames(FRAMES);
            runtime.driver.dummy_run_requested_frames();
        }

        backend
            .grab_loops(&[BackendGrabRequest {
                loop_id: target.loops[0],
                reverse_start_cycle: Some(1),
                cycles_length: Some(1),
                go_to_cycle: Some(0),
                go_to_mode: BackendLoopMode::Playing,
            }])
            .unwrap();

        let captured = backend.capture_session().unwrap();
        let target_loop = &captured
            .tracks
            .iter()
            .find(|track| track.source_id == target.track_id.raw())
            .unwrap()
            .loops[0];
        assert_eq!(target_loop.length, FRAMES);
        assert_eq!(target_loop.audio[0].samples, dry_left);
        assert_eq!(target_loop.audio[1].samples, dry_right);
        assert_eq!(
            target_loop.midi[0].events,
            vec![
                BackendMidiEvent {
                    time: NOTE_FRAME as u32,
                    data: vec![0x90, 64, NOTE_VELOCITY],
                },
                BackendMidiEvent {
                    time: 64,
                    data: vec![0x80, 64, 0],
                },
            ]
        );

        let mut expected_left = vec![0.125; FRAMES as usize];
        let mut expected_right = vec![-0.25; FRAMES as usize];
        let note_impulse = f32::from(NOTE_VELOCITY) / 255.0;
        expected_left[NOTE_FRAME] += note_impulse;
        expected_right[NOTE_FRAME] += note_impulse;
        for (frame, (actual, expected)) in target_loop.audio[2]
            .samples
            .iter()
            .zip(&expected_left)
            .enumerate()
        {
            assert!(
                (*actual - *expected).abs() < 1.0e-6,
                "left wet sample {frame}: expected {expected}, got {actual}"
            );
        }
        for (frame, (actual, expected)) in target_loop.audio[3]
            .samples
            .iter()
            .zip(&expected_right)
            .enumerate()
        {
            assert!(
                (*actual - *expected).abs() < 1.0e-6,
                "right wet sample {frame}: expected {expected}, got {actual}"
            );
        }
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_dummy_tiny_synth_fx_processes_midi_and_round_trips_state() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        });
        let mut backend = NativeBackend::new(config.clone()).unwrap();
        let created = backend
            .create_track(TrackRequest {
                port_name_base: "tiny".to_owned(),
                topology: BackendTrackTopology::DryWetProcessor {
                    processor_type: TrackProcessorTypeId::TINY_SYNTH_FX.to_owned(),
                    dry_audio_channels: 1,
                    wet_audio_channels: 1,
                    dry_midi: true,
                },
                initial_loops: 1,
            })
            .unwrap();
        backend
            .set_track_control(created.track_id, BackendTrackControl::InputMonitoring(true))
            .unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::SelectPreset(
                    "pad".to_owned(),
                )),
            )
            .unwrap();
        let _ = backend.poll().unwrap();
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::AssignMidiCc(
                    TinySynthFxMidiCcAssignment {
                        parameter: TinySynthFxParameter::ReverbAmount,
                        channel: 3,
                        controller: 21,
                    },
                )),
            )
            .unwrap();
        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_enter_controlled_mode();
            let midi = runtime.tracks[&created.track_id]
                .midi_input
                .as_ref()
                .unwrap()
                .clone();
            midi.dummy_queue_msg(&shoop_engine::MidiEvent {
                time: 0,
                data: vec![0xb3, 21, 127],
            })
            .unwrap();
            runtime.driver.dummy_request_controlled_frames(128);
            runtime.driver.dummy_run_requested_frames();
        }
        let snapshot = backend.poll().unwrap();
        assert_eq!(
            snapshot.tracks[&created.track_id]
                .latest_input_midi_message
                .unwrap(),
            BackendLatestMidiMessage {
                bytes: [0xb3, 21, 127, 0],
                len: 3,
            }
        );
        let Some(TrackProcessorEditorState::TinySynthFx(editor)) = snapshot.tracks
            [&created.track_id]
            .fx
            .as_ref()
            .and_then(|fx| fx.editor.as_ref())
        else {
            panic!("missing Tiny Synth/FX editor state");
        };
        assert_eq!(editor.reverb_amount, 1.0);

        {
            let runtime = backend.runtime_mut().unwrap();
            runtime.driver.dummy_enter_controlled_mode();
            let track = &runtime.tracks[&created.track_id];
            track
                .midi_input
                .as_ref()
                .unwrap()
                .dummy_queue_msg(&shoop_engine::MidiEvent {
                    time: 0,
                    data: vec![0x90, 69, 127],
                })
                .unwrap();
            let output = track.audio_outputs[0].clone();
            output.dummy_request_data(128).unwrap();
            runtime.driver.dummy_request_controlled_frames(128);
            runtime.driver.dummy_run_requested_frames();
            assert!(output
                .dummy_dequeue_data(128)
                .iter()
                .any(|sample| sample.abs() > 0.001));
        }
        backend
            .set_track_fx_control(
                created.track_id,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::Panic),
            )
            .unwrap();
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Recording, None)
            .unwrap();
        backend.poll().unwrap();
        {
            let runtime = backend.runtime_mut().unwrap();
            let midi = runtime.tracks[&created.track_id]
                .midi_input
                .as_ref()
                .unwrap()
                .clone();
            midi.dummy_queue_msg(&shoop_engine::MidiEvent {
                time: 0,
                data: vec![0x90, 69, 127],
            })
            .unwrap();
            for _ in 0..8 {
                runtime.driver.dummy_request_controlled_frames(128);
                runtime.driver.dummy_run_requested_frames();
            }
            midi.dummy_queue_msg(&shoop_engine::MidiEvent {
                time: 0,
                data: vec![0x80, 69, 0],
            })
            .unwrap();
            for _ in 0..2 {
                runtime.driver.dummy_request_controlled_frames(128);
                runtime.driver.dummy_run_requested_frames();
            }
        }
        backend
            .transition_loop(created.loops[0], BackendLoopMode::Stopped, None)
            .unwrap();

        let captured = backend.capture_session().unwrap();
        let wet = &captured.tracks[0].loops[0].audio[1].samples;
        assert!(wet.iter().filter(|sample| sample.abs() > 1.0e-7).count() > wet.len() / 2);
        assert!(captured.tracks[0]
            .processor_state
            .as_deref()
            .is_some_and(|state| state.starts_with("shoop-tiny-synth-fx:1:")));
        assert_eq!(
            captured.tracks[0].tiny_synth_midi_cc_assignments,
            [BackendTinySynthFxMidiCcAssignment {
                parameter: BackendTinySynthFxParameter::ReverbAmount,
                channel: 3,
                controller: 21,
            }]
        );
        backend
            .switch_audio_driver(
                &AudioDriverConfig::Dummy(DummyAudioDriverConfig {
                    sample_rate: 48_000,
                    buffer_size: 256,
                }),
                48_000,
                &captured,
            )
            .unwrap();
        let switched = backend.capture_session().unwrap();
        assert_eq!(
            switched.tracks[0].processor_state,
            captured.tracks[0].processor_state
        );
        assert_eq!(
            switched.tracks[0].tiny_synth_midi_cc_assignments,
            captured.tracks[0].tiny_synth_midi_cc_assignments
        );

        let mut browser = EngineBackend::new_web_audio(48_000, 128).unwrap();
        browser.replace_session(&captured).unwrap();
        let browser_state = browser.capture_session().unwrap();
        assert_eq!(
            browser_state.tracks[0].topology,
            captured.tracks[0].topology
        );
        assert_eq!(
            browser_state.tracks[0].processor_state,
            captured.tracks[0].processor_state
        );
        assert_eq!(
            browser_state.tracks[0].tiny_synth_midi_cc_assignments,
            captured.tracks[0].tiny_synth_midi_cc_assignments
        );

        let mut restored = NativeBackend::new(config).unwrap();
        let replacement = restored.replace_session(&browser_state).unwrap();
        let restored_track = replacement.tracks[&captured.tracks[0].source_id].track_id;
        let restored_state = restored.capture_session().unwrap();
        assert_eq!(
            restored_state.tracks[0].topology,
            captured.tracks[0].topology
        );
        assert_eq!(
            restored_state.tracks[0].processor_state,
            captured.tracks[0].processor_state
        );
        assert_eq!(
            restored_state.tracks[0].tiny_synth_midi_cc_assignments,
            captured.tracks[0].tiny_synth_midi_cc_assignments
        );
        restored
            .set_track_fx_control(
                restored_track,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::RemoveMidiCc(
                    TinySynthFxParameter::ReverbAmount,
                )),
            )
            .unwrap();
        assert!(restored.capture_session().unwrap().tracks[0]
            .tiny_synth_midi_cc_assignments
            .is_empty());
        restored
            .set_track_fx_control(
                restored_track,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::AssignMidiCc(
                    TinySynthFxMidiCcAssignment {
                        parameter: TinySynthFxParameter::EqLow,
                        channel: 1,
                        controller: 71,
                    },
                )),
            )
            .unwrap();
        restored
            .set_track_fx_control(
                restored_track,
                BackendTrackFxControl::TinySynthFx(TinySynthFxControl::ClearMidiCcAssignments),
            )
            .unwrap();
        assert!(restored.capture_session().unwrap().tracks[0]
            .tiny_synth_midi_cc_assignments
            .is_empty());
    }

    #[shoop_wasm_test_support::shoop_test]
    fn native_track_removal_releases_ports_for_same_name_recreation() {
        let config = AudioDriverConfig::Dummy(DummyAudioDriverConfig {
            sample_rate: 48_000,
            buffer_size: 128,
        });
        let mut backend = NativeBackend::new(config).unwrap();
        let request = DirectTrackRequest {
            port_name_base: "reusable_native".to_owned(),
            audio_channels: 2,
            midi: true,
            initial_loops: 2,
        };
        let first = backend.create_direct_track(request.clone()).unwrap();
        let first_names = first
            .ports
            .iter()
            .map(|port| port.name.clone())
            .collect::<Vec<_>>();
        backend.remove_track(first.track_id).unwrap();
        let removed = backend.poll().unwrap();
        assert!(!removed.tracks.contains_key(&first.track_id));
        assert!(first
            .loops
            .iter()
            .all(|loop_id| !removed.loops.contains_key(loop_id)));
        assert!(first
            .ports
            .iter()
            .all(|port| !removed.connections.application_ports.contains_key(&port.id)));

        let recreated = backend.create_direct_track(request).unwrap();
        assert_eq!(
            recreated
                .ports
                .iter()
                .map(|port| port.name.clone())
                .collect::<Vec<_>>(),
            first_names
        );
        assert!(recreated
            .ports
            .iter()
            .all(|port| first.ports.iter().all(|old| old.id != port.id)));
    }

    #[shoop_wasm_test_support::shoop_test]
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
            soundfonts: soundfont_library::SoundFontLibrary::with_embedded().unwrap(),
            soundfont_directory: std::env::temp_dir(),
        };
        let created = backend
            .create_direct_track(DirectTrackRequest {
                port_name_base: "jack_test".to_owned(),
                audio_channels: 1,
                midi: true,
                initial_loops: 1,
            })
            .unwrap();
        assert_injected_note_reaches_output(&mut backend, &created, 61);
        assert!(backend
            .poll()
            .unwrap()
            .connections
            .host_ports
            .keys()
            .any(|name| name.starts_with("test_client_1:")));
    }

    #[shoop_wasm_test_support::shoop_test]
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
            soundfonts: soundfont_library::SoundFontLibrary::with_embedded().unwrap(),
            soundfont_directory: std::env::temp_dir(),
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
        backend
            .inject_midi_input(
                created.track_id,
                &[BackendMidiEvent {
                    time: 0,
                    data: vec![0x90, 62, 100],
                }],
            )
            .unwrap();
        assert_eq!(backend.capture_session().unwrap().tracks.len(), 1);
        let snapshot = backend.poll().unwrap();
        assert_eq!(snapshot.status.sample_rate, 48_000);
        assert_eq!(snapshot.status.buffer_size, 128);
        assert!(snapshot.tracks[&created.track_id].midi);
    }

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
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

    #[shoop_wasm_test_support::shoop_test]
    fn production_catalog_never_exposes_test_drivers() {
        let backend = NativeBackend::new(AudioDriverConfig::default()).unwrap();
        assert_eq!(backend.catalog.len(), 3);
        assert_eq!(backend.catalog[0].kind, AudioDriverKind::Dummy);
        assert_eq!(backend.catalog[1].kind, AudioDriverKind::Jack);
        assert_eq!(backend.catalog[2].kind, AudioDriverKind::Cpal);
    }
}
