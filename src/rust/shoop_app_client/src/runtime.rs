//! UI-local workflows and Lua execution over a confirmed session projection.
use crate::{Client, Error, State};
use shoop_app_api::*;
use shoop_app_protocol::*;
use shoop_scripting::{ControlSnapshot, MidiControlService, ScriptManager};
use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    sync::Arc,
    time::Duration,
};

pub trait Delivery {
    fn send(&mut self, message: ClientMessage) -> Result<(), Error>;
    fn poll(&mut self, client: &mut Client) -> Result<usize, Error>;
}
impl Delivery for local::ClientPort {
    fn send(&mut self, message: ClientMessage) -> Result<(), Error> {
        self.commands
            .try_send(message)
            .map_err(|error| match error {
                std::sync::mpsc::TrySendError::Full(_) => Error::Full,
                std::sync::mpsc::TrySendError::Disconnected(_) => Error::Disconnected,
            })
    }
    fn poll(&mut self, client: &mut Client) -> Result<usize, Error> {
        client.pump_local(self)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StartupScript {
    pub name: String,
    pub identity: Option<String>,
    pub source: String,
    pub source_path: Option<String>,
    pub kind: ScriptKind,
    pub enabled: bool,
}
#[derive(Clone, Debug)]
pub struct ApplicationFileOutput {
    pub task_id: TaskId,
    pub suggested_name: String,
    pub mime_type: String,
    pub bytes: Arc<[u8]>,
}
#[derive(Clone, Debug)]
pub struct ApplicationAudioPreview {
    pub output: ClickTrackPreviewOutput,
    pub request_id: u64,
    pub sample_rate: u32,
    pub samples: Arc<[f32]>,
}

pub enum Startup {
    Fresh(Vec<StartupScript>),
    IfFresh(Vec<StartupScript>),
    Recovered,
}

pub struct Runtime {
    client: Client,
    delivery: Box<dyn Delivery>,
    scripts: ScriptManager,
    previous: ControlSnapshot,
    startup: Option<Startup>,
    startup_ids: Vec<Option<ScriptId>>,
    recovered: bool,
    explicitly_started_scripts: BTreeSet<String>,
    reset_operation: Option<OperationId>,
    reset_queued: bool,
    io_task: Option<IoTaskState>,
    intents: VecDeque<AppIntent>,
    commands: VecDeque<(Command, Option<(u64, bool)>)>,
    creations: BTreeMap<OperationId, (u64, bool)>,
    seen_results: BTreeSet<OperationId>,
    track_results: VecDeque<TrackCreationResult>,
    bus_results: VecDeque<BusCreationResult>,
    notifications: VecDeque<AppNotification>,
    next_notice: u64,
    outputs: VecDeque<ApplicationFileOutput>,
    snapshot: Arc<AppSnapshot>,
    requested_media: Option<(LoopId, u64, bool)>,
    media_refusal_notified: bool,
    loss_reported: bool,
    controls_suspended: bool,
}

impl Runtime {
    pub fn new(
        mut client: Client,
        mut delivery: Box<dyn Delivery>,
        midi: Box<dyn MidiControlService>,
        startup: Startup,
    ) -> Result<Self, Error> {
        delivery.send(client.attach()?)?;
        let recovered = matches!(startup, Startup::Recovered);
        let mut scripts = ScriptManager::new_with_midi(midi);
        scripts.use_confirmed_control_reads();
        let mut runtime = Self {
            client,
            delivery,
            scripts,
            previous: ControlSnapshot::default(),
            startup: Some(startup),
            startup_ids: Vec::new(),
            recovered,
            explicitly_started_scripts: BTreeSet::new(),
            reset_operation: None,
            reset_queued: false,
            io_task: None,
            intents: VecDeque::new(),
            commands: VecDeque::new(),
            creations: BTreeMap::new(),
            seen_results: BTreeSet::new(),
            track_results: VecDeque::new(),
            bus_results: VecDeque::new(),
            notifications: VecDeque::new(),
            next_notice: 1,
            outputs: VecDeque::new(),
            snapshot: Arc::new(AppSnapshot::default()),
            requested_media: None,
            media_refusal_notified: false,
            loss_reported: false,
            controls_suspended: false,
        };
        if recovered {
            runtime.notify("Recovered the live session. Lua state was lost; start scripts explicitly when needed.");
        }
        Ok(runtime)
    }
    pub fn state(&self) -> State {
        self.client.state()
    }
    pub fn client(&self) -> &Client {
        &self.client
    }
    pub fn startup_script_ids(&self) -> &[Option<ScriptId>] {
        &self.startup_ids
    }
    pub fn snapshot(&self) -> Arc<AppSnapshot> {
        Arc::clone(&self.snapshot)
    }
    pub fn has_pending_commands(&self) -> bool {
        !self.intents.is_empty() || !self.commands.is_empty()
    }
    pub fn take_file_output(&mut self) -> Option<ApplicationFileOutput> {
        self.outputs.pop_front()
    }
    pub fn take_audio_preview(&self) -> Option<ApplicationAudioPreview> {
        None
    }
    pub fn dispatch(&mut self, intent: AppIntent) -> Result<(), Error> {
        if self.client.state() != State::Ready || self.controls_suspended {
            return Err(Error::Disconnected);
        }
        if self.intents.len() == QUEUE_CAPACITY {
            return Err(Error::Full);
        }
        self.intents.push_back(intent);
        Ok(())
    }
    pub fn fail(&mut self, message: String) {
        if self.client.state() != State::Lost {
            self.notify(message);
        }
        self.client.disconnect();
    }
    pub fn notify(&mut self, message: impl Into<String>) {
        if self.notifications.len() == 64 {
            self.notifications.pop_front();
        }
        let mut message = message.into();
        if message.len() > 1024 {
            let mut end = 1024;
            while !message.is_char_boundary(end) {
                end -= 1;
            }
            message.truncate(end);
        }
        self.notifications.push_back(AppNotification {
            id: self.next_notice,
            level: NotificationLevel::Warning,
            message,
        });
        self.next_notice = self.next_notice.saturating_add(1);
    }
    pub fn tick(&mut self, elapsed: Duration) {
        let _span = tracing::trace_span!("app.client.tick").entered();
        if self.client.state() != State::Lost {
            if let Err(error) = self.delivery.poll(&mut self.client) {
                self.lost(error);
            }
        }
        if let Some(message) = self.client.ready_message() {
            match self.delivery.send(message) {
                Ok(()) => {
                    let _ = self.client.ready_sent();
                }
                Err(Error::Full) => {}
                Err(error) => self.lost(error),
            }
        }
        if self.client.state() != State::Ready {
            self.project();
            return;
        }
        self.reconcile_results();
        if self.controls_suspended {
            self.project();
            return;
        }
        let control = self.control_snapshot();
        self.scripts.set_control_snapshot(control);
        if let Some(startup) = self.startup.take() {
            let startup = match startup {
                Startup::IfFresh(scripts)
                    if self
                        .client
                        .authority()
                        .is_some_and(|authority| authority.generation.0 == 1) =>
                {
                    Startup::Fresh(scripts)
                }
                Startup::IfFresh(mut scripts) => {
                    self.recovered = true;
                    self.notify("Recovered the live session. Lua state was lost; start scripts explicitly when needed.");
                    for script in &mut scripts {
                        script.enabled = false;
                    }
                    Startup::Fresh(scripts)
                }
                startup => startup,
            };
            if let Startup::Fresh(scripts) = startup {
                for script in scripts {
                    let result = if let Some(identity) = script.identity {
                        self.scripts.add_catalog_script(
                            identity,
                            script.source,
                            script.kind,
                            script.enabled,
                            script.source_path,
                            None,
                        )
                    } else {
                        self.scripts.add_with_source_path(
                            script.name,
                            script.source,
                            script.kind,
                            script.enabled,
                            script.source_path,
                        )
                    };
                    match result {
                        Ok(id) => self.startup_ids.push(Some(id)),
                        Err(error) => {
                            self.startup_ids.push(None);
                            self.notify(error.to_string());
                        }
                    }
                }
                self.queue_script_operations();
            }
        }
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some(intent) = self.intents.pop_front() else {
                break;
            };
            if let Err(error) = self.apply_intent(intent) {
                self.notify(error.to_string());
            }
            self.queue_script_operations();
        }
        self.advance_scripting(elapsed);
        self.queue_script_operations();
        for _ in 0..MAX_MESSAGES_PER_TURN {
            let Some((command, creation)) = self.commands.front().cloned() else {
                break;
            };
            let resetting = matches!(command, Command::NewSession);
            let message = match self.client.command(command, None) {
                Ok(message) => message,
                Err(Error::Full) => break,
                Err(error) => {
                    self.lost(error);
                    break;
                }
            };
            let ClientMessage::Submit { operation, .. } = &message else {
                unreachable!()
            };
            let operation = *operation;
            match self.delivery.send(message) {
                Ok(()) => {
                    self.commands.pop_front();
                    if resetting {
                        self.reset_operation = Some(operation);
                        self.reset_queued = false;
                    }
                    if let Some(creation) = creation {
                        self.creations.insert(operation, creation);
                    }
                }
                Err(Error::Full) => {
                    self.client.unsent(operation);
                    break;
                }
                Err(error) => {
                    self.client.unsent(operation);
                    self.lost(error);
                    break;
                }
            }
        }
        self.update_media();
        self.project();
    }
    fn stop_scripts(&mut self) {
        for script in self.scripts.states() {
            let _ = self.scripts.stop(script.id);
        }
    }
    fn lost(&mut self, error: Error) {
        if !self.loss_reported {
            self.notify(format!(
                "Session connection lost ({error:?}). Admitted work remains with its host."
            ));
            self.stop_scripts();
            self.loss_reported = true;
        }
        self.client.disconnect();
        self.requested_media = None;
        self.intents.clear();
        self.commands.clear();
        if let Some(task) = &mut self.io_task {
            if task.status == IoTaskStatus::Running {
                task.status = IoTaskStatus::Failed;
                task.message =
                    "Connection lost; use the next session snapshot to determine the outcome."
                        .into();
            }
        }
    }
    pub fn detach(&mut self) -> Result<(), Error> {
        let message = self.client.detach()?;
        self.stop_scripts();
        self.delivery.send(message)
    }
    pub fn shutdown(&mut self) -> Result<OperationId, Error> {
        let message = self.client.shutdown()?;
        let ClientMessage::Shutdown { operation, .. } = message else {
            unreachable!()
        };
        if let Err(error) = self.delivery.send(message) {
            self.client.unsent(operation);
            return Err(error);
        }
        Ok(operation)
    }
    pub fn prepare_shutdown(&mut self) -> Result<ClientMessage, Error> {
        let message = self.client.management_shutdown()?;
        self.controls_suspended = true;
        self.intents.clear();
        self.commands.clear();
        self.stop_scripts();
        Ok(message)
    }
    pub fn resume_controls(&mut self) {
        self.controls_suspended = false;
    }
    fn queue(&mut self, command: Command, creation: Option<(u64, bool)>) -> anyhow::Result<()> {
        if !command.bounded() {
            anyhow::bail!("command exceeds session protocol bounds");
        }
        if self.commands.len() == QUEUE_CAPACITY {
            anyhow::bail!("session command queue is full");
        }
        if matches!(command, Command::NewSession) {
            if self.reset_operation.is_some() || self.reset_queued {
                anyhow::bail!("a session reset is already pending");
            }
            self.reset_queued = true;
            self.io_task = Some(IoTaskState {
                id: TaskId::from_raw(self.next_notice),
                kind: IoTaskKind::LoadSession,
                status: IoTaskStatus::Running,
                progress: 0.0,
                message: "Starting a fresh session".into(),
                sample_rate_warning: None,
                audio_channel_mapping: None,
                audio_channel_selection: None,
            });
            self.next_notice = self.next_notice.saturating_add(1);
        }
        self.commands.push_back((command, creation));
        Ok(())
    }
    fn queue_script_operations(&mut self) {
        for operation in self.scripts.take_control_operations() {
            if let Err(error) = self.queue(Command::Control(operation), None) {
                self.notify(error.to_string());
            }
        }
    }
    fn reconcile_results(&mut self) {
        let results: Vec<_> = self
            .client
            .results()
            .iter()
            .map(|(id, outcome)| (*id, outcome.clone()))
            .collect();
        for (id, outcome) in results {
            if !self.seen_results.insert(id) {
                continue;
            }
            if self.reset_operation == Some(id) {
                if let Some(task) = &mut self.io_task {
                    task.progress = 1.0;
                    task.status = if matches!(outcome, Outcome::Refused { .. }) {
                        IoTaskStatus::Failed
                    } else {
                        IoTaskStatus::Completed
                    };
                    task.message = match &outcome {
                        Outcome::Refused { message, .. } => message.clone(),
                        _ => "Fresh session ready".into(),
                    };
                }
                self.reset_operation = None;
            }
            if let Some((request_id, bus)) = self.creations.remove(&id) {
                let success = !matches!(outcome, Outcome::Refused { .. });
                if bus {
                    if self.bus_results.len() == MAX_RETAINED_RESULTS {
                        self.bus_results.pop_front();
                    }
                    self.bus_results.push_back(BusCreationResult {
                        request_id,
                        success,
                    });
                } else {
                    if self.track_results.len() == MAX_RETAINED_RESULTS {
                        self.track_results.pop_front();
                    }
                    self.track_results.push_back(TrackCreationResult {
                        request_id,
                        success,
                    });
                }
            }
            if let Outcome::Refused { message, .. } = outcome {
                self.notify(message);
            }
        }
        self.seen_results
            .retain(|id| self.client.results().contains_key(id));
    }
    fn project(&mut self) {
        let mut view = self
            .client
            .snapshot()
            .map(|state| AppSnapshot {
                tracks: state.tracks.clone(),
                buses: Arc::clone(&state.buses),
                track_processors: Arc::clone(&state.track_processors),
                bus_processors: Arc::clone(&state.bus_processors),
                global_controls: state.global.clone(),
                status: state.status.clone(),
                audio_drivers: state.audio_drivers.clone(),
                capabilities: state.capabilities,
                connections: Arc::clone(&state.connections),
                ..Default::default()
            })
            .unwrap_or_default();
        view.details = self.details();
        view.io_task = self.io_task.clone();
        view.revision = self.snapshot.revision.saturating_add(1);
        view.notifications = self
            .notifications
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .into();
        view.track_creation_results = self
            .track_results
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .into();
        view.bus_creation_results = self.bus_results.iter().copied().collect::<Vec<_>>().into();
        view.scripting = Arc::new(ScriptingState {
            supported: true,
            scripts: self.scripts.states().into(),
            dialogs: self.scripts.dialogs().into(),
            ..Default::default()
        });
        view.connections = self.with_script_connections(&view.connections, &view.scripting.scripts);
        self.snapshot = Arc::new(view);
    }

    fn with_script_connections(
        &self,
        shared: &ConnectionViewState,
        scripts: &[ScriptState],
    ) -> Arc<ConnectionViewState> {
        let mut view = shared.clone();
        let mut ports = view.application_ports.to_vec();
        let mut hosts: BTreeMap<_, _> = view
            .host_ports
            .iter()
            .map(|host| (host.id.clone(), host.clone()))
            .collect();
        let mut links: BTreeSet<_> = view
            .confirmed_links
            .iter()
            .map(|link| (link.application_port_id, link.host_port_id.clone()))
            .collect();
        for script in scripts {
            for (registration, rule) in script.midi.rule_states.iter().enumerate() {
                let Ok(registration) = u32::try_from(registration) else {
                    continue;
                };
                let (direction, role, host_direction, endpoint_direction, label) =
                    match rule.direction {
                        ScriptMidiRuleDirection::Input => (
                            PortDirection::Input,
                            PortRole::MidiInput,
                            PortDirection::Output,
                            shoop_scripting::MidiEndpointDirection::Output,
                            "source",
                        ),
                        ScriptMidiRuleDirection::Output => (
                            PortDirection::Output,
                            PortRole::MidiOutput,
                            PortDirection::Input,
                            shoop_scripting::MidiEndpointDirection::Input,
                            "sink",
                        ),
                    };
                let port = PortId::from_raw(
                    (1 << 63)
                        | ((script.id.raw() & 0x7fff_ffff) << 32)
                        | u64::from(registration.saturating_add(1)),
                );
                ports.push(ApplicationPortState {
                    id: port,
                    owner: ApplicationPortOwner::LuaControl {
                        script_id: script.id,
                        registration,
                    },
                    name: format!(
                        "{}: MIDI {} {}",
                        script.name,
                        label,
                        registration.saturating_add(1)
                    ),
                    data_type: PortDataType::Midi,
                    direction,
                    role,
                    connection_policy: ConnectionPolicy::OwnerManaged,
                });
                for endpoint in rule.endpoints.iter() {
                    let host = HostPortId::new(shoop_scripting::midi_endpoint_host_id(
                        endpoint_direction,
                        &endpoint.id,
                    ));
                    hosts.entry(host.clone()).or_insert_with(|| HostPortState {
                        id: host.clone(),
                        name: endpoint.name.clone(),
                        data_type: PortDataType::Midi,
                        direction: host_direction,
                    });
                    if endpoint.connected {
                        links.insert((port, host));
                    }
                }
            }
        }
        view.application_ports = ports.into();
        view.host_ports = hosts.into_values().collect::<Vec<_>>().into();
        view.confirmed_links = links
            .into_iter()
            .map(
                |(application_port_id, host_port_id)| ConfirmedConnectionState {
                    application_port_id,
                    host_port_id,
                },
            )
            .collect::<Vec<_>>()
            .into();
        Arc::new(view)
    }

    fn update_media(&mut self) {
        if !self.media_refusal_notified {
            if let Some(Err(reason)) = self.client.media() {
                self.notify(format!(
                    "The selected loop's media view is unavailable ({reason:?})."
                ));
                self.media_refusal_notified = true;
            }
        }
        let selected = self.client.snapshot().and_then(|state| {
            let mut selected = state
                .tracks
                .iter()
                .flat_map(|track| &track.loops)
                .filter(|entry| entry.selected);
            let entry = selected.next()?;
            if selected.next().is_some() {
                return None;
            }
            Some((
                entry.id,
                entry.content_revision,
                entry.mode == LoopMode::Recording,
            ))
        });
        if self.requested_media == selected {
            return;
        }
        let message = if let Some((id, _, false)) = selected {
            self.client.read_media(id)
        } else {
            self.client.cancel_media()
        };
        match message.and_then(|message| self.delivery.send(message)) {
            Ok(()) => {
                self.requested_media = selected;
                self.media_refusal_notified = false;
            }
            Err(Error::Full) => {}
            Err(error) => self.lost(error),
        }
    }
    fn details(&self) -> Option<LoopDetailsState> {
        let state = self.client.snapshot()?;
        let mut selected = state
            .tracks
            .iter()
            .flat_map(|track| &track.loops)
            .filter(|entry| entry.selected);
        let entry = selected.next()?;
        if selected.next().is_some() {
            return None;
        }
        let data = self
            .client
            .media()
            .and_then(|result| result.as_ref().ok())
            .filter(|data| {
                data.loop_id == entry.id && data.content_revision == entry.content_revision
            });
        let played = matches!(
            entry.mode,
            LoopMode::Playing | LoopMode::PlayingDryThroughWet
        )
        .then_some((entry.position * entry.length_frames as f32) as i64);
        Some(LoopDetailsState {
            generation: entry.content_revision,
            loop_id: entry.id,
            title: entry.name.clone(),
            loading: data.is_none() && self.client.media().is_none(),
            midi_loading: data.is_none() && self.client.media().is_none(),
            channels: data
                .map(|data| {
                    data.audio
                        .iter()
                        .enumerate()
                        .map(|(index, channel)| WaveformChannelState {
                            id: ChannelId::from_raw((entry.id.raw() << 8) | (index as u64 + 1)),
                            label: format!("Audio {}", index + 1),
                            samples: Arc::clone(&channel.samples),
                            start_offset: i64::from(channel.start_offset)
                                + i64::from(channel.capture_alignment_frames),
                            preplay_samples: u64::from(channel.preplay),
                            loop_length: entry.length_frames,
                            played_sample: played,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            midi_channels: data
                .map(|data| {
                    data.midi
                        .iter()
                        .enumerate()
                        .map(|(index, channel)| MidiSequenceChannelState {
                            id: ChannelId::from_raw((entry.id.raw() << 8) | (index as u64 + 129)),
                            label: format!("MIDI {}", index + 1),
                            content_revision: channel.content_revision,
                            events: Arc::clone(&channel.events),
                            start_offset: i64::from(channel.start_offset),
                            preplay_samples: u64::from(channel.preplay),
                            loop_length: entry.length_frames,
                            played_sample: played,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            sync_loop_length: state
                .tracks
                .iter()
                .find(|track| track.is_sync)
                .and_then(|track| track.loops.first())
                .map_or(0, |entry| entry.length_frames),
            composite: None,
        })
    }
    fn apply_intent(&mut self, intent: AppIntent) -> anyhow::Result<()> {
        let command = match intent {
            AppIntent::AddTrack(spec) => Command::AddTrack(spec.into()),
            AppIntent::AddTrackWithTopology(spec) => {
                let creation = spec.creation_request_id.map(|id| (id, false));
                return self.queue(Command::AddTrack(spec), creation);
            }
            AppIntent::AddBus(spec) => {
                let creation = spec.creation_request_id.map(|id| (id, true));
                return self.queue(Command::AddBus(spec), creation);
            }
            AppIntent::Track { track_id, action } => Command::Track {
                track: track_id,
                action,
            },
            AppIntent::Loop {
                track_id,
                loop_id,
                action,
            } => Command::Loop {
                track: track_id,
                loop_id,
                action,
            },
            AppIntent::Bus { bus_id, action } => Command::Bus {
                bus: bus_id,
                action,
            },
            AppIntent::AddLoop { track_id } => Command::AddLoop { track: track_id },
            AppIntent::Global(action) => Command::Global(action),
            AppIntent::Piano(action) => Command::Piano(action),
            AppIntent::SetPortConnected {
                port_id,
                host_port_id,
                connected,
            } => Command::SetPortConnected {
                port: port_id,
                host_port: host_port_id,
                connected,
            },
            AppIntent::SetMixerRouteConnected {
                source_port_id,
                destination_channel_id,
                connected,
            } => Command::SetMixerRoute {
                source: source_port_id,
                destination: destination_channel_id,
                connected,
            },
            AppIntent::SetMasterAutoConnect(value) => Command::SetMasterAutoConnect(value),
            AppIntent::SetLoopSmoothingMs(value) => Command::SetLoopSmoothingMs(value),
            AppIntent::ResetXruns => Command::ResetXruns,
            AppIntent::RequestNewSession => Command::NewSession,
            AppIntent::KeyEvent(event) => {
                self.scripts
                    .dispatch_key_event(shoop_scripting::ScriptKeyEvent {
                        event_type: match event.event_type {
                            KeyEventType::Pressed => 0,
                            KeyEventType::Released => 1,
                        },
                        key: event.key,
                        modifiers: event.modifiers,
                    });
                return Ok(());
            }
            AppIntent::AddScriptSource {
                name,
                source,
                kind,
                enabled,
            } => {
                self.scripts.add(name, source.to_string(), kind, enabled)?;
                return Ok(());
            }
            AppIntent::AddScriptFileSource {
                name,
                source,
                source_path,
                kind,
                enabled,
            } => {
                self.scripts.add_with_source_path(
                    name,
                    source.to_string(),
                    kind,
                    enabled,
                    Some(source_path),
                )?;
                return Ok(());
            }
            AppIntent::AddEphemeralScript {
                name,
                source,
                source_path,
            } => {
                self.scripts.add_ephemeral_with_source_path(
                    name,
                    source.to_string(),
                    source_path,
                )?;
                return Ok(());
            }
            AppIntent::ReconcileCatalogScripts {
                scripts,
                preserve_identities,
            } => {
                let mut scripts = scripts.to_vec();
                if self.recovered {
                    for script in &mut scripts {
                        script.enabled &=
                            self.explicitly_started_scripts.contains(&script.identity);
                    }
                }
                self.scripts
                    .reconcile_catalog_scripts(&scripts, &preserve_identities)?;
                return Ok(());
            }
            AppIntent::SetScriptEnabled { script_id, enabled } => {
                self.allow_script_restart(script_id, enabled);
                self.scripts.set_enabled(script_id, enabled)?;
                return Ok(());
            }
            AppIntent::RestartScript { script_id } => {
                self.allow_script_restart(script_id, true);
                self.scripts.stop(script_id)?;
                self.scripts.start(script_id)?;
                return Ok(());
            }
            AppIntent::ReplaceScriptSource { script_id, source } => {
                self.scripts
                    .replace_user_source(script_id, source.to_string())?;
                return Ok(());
            }
            AppIntent::StopScript { script_id } => {
                self.scripts.stop(script_id)?;
                return Ok(());
            }
            AppIntent::ForgetScript { script_id } => {
                self.allow_script_restart(script_id, false);
                self.scripts.forget(script_id)?;
                return Ok(());
            }
            AppIntent::ConvertScriptKind { script_id, kind } => {
                self.scripts.convert_kind(script_id, kind)?;
                return Ok(());
            }
            AppIntent::RemoveSessionScript { script_id } => {
                self.scripts.remove_session_script(script_id)?;
                return Ok(());
            }
            AppIntent::InvokeScriptDialogButton {
                script_id,
                dialog_id,
                button_id,
            } => {
                self.scripts
                    .invoke_dialog_button(script_id, dialog_id, button_id)?;
                return Ok(());
            }
            AppIntent::ExportScript { script_id } => {
                if self.outputs.len() == 8 {
                    anyhow::bail!("file output queue is full");
                }
                let (name, source) = self.scripts.source(script_id)?;
                self.outputs.push_back(ApplicationFileOutput {
                    task_id: TaskId::from_raw(self.next_notice),
                    suggested_name: format!("{name}.lua"),
                    mime_type: "text/x-lua".into(),
                    bytes: Arc::from(source.as_bytes()),
                });
                self.next_notice = self.next_notice.saturating_add(1);
                return Ok(());
            }
            AppIntent::DismissNotification { id } => {
                self.notifications.retain(|notice| notice.id != id);
                return Ok(());
            }
            AppIntent::FailIoTask { message, .. } | AppIntent::FailIoWorkflow { message, .. } => {
                self.notify(message);
                return Ok(());
            }
            AppIntent::StopClickTrackPreview
            | AppIntent::CompleteClickTrackPreview { .. }
            | AppIntent::ClickTrackPreviewStarted { .. } => return Ok(()),
            other => {
                let capability = match other {
                    AppIntent::RequestSaveSession
                    | AppIntent::RequestLoopAudioExport { .. }
                    | AppIntent::RequestLoopMidiExport { .. } => BackendCapability::SessionCapture,
                    AppIntent::RequestLoadSessionPicker
                    | AppIntent::RequestLoadSessionUrl
                    | AppIntent::LoadSessionBytes { .. } => BackendCapability::SessionContentLoad,
                    AppIntent::RequestLoopAudioImportPicker { .. }
                    | AppIntent::ImportLoopAudioBytes { .. }
                    | AppIntent::RequestLoopMidiImportPicker { .. }
                    | AppIntent::ImportLoopMidiBytes { .. }
                    | AppIntent::GenerateClickTrack { .. } => BackendCapability::LoopContentImport,
                    AppIntent::SetLoopTimeline { .. } => BackendCapability::LoopTimingEdits,
                    AppIntent::SetTrackLatency { .. } => BackendCapability::LatencyCompensation,
                    AppIntent::SetTakeAlignment { .. }
                    | AppIntent::SetTakeProcessorAlignment { .. } => {
                        BackendCapability::TakeAlignment
                    }
                    AppIntent::ComposeLoopSerial { .. }
                    | AppIntent::ComposeLoopAt { .. }
                    | AppIntent::DeleteCompositeEvents { .. }
                    | AppIntent::RelocateCompositeEvents { .. }
                    | AppIntent::SetCompositeLoopCycles { .. }
                    | AppIntent::SetCompositeKind { .. }
                    | AppIntent::SetCompositeEventMode { .. } => BackendCapability::CompositeLoops,
                    AppIntent::RequestAudioDriverSwitch { .. }
                    | AppIntent::RefreshAudioDriverDiscovery { .. }
                    | AppIntent::ConfirmAudioDriverSwitch { .. }
                    | AppIntent::CompleteAudioDriverSwitchPersistence { .. } => {
                        BackendCapability::AudioDriverSwitching
                    }
                    _ => anyhow::bail!("{} is unavailable with this engine", other.kind()),
                };
                anyhow::bail!(capability.unavailable_reason());
            }
        };
        self.queue(command, None)
    }
    fn allow_script_restart(&mut self, id: ScriptId, enabled: bool) {
        if let Some(identity) = self
            .scripts
            .states()
            .iter()
            .find(|script| script.id == id)
            .and_then(|script| script.identity.as_ref())
        {
            if enabled {
                self.explicitly_started_scripts.insert(identity.to_string());
            } else {
                self.explicitly_started_scripts.remove(identity.as_ref());
            }
        }
    }
    fn control_snapshot(&self) -> ControlSnapshot {
        use shoop_scripting::{ControlBus, ControlLoop, ControlTrack};
        let Some(state) = self.client.snapshot() else {
            return ControlSnapshot::default();
        };
        let mut tracks = Vec::new();
        let mut loops = Vec::new();
        let mut main_index = 0;
        for track in &state.tracks {
            let index = if track.is_sync {
                -1
            } else {
                let index = main_index;
                main_index += 1;
                index
            };
            let controls = &track.controls;
            tracks.push(ControlTrack {
                id: track.id,
                index,
                output_gain_db: controls.output_gain_db,
                output_balance: controls.output_balance,
                output_muted: controls.output_muted,
                input_gain_db: controls.input_gain_db,
                input_balance: controls.input_balance,
                input_muted: !controls.input_monitoring,
            });
            for (row, entry) in track.loops.iter().enumerate() {
                loops.push(ControlLoop {
                    id: entry.id,
                    coords: [index, row as i64],
                    default_playback_mode: LoopMode::Playing,
                    mode: entry.mode,
                    next_mode: entry.next_transition_delay.and_then(|_| {
                        (entry.next_mode != LoopMode::Unknown).then_some(entry.next_mode)
                    }),
                    next_mode_delay: entry.next_transition_delay,
                    length: entry.length_frames.min(u64::from(u32::MAX)) as u32,
                    gain: entry.gain,
                    balance: entry.balance,
                    selected: entry.selected,
                    targeted: entry.targeted,
                });
            }
        }
        ControlSnapshot {
            loops,
            tracks,
            buses: state
                .buses
                .iter()
                .enumerate()
                .map(|(index, bus)| ControlBus {
                    id: bus.id,
                    index: index as i64,
                    channel_count: bus.channels.len(),
                    gain_db: bus.gain_db,
                    balance: bus.balance,
                    muted: bus.muted,
                })
                .collect(),
            apply_n_cycles: state.global.apply_n_cycles,
            solo: state.global.solo,
            sync_active: state.global.sync,
            play_after_record: state.global.play_after_record,
            auto_mute_other_track_inputs: state.global.auto_mute_other_track_inputs,
            default_recording_action: state.global.default_recording_action,
        }
    }
    fn advance_scripting(&mut self, elapsed: Duration) {
        let current = self.control_snapshot();
        self.scripts.set_control_snapshot(current.clone());
        for loop_ in &current.loops {
            let previous = self
                .previous
                .loops
                .iter()
                .find(|candidate| candidate.id == loop_.id);
            let mut event_types = Vec::new();
            match previous {
                Some(previous) => {
                    if previous.mode != loop_.mode {
                        event_types.push(0);
                    }
                    if previous.length != loop_.length {
                        event_types.push(1);
                    }
                    if previous.selected != loop_.selected {
                        event_types.push(2);
                    }
                    if previous.targeted != loop_.targeted {
                        event_types.push(3);
                    }
                    if previous.coords != loop_.coords {
                        event_types.push(4);
                    }
                }
                None => event_types.extend([0, 1, 2, 3, 4]),
            }
            for event_type in event_types {
                self.scripts
                    .dispatch_loop_event(&shoop_scripting::ScriptLoopEvent {
                        coords: loop_.coords,
                        event_type,
                        mode: loop_.mode,
                        length: loop_.length,
                        selected: loop_.selected,
                        targeted: loop_.targeted,
                    });
            }
        }
        if current.apply_n_cycles != self.previous.apply_n_cycles
            || current.solo != self.previous.solo
            || current.sync_active != self.previous.sync_active
            || current.play_after_record != self.previous.play_after_record
            || current.auto_mute_other_track_inputs != self.previous.auto_mute_other_track_inputs
            || current.default_recording_action != self.previous.default_recording_action
        {
            self.scripts.dispatch_global_event();
        }
        self.scripts.advance_timers(elapsed);
        self.scripts.advance_midi(elapsed);
        self.previous = current;
    }
}
