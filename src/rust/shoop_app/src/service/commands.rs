use alloc::borrow::ToOwned;
use alloc::format;
use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;

use super::*;

mod control;

type CommandResult = core::result::Result<Effect, CommandError>;

impl Service {
    pub(super) fn execute(&mut self, command: Command) -> CommandResult {
        match command {
            Command::Composite(edit) => self.edit_composite(edit),
            Command::SetProfiling(enabled) => {
                self.backend.set_profiling(enabled)?;
                Ok(Effect::None)
            }
            Command::SetHostGraphConnected(request) => {
                let controller =
                    self.host_graph_controller
                        .as_mut()
                        .ok_or_else(|| CommandError {
                            reason: Refusal::Unsupported,
                            message: "the active host does not support graph control".into(),
                        })?;
                controller
                    .submit(request.into())
                    .map(Effect::HostGraph)
                    .map_err(CommandError::graph)
            }
            Command::Preview(command) => self.preview_command(command),
            Command::CommitPersistence(key) => self.commit_persistence(key),
            Command::AddTrack(spec) => self.add_track(spec),
            Command::AddLoop { track } => {
                if self.loops.len() == MAX_LOOPS {
                    return Err(CommandError::invalid("session loop limit reached"));
                }
                let entry = self.track(track)?;
                if entry.view.is_sync {
                    return Err(CommandError::invalid("the sync track has one loop"));
                }
                let backend = entry.backend;
                let audio = entry.audio_channels;
                let midi = entry.midi;
                self.next_loop
                    .checked_add(1)
                    .ok_or_else(|| CommandError::invalid("session identity space exhausted"))?;
                let created = self.backend.add_loop_to_track(backend)?;
                let id = self.insert_loop(track, created, audio, midi, false)?;
                self.active_effect = Effect::LoopCreated(track, id);
                self.tracks.get_mut(&track).unwrap().loops.push(id);
                if self.global.sync {
                    let source = self.sync_backend();
                    self.backend.set_loop_sync_source(created, source)?;
                    if self.backend.composite_limits().is_some() {
                        self.backend.set_loop_repeat_sync(created, true)?;
                    }
                }
                Ok(Effect::LoopCreated(track, id))
            }
            Command::Track { track, action } => self.track_action(track, action),
            Command::Loop {
                track,
                loop_id,
                action,
            } => self.loop_action(track, loop_id, action),
            Command::Bus { bus, action } => self.bus_action(bus, action),
            Command::AddBus(spec) => {
                if spec.fx.is_some() {
                    return Err(CommandError::unsupported(BackendCapability::BusFx));
                }
                let request = BackendBusRequest {
                    name: spec.name,
                    channel_count: spec.channel_count,
                    fx: None,
                }
                .normalized()?;
                let limits = self.backend.bus_limits();
                if self.buses.len() >= limits.buses as usize
                    || request.channel_count > limits.channels
                {
                    return Err(CommandError::invalid("prepared bus capacity exceeded"));
                }
                self.next_bus
                    .checked_add(1)
                    .ok_or_else(|| CommandError::invalid("bus identity exhausted"))?;
                self.next_channel
                    .checked_add(u64::from(request.channel_count))
                    .ok_or_else(|| CommandError::invalid("channel identity exhausted"))?;
                self.next_port
                    .checked_add(u64::from(request.channel_count))
                    .ok_or_else(|| CommandError::invalid("port identity exhausted"))?;
                let created = self.backend.create_bus(request.clone())?;
                let id = self.insert_bus(created, request.name, StructuralState::Creating)?;
                Ok(Effect::BusCreated(id))
            }
            Command::Global(action) => {
                self.global_action(action)?;
                Ok(Effect::None)
            }
            Command::Piano(action) => {
                self.piano(action)?;
                Ok(Effect::None)
            }
            Command::SetPortConnected {
                port,
                host_port,
                connected,
            } => {
                if self.dynamic_ports()
                    && !self
                        .observed
                        .connections
                        .host_ports
                        .contains_key(host_port.as_str())
                    && self.graph_host(host_port.as_str()).is_none()
                {
                    return Err(CommandError {
                        reason: Refusal::NotReady,
                        message: "the external port is unavailable".into(),
                    });
                }
                self.connect_port(port, host_port.as_str(), connected)?;
                Ok(
                    if self.dynamic_ports()
                        && !self
                            .observed
                            .connections
                            .host_ports
                            .contains_key(host_port.as_str())
                    {
                        Effect::Connection(port, host_port.as_str().to_owned())
                    } else {
                        Effect::None
                    },
                )
            }
            Command::SetMixerRoute {
                source,
                destination,
                connected,
            } => {
                self.route(source, destination, connected)?;
                Ok(Effect::None)
            }
            Command::SetMasterAutoConnect(value) => {
                if !value {
                    self.master_graph_auto_done = false;
                }
                if value && !self.options.master_auto_connect {
                    self.connect_master()?;
                }
                self.options.master_auto_connect = value;
                Ok(Effect::None)
            }
            Command::SetLoopSmoothingMs(value) => {
                self.backend.set_loop_smoothing_ms(value)?;
                self.options.loop_smoothing_ms = value;
                Ok(Effect::None)
            }
            Command::SelectLoops { loops } => {
                let targets: BTreeSet<_> = loops.into_iter().collect();
                for id in &targets {
                    self.loop_entry(*id)?;
                }
                for (id, entry) in &mut self.loops {
                    entry.view.selected = targets.contains(id);
                    if entry.view.selected {
                        entry.view.targeted = false;
                    }
                }
                Ok(Effect::None)
            }
            Command::SetTarget(target) => {
                if let Some(target) = target {
                    self.loop_entry(target)?;
                }
                for (id, entry) in &mut self.loops {
                    entry.view.targeted = Some(*id) == target;
                    if entry.view.targeted {
                        entry.view.selected = false;
                    }
                }
                Ok(Effect::None)
            }
            Command::PlaySelected => {
                let selected = self
                    .ordered_loops()
                    .into_iter()
                    .filter(|id| self.loops[id].view.selected)
                    .collect();
                self.transition(
                    selected,
                    BackendLoopMode::Playing,
                    self.global.sync.then(|| self.target_delay()),
                    None,
                )?;
                Ok(Effect::None)
            }
            Command::RecordWithTarget { loops } => {
                let target = self
                    .loops
                    .values()
                    .find(|entry| entry.view.targeted)
                    .map(|entry| entry.backend)
                    .ok_or_else(|| {
                        CommandError::invalid("cannot record with targeted: no loop is targeted")
                    })?;
                for id in &loops {
                    if self.composites.contains_key(id) {
                        return Err(CommandError::invalid(
                            "a composite's outer mode is Play or Stop",
                        ));
                    }
                    if self.loop_entry(*id)?.backend == target {
                        return Err(CommandError::invalid("a loop cannot synchronize to itself"));
                    }
                }
                for id in loops {
                    let backend = self.loops[&id].backend;
                    self.backend.set_loop_sync_source(backend, Some(target))?;
                    self.backend
                        .transition_loop(backend, BackendLoopMode::Recording, Some(0))?;
                }
                Ok(Effect::None)
            }
            Command::Control(operation) => self.control_operation(operation),
            Command::NewSession => self.new_session_image(),
            Command::ResetXruns => {
                self.xruns = 0;
                self.observed.status.xruns = 0;
                Ok(Effect::None)
            }
        }
    }

    fn bus_action(&mut self, id: BusId, action: BusAction) -> CommandResult {
        let bus = self
            .buses
            .get(&id)
            .filter(|b| b.view.structural_state != StructuralState::Removing)
            .ok_or_else(|| CommandError::invalid("stale or unknown bus"))?;
        let backend = bus.backend;
        match action {
            BusAction::Remove => {
                self.backend.remove_bus(backend)?;
                self.buses
                    .get_mut(&id)
                    .expect("validated bus")
                    .view
                    .structural_state = StructuralState::Removing;
                Ok(Effect::BusRemoved(id))
            }
            BusAction::MoveBefore(target) => {
                if target.is_some_and(|t| !self.buses.contains_key(&t)) {
                    return Err(CommandError::invalid("unknown bus order target"));
                }
                if target != Some(id) {
                    self.bus_order.retain(|b| *b != id);
                    let index = target
                        .and_then(|t| self.bus_order.iter().position(|b| *b == t))
                        .unwrap_or(self.bus_order.len());
                    self.bus_order.insert(index, id);
                }
                Ok(Effect::None)
            }
            action => {
                let control = match action {
                    BusAction::GainChanged(v) => BackendBusControl::GainDb(v),
                    BusAction::BalanceChanged(v) => BackendBusControl::Balance(v),
                    BusAction::MuteChanged(v) => BackendBusControl::Mute(v),
                    _ => return Err(CommandError::unsupported(BackendCapability::BusFx)),
                };
                self.backend.set_bus_control(backend, control)?;
                self.buses
                    .get_mut(&id)
                    .expect("validated bus")
                    .view
                    .control_error = None;
                Ok(Effect::BusControl(id))
            }
        }
    }

    fn track(&self, id: TrackId) -> core::result::Result<&Track, CommandError> {
        self.tracks
            .get(&id)
            .filter(|track| track.view.structural_state != StructuralState::Removing)
            .ok_or_else(|| CommandError::invalid(format!("stale or unknown track {id}")))
    }
    pub(super) fn loop_entry(&self, id: LoopId) -> core::result::Result<&Loop, CommandError> {
        let entry = self
            .loops
            .get(&id)
            .ok_or_else(|| CommandError::invalid(format!("stale or unknown loop {id}")))?;
        self.track(entry.track)?;
        Ok(entry)
    }
    pub(super) fn sync_backend(&self) -> Option<BackendLoopId> {
        self.track_order
            .iter()
            .filter_map(|id| self.tracks.get(id))
            .find(|track| track.view.is_sync)
            .and_then(|track| track.loops.first())
            .and_then(|id| self.loops.get(id))
            .map(|entry| entry.backend)
    }

    fn add_track(&mut self, spec: TrackSpec) -> CommandResult {
        if self.tracks.len() == MAX_TRACKS {
            return Err(CommandError::invalid("session track limit reached"));
        }
        let (audio_channels, midi) = match spec.topology {
            TrackSpecTopology::Direct {
                audio_channels,
                midi,
            } => (audio_channels, midi),
            TrackSpecTopology::DryWet { .. } => {
                return Err(CommandError::unsupported(BackendCapability::DryWetTracks))
            }
        };
        spec.validate(&[])
            .map_err(|error| CommandError::invalid(format!("invalid track: {error:?}")))?;
        if spec.latency != TrackLatencySpec::default() {
            return Err(CommandError::unsupported(
                BackendCapability::LatencyCompensation,
            ));
        }
        let slot_count = self
            .tracks
            .values()
            .filter(|track| !track.view.is_sync)
            .map(|track| track.loops.len())
            .max()
            .unwrap_or(8)
            .max(8);
        if self.loops.len() + slot_count > MAX_LOOPS {
            return Err(CommandError::invalid("session loop limit reached"));
        }
        self.next_track
            .checked_add(1)
            .ok_or_else(|| CommandError::invalid("session identity space exhausted"))?;
        self.next_loop
            .checked_add(slot_count as u64)
            .ok_or_else(|| CommandError::invalid("session identity space exhausted"))?;
        self.next_port
            .checked_add(u64::from(audio_channels) * 2 + u64::from(midi) * 2)
            .ok_or_else(|| CommandError::invalid("session identity space exhausted"))?;
        let mut base: String = spec
            .name
            .trim()
            .to_lowercase()
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        if base.is_empty() {
            base = "track".into();
        }
        if self
            .ports
            .values()
            .any(|port| port.view.name.starts_with(&format!("{base}_direct_")))
        {
            base = format!("{base}_{}", self.next_track);
        }
        let created = self.backend.create_direct_track(DirectTrackRequest {
            port_name_base: base,
            audio_channels,
            midi,
            initial_loops: slot_count,
        })?;
        let (id, loops) = self.insert_track(created, spec.name, audio_channels, midi, false)?;
        self.active_effect = Effect::TrackCreated(id, loops.clone());
        if self.global.sync {
            let source = self.sync_backend();
            for loop_id in &loops {
                self.backend
                    .set_loop_sync_source(self.loops[loop_id].backend, source)?;
                if self.backend.composite_limits().is_some() {
                    self.backend
                        .set_loop_repeat_sync(self.loops[loop_id].backend, true)?;
                }
            }
        }
        self.connect_track_inputs(id)?;
        for name in &spec.initial_output_bus_names {
            self.route_track_to_bus(id, name)?;
        }
        Ok(Effect::TrackCreated(id, loops))
    }

    fn track_action(&mut self, id: TrackId, action: TrackAction) -> CommandResult {
        let track = self.track(id)?;
        let backend = track.backend;
        let control = match action {
            TrackAction::NameChanged(name) => {
                self.tracks.get_mut(&id).unwrap().view.name = name;
                return Ok(Effect::None);
            }
            TrackAction::MoveBefore(target) => {
                if track.view.is_sync {
                    return Err(CommandError::invalid("the sync track cannot be reordered"));
                }
                if let Some(target) = target {
                    if self.track(target)?.view.is_sync {
                        return Err(CommandError::invalid("tracks follow the sync track"));
                    }
                }
                if target == Some(id) {
                    return Ok(Effect::None);
                }
                self.track_order.retain(|track| *track != id);
                let index = target
                    .and_then(|target| self.track_order.iter().position(|id| *id == target))
                    .unwrap_or(self.track_order.len());
                self.track_order.insert(index, id);
                return Ok(Effect::None);
            }
            TrackAction::Remove => {
                if track.view.is_sync {
                    return Err(CommandError::invalid("the sync track cannot be removed"));
                }
                return self.remove_track_with_composites(id);
            }
            TrackAction::DefaultPlaybackModeChanged(DefaultPlaybackMode::Regular) => {
                return Ok(Effect::None)
            }
            TrackAction::DefaultPlaybackModeChanged(_) => {
                return Err(CommandError::unsupported(BackendCapability::DryWetModes))
            }
            TrackAction::OutputGainChanged(value) => BackendTrackControl::OutputGainDb(value),
            TrackAction::OutputBalanceChanged(value) => BackendTrackControl::OutputBalance(value),
            TrackAction::OutputMuteChanged(value) => BackendTrackControl::OutputMute(value),
            TrackAction::InputGainChanged(value) => BackendTrackControl::InputGainDb(value),
            TrackAction::InputBalanceChanged(value) => BackendTrackControl::InputBalance(value),
            TrackAction::InputMonitoringChanged {
                enabled,
                respect_auto_mute,
            } => {
                if enabled && respect_auto_mute && self.global.auto_mute_other_track_inputs {
                    let others: Vec<_> = self
                        .tracks
                        .iter()
                        .filter_map(|(other, track)| {
                            (*other != id && !track.view.is_sync).then_some(*other)
                        })
                        .collect();
                    for other in others {
                        self.set_input_monitoring(other, false)?;
                    }
                }
                self.set_input_monitoring(id, enabled)?;
                return Ok(Effect::None);
            }
            _ => return Err(CommandError::unsupported(BackendCapability::TrackFx)),
        };
        self.backend.set_track_control(backend, control)?;
        Ok(Effect::None)
    }

    fn loop_action(&mut self, track: TrackId, id: LoopId, action: LoopAction) -> CommandResult {
        let entry = self.loop_entry(id)?;
        if let LoopAction::ComposeIntoEnd(target) | LoopAction::ComposeIntoParallel(target) = action
        {
            if self.loop_entry(target)?.track != track {
                return Err(CommandError::invalid(
                    "composition target does not belong to the requested track",
                ));
            }
            return self.edit_composite(shoop_app_protocol::composite::Edit::Append {
                target,
                add: vec![id],
                parallel: matches!(action, LoopAction::ComposeIntoParallel(_)),
                convert_if_empty: true,
            });
        }
        if entry.track != track {
            return Err(CommandError::invalid(
                "loop does not belong to the requested track",
            ));
        }
        let backend = entry.backend;
        match action {
            LoopAction::ConvertToComposite => {
                return self
                    .edit_composite(shoop_app_protocol::composite::Edit::Convert { target: id })
            }
            LoopAction::NameChanged(name) => {
                self.loops.get_mut(&id).unwrap().view.name = name;
            }
            LoopAction::IconClicked(modifiers) => {
                let selected = entry.view.selected;
                let targeted = entry.view.targeted;
                if !modifiers.additive && !selected && !targeted {
                    for entry in self.loops.values_mut() {
                        entry.view.selected = false;
                    }
                }
                let view = &mut self.loops.get_mut(&id).unwrap().view;
                view.selected = !selected && !targeted;
                view.targeted = false;
            }
            LoopAction::IconDoubleClicked => {
                let targeted = entry.view.targeted;
                for entry in self.loops.values_mut() {
                    entry.view.targeted = false;
                }
                let view = &mut self.loops.get_mut(&id).unwrap().view;
                view.targeted = !targeted;
                if view.targeted {
                    view.selected = false;
                }
            }
            LoopAction::DefaultClicked => {
                let view = &entry.view;
                let mode = if view.next_transition_delay.is_some()
                    && !matches!(view.next_mode, LoopMode::Unknown | LoopMode::Stopped)
                {
                    BackendLoopMode::Stopped
                } else if view.mode == LoopMode::Recording {
                    BackendLoopMode::Playing
                } else if self.composites.contains_key(&id) {
                    if view.mode == LoopMode::Playing {
                        BackendLoopMode::Stopped
                    } else {
                        BackendLoopMode::Playing
                    }
                } else if view.empty && view.mode == LoopMode::Stopped {
                    BackendLoopMode::Recording
                } else if !view.empty && view.mode == LoopMode::Stopped {
                    BackendLoopMode::Playing
                } else {
                    BackendLoopMode::Stopped
                };
                if mode == BackendLoopMode::Recording
                    && self.global.default_recording_action == DefaultRecordingAction::Grab
                {
                    return Err(CommandError::unsupported(BackendCapability::Grab));
                }
                self.transition(
                    vec![id],
                    mode,
                    self.global.sync.then(|| self.target_delay()),
                    Some(self.global.apply_n_cycles),
                )?;
            }
            LoopAction::PlayClicked | LoopAction::RecordClicked | LoopAction::StopClicked => {
                let mode = match action {
                    LoopAction::PlayClicked => BackendLoopMode::Playing,
                    LoopAction::RecordClicked => BackendLoopMode::Recording,
                    _ => BackendLoopMode::Stopped,
                };
                let targets = if entry.view.selected {
                    self.ordered_loops()
                        .into_iter()
                        .filter(|id| self.loops[id].view.selected)
                        .collect()
                } else {
                    vec![id]
                };
                self.transition(
                    targets,
                    mode,
                    self.global.sync.then(|| self.target_delay()),
                    Some(self.global.apply_n_cycles),
                )?;
            }
            LoopAction::GainChanged(value) => self.backend.set_loop_gain(backend, value)?,
            LoopAction::BalanceChanged(value) => self.backend.set_loop_balance(backend, value)?,
            LoopAction::MoveBefore(target) => {
                if let Some(target) = target {
                    if self.loop_entry(target)?.track != track {
                        return Err(CommandError::invalid("loop order belongs to one track"));
                    }
                }
                if target == Some(id) {
                    return Ok(Effect::None);
                }
                let order = &mut self.tracks.get_mut(&track).unwrap().loops;
                order.retain(|loop_id| *loop_id != id);
                let index = target
                    .and_then(|target| order.iter().position(|id| *id == target))
                    .unwrap_or(order.len());
                order.insert(index, id);
            }
            LoopAction::SwapWith(other) => {
                if self.loop_entry(other)?.track != track {
                    return Err(CommandError::invalid(
                        "loops on different tracks cannot exchange ownership",
                    ));
                }
                let order = &mut self.tracks.get_mut(&track).unwrap().loops;
                let a = order.iter().position(|candidate| *candidate == id).unwrap();
                let b = order
                    .iter()
                    .position(|candidate| *candidate == other)
                    .unwrap();
                order.swap(a, b);
            }
            LoopAction::PlayDryClicked | LoopAction::RerecordClicked => {
                return Err(CommandError::unsupported(BackendCapability::DryWetModes))
            }
            LoopAction::GrabClicked => {
                return Err(CommandError::unsupported(BackendCapability::Grab))
            }
            LoopAction::RestoreRecordedFxState => {
                return Err(CommandError::unsupported(BackendCapability::TrackFx))
            }
            LoopAction::ComposeIntoEnd(_) | LoopAction::ComposeIntoParallel(_) => {
                return Err(CommandError::unsupported(BackendCapability::CompositeLoops))
            }
            LoopAction::Duplicate | LoopAction::DuplicateTo(_) => {
                return Err(CommandError::unsupported(
                    BackendCapability::LoopDuplication,
                ))
            }
        }
        Ok(Effect::None)
    }

    fn ordered_loops(&self) -> Vec<LoopId> {
        self.track_order
            .iter()
            .filter_map(|id| self.tracks.get(id))
            .flat_map(|track| track.loops.iter().copied())
            .collect()
    }

    fn target_delay(&self) -> u32 {
        let Some(target) = self.loops.values().find(|entry| entry.view.targeted) else {
            return 0;
        };
        if let Some(delay) = target.view.next_transition_delay {
            return delay;
        }
        let sync = self
            .loops
            .values()
            .find(|entry| entry.view.sync)
            .map_or(0, |entry| entry.view.length_frames);
        let Some(observed) = self.observed.loops.get(&target.backend) else {
            return 0;
        };
        if sync == 0 {
            0
        } else {
            u32::try_from(u64::from(observed.length.saturating_sub(observed.position)) / sync)
                .unwrap_or(u32::MAX)
        }
    }

    fn transition(
        &mut self,
        ids: Vec<LoopId>,
        mode: BackendLoopMode,
        delay: Option<u32>,
        cycles: Option<u32>,
    ) -> core::result::Result<(), CommandError> {
        let targets: BTreeSet<_> = ids.into_iter().collect();
        let mut tracks = BTreeSet::new();
        for id in &targets {
            self.require_restored_composite(*id, mode)?;
            tracks.insert(self.loop_entry(*id)?.track);
            if self.composites.contains_key(id)
                && mode != BackendLoopMode::Stopped
                && mode != BackendLoopMode::Playing
            {
                return Err(CommandError::invalid("a composite's outer mode is Play or Stop; use explicit event modes to record children"));
            }
        }
        if self.global.solo && matches!(mode, BackendLoopMode::Playing | BackendLoopMode::Recording)
        {
            let others: Vec<_> = self
                .loops
                .iter()
                .filter_map(|(id, entry)| {
                    (tracks.contains(&entry.track) && !targets.contains(id))
                        .then_some(entry.backend)
                })
                .collect();
            for other in others {
                self.backend
                    .transition_loop(other, BackendLoopMode::Stopped, delay)?;
            }
        }
        for id in self
            .ordered_loops()
            .into_iter()
            .filter(|id| targets.contains(id))
        {
            let backend = self.loops[&id].backend;
            self.backend.transition_loop(backend, mode, delay)?;
            if mode == BackendLoopMode::Recording && cycles.unwrap_or(0) > 0 {
                let next = if self.global.play_after_record {
                    BackendLoopMode::Playing
                } else {
                    BackendLoopMode::Stopped
                };
                self.backend.transition_loop(
                    backend,
                    next,
                    Some(delay.unwrap_or(0).saturating_add(cycles.unwrap())),
                )?;
            }
        }
        Ok(())
    }

    fn global_action(
        &mut self,
        action: GlobalControlAction,
    ) -> core::result::Result<(), CommandError> {
        match action {
            GlobalControlAction::StopAll => self.transition(
                self.ordered_loops(),
                BackendLoopMode::Stopped,
                self.global.sync.then_some(0),
                None,
            )?,
            GlobalControlAction::MidiPanic => self.midi_panic(false)?,
            GlobalControlAction::DeselectAll => {
                for entry in self.loops.values_mut() {
                    entry.view.selected = false;
                }
            }
            GlobalControlAction::ClearAll { include_sync }
            | GlobalControlAction::ClearRecordings { include_sync } => {
                let targets = self
                    .loops
                    .iter()
                    .filter_map(|(id, entry)| {
                        (include_sync || !self.tracks[&entry.track].view.is_sync).then_some(*id)
                    })
                    .collect();
                self.clear_composite_slots(targets)?;
            }
            GlobalControlAction::SetDefaultRecordingAction(value) => {
                if value == DefaultRecordingAction::Grab {
                    return Err(CommandError::unsupported(BackendCapability::Grab));
                }
                self.global.default_recording_action = value;
            }
            GlobalControlAction::SetPlayAfterRecord(value) => self.global.play_after_record = value,
            GlobalControlAction::SetSync(value) => {
                let source = if value { self.sync_backend() } else { None };
                let targets: Vec<_> = self
                    .loops
                    .values()
                    .filter_map(|entry| {
                        (!self.tracks[&entry.track].view.is_sync).then_some(entry.backend)
                    })
                    .collect();
                for target in targets {
                    self.backend.set_loop_sync_source(target, source)?;
                    if self.backend.composite_limits().is_some() {
                        self.backend.set_loop_repeat_sync(target, value)?;
                    }
                }
                self.global.sync = value;
            }
            GlobalControlAction::SetSolo(value) => self.global.solo = value,
            GlobalControlAction::SetAutoMuteOtherTrackInputs(value) => {
                self.global.auto_mute_other_track_inputs = value
            }
            GlobalControlAction::SetAutoArmTrackInputs(value) => {
                self.global.auto_arm_track_inputs = value
            }
            GlobalControlAction::SetApplyNCycles(value) => self.global.apply_n_cycles = value,
        }
        Ok(())
    }

    fn piano(&mut self, action: PianoAction) -> core::result::Result<(), CommandError> {
        if !self
            .backend
            .capabilities()
            .supports(BackendCapability::MidiInjection)
        {
            return Err(CommandError::unsupported(BackendCapability::MidiInjection));
        }
        match action {
            PianoAction::Press(note) => {
                if self.piano_notes.contains_key(&note.value()) {
                    return Ok(());
                }
                let tracks: BTreeSet<_> =
                    self.tracks
                        .iter()
                        .filter_map(|(id, track)| {
                            (track.midi
                                && track.monitoring_intent.map_or(
                                    track.view.controls.input_monitoring,
                                    |(_, desired)| desired,
                                ))
                            .then_some(*id)
                        })
                        .collect();
                let active: usize = self.piano_notes.values().map(BTreeSet::len).sum();
                if active + tracks.len() > 512 {
                    return Err(CommandError {
                        reason: Refusal::Overloaded,
                        message: "piano voice tracking is full".into(),
                    });
                }
                for id in &tracks {
                    self.backend.inject_midi_input(
                        self.tracks[id].backend,
                        &[BackendMidiEvent {
                            frame: 0,
                            data: vec![0x90, note.value(), 100].into(),
                        }],
                    )?;
                    self.piano_notes
                        .entry(note.value())
                        .or_default()
                        .insert(*id);
                }
            }
            PianoAction::Release(note) => {
                if let Some(tracks) = self.piano_notes.get(&note.value()).cloned() {
                    for id in tracks {
                        if let Some(track) = self.tracks.get(&id) {
                            self.backend.inject_midi_input(
                                track.backend,
                                &[BackendMidiEvent {
                                    frame: 0,
                                    data: vec![0x80, note.value(), 0].into(),
                                }],
                            )?;
                        }
                    }
                    self.piano_notes.remove(&note.value());
                }
            }
            PianoAction::ReleaseAll => {
                let notes: Vec<_> = self.piano_notes.keys().copied().collect();
                for note in notes {
                    self.piano(PianoAction::Release(MidiNote::new(note).unwrap()))?;
                }
            }
        }
        Ok(())
    }

    fn set_input_monitoring(&mut self, id: TrackId, enabled: bool) -> Result<(), CommandError> {
        let before = self.backend.mutation_cursor();
        self.backend.set_track_control(
            self.tracks[&id].backend,
            BackendTrackControl::InputMonitoring(enabled),
        )?;
        let sequence = self.backend.mutation_cursor();
        self.tracks.get_mut(&id).unwrap().monitoring_intent =
            (sequence > before).then_some((sequence, enabled));
        Ok(())
    }
    fn midi_panic(&mut self, all_tracks: bool) -> Result<(), CommandError> {
        let tracks: Vec<_> = self
            .tracks
            .values()
            .filter(|track| {
                track.midi
                    && (all_tracks
                        || track
                            .monitoring_intent
                            .map_or(track.view.controls.input_monitoring, |(_, desired)| desired))
            })
            .map(|track| track.backend)
            .collect();
        let events: Vec<_> = (0..16)
            .map(|channel| BackendMidiEvent {
                frame: 0,
                data: vec![0xb0 | channel, 120, 0].into(),
            })
            .collect();
        for track in tracks {
            self.backend.inject_midi_input(track, &events)?;
        }
        self.piano_notes.clear();
        Ok(())
    }

    pub(super) fn prepare_shutdown(&mut self) -> CommandResult {
        if self.engine_closed {
            return Ok(Effect::Shutdown);
        }
        if self
            .backend
            .capabilities()
            .supports(BackendCapability::MidiInjection)
        {
            self.piano(PianoAction::ReleaseAll)?;
            self.midi_panic(true)?;
        }
        self.transition(self.ordered_loops(), BackendLoopMode::Stopped, None, None)?;
        Ok(Effect::Shutdown)
    }

    fn connect_port(
        &mut self,
        id: PortId,
        host: &str,
        connected: bool,
    ) -> core::result::Result<(), CommandError> {
        let port = self
            .ports
            .get(&id)
            .ok_or_else(|| CommandError::invalid("stale or unknown port"))?;
        if self.dynamic_ports() && matches!(port.view.owner, ApplicationPortOwner::GlobalFxControl)
        {
            return Err(CommandError {
                reason: Refusal::Unsupported,
                message: "the active backend does not provide global FX control MIDI".into(),
            });
        }
        if self.connection_intent.len() == MAX_BUS_HOST_LINKS
            && !self.connection_intent.contains_key(&(id, host.to_owned()))
        {
            return Err(CommandError::invalid("connection intent limit reached"));
        }
        if self.dynamic_ports() && !self.observed.connections.host_ports.contains_key(host) {
            self.connection_intent.insert(
                (id, host.to_owned()),
                Intent {
                    connected,
                    pending: None,
                    error: None,
                },
            );
            return Ok(());
        }
        let before = self.backend.mutation_cursor();
        self.backend
            .set_port_connected(port.backend, host, connected)?;
        self.connection_intent.insert(
            (id, host.to_owned()),
            Intent {
                connected,
                pending: (self.backend.mutation_cursor() > before)
                    .then_some(self.backend.mutation_cursor()),
                error: None,
            },
        );
        Ok(())
    }
    fn route(
        &mut self,
        source: PortId,
        destination: BusChannelId,
        connected: bool,
    ) -> core::result::Result<(), CommandError> {
        let source_backend = self
            .ports
            .get(&source)
            .ok_or_else(|| CommandError::invalid("unknown source port"))?
            .backend;
        let destination_backend = *self
            .channels
            .get(&destination)
            .ok_or_else(|| CommandError::invalid("unknown destination channel"))?;
        let route = MixerRouteState {
            source_port_id: source,
            destination_channel_id: destination,
        };
        if self.route_intent.len() == MAX_MIXER_ROUTES && !self.route_intent.contains_key(&route) {
            return Err(CommandError::invalid("route intent limit reached"));
        }
        let before = self.backend.mutation_cursor();
        self.backend
            .set_mixer_route(source_backend, destination_backend, connected)?;
        self.route_intent.insert(
            route,
            Intent {
                connected,
                pending: (self.backend.mutation_cursor() > before)
                    .then_some(self.backend.mutation_cursor()),
                error: None,
            },
        );
        Ok(())
    }
    pub(super) fn connect_master(&mut self) -> Result<()> {
        if self.dynamic_ports() {
            let outputs: Vec<_> = self.ports.iter().filter_map(|(id, port)| matches!(port.view.owner, ApplicationPortOwner::Bus { bus_id } if self.buses.get(&bus_id).is_some_and(|bus| bus.backend == MASTER_BUS_ID)).then_some(*id)).collect();
            let mut groups: BTreeMap<String, Vec<String>> = BTreeMap::new();
            for port in self.host_graph.ports.iter().filter(|port| {
                port.physical
                    && port.data_type == PortDataType::Audio
                    && port.direction == PortDirection::Input
            }) {
                if let Some(locator) = &port.locator {
                    groups
                        .entry(port.name.split(':').next().unwrap_or("").to_owned())
                        .or_default()
                        .push(locator.clone());
                }
            }
            if let Some(mut sinks) = groups
                .into_values()
                .find(|ports| ports.len() >= outputs.len())
            {
                self.master_graph_auto_done = true;
                sinks.sort_by_key(|port| host_port_order(port));
                for (port, sink) in outputs.into_iter().zip(sinks) {
                    self.connect_port(port, &sink, true)
                        .map_err(|error| anyhow!(error.message))?;
                }
            }
            return Ok(());
        }
        let mut sinks: Vec<_> = self
            .observed
            .connections
            .host_ports
            .values()
            .filter(|port| {
                port.data_type == BackendPortDataType::Audio
                    && port.direction == BackendPortDirection::Input
            })
            .cloned()
            .collect();
        sinks.sort_by_key(|port| (!port.preferred_playback, host_port_order(&port.id)));
        let outputs: Vec<_> = self
            .ports
            .iter()
            .filter_map(|(id, entry)| {
                (matches!(entry.view.owner, ApplicationPortOwner::Bus { bus_id } if self.buses.get(&bus_id).is_some_and(|bus| bus.backend == MASTER_BUS_ID))
                    && entry.view.data_type == PortDataType::Audio
                    && entry.view.direction == PortDirection::Output)
                    .then_some(*id)
            })
            .collect();
        for (index, port) in outputs.into_iter().enumerate() {
            if let Some(sink) = sinks.get(index).or_else(|| sinks.last()) {
                self.connect_port(port, &sink.id, true)
                    .map_err(|error| anyhow!(error.message))?;
            }
        }
        Ok(())
    }
    fn connect_track_inputs(&mut self, track: TrackId) -> core::result::Result<(), CommandError> {
        let inputs: Vec<_> = self.tracks[&track]
            .view
            .port_ids
            .iter()
            .filter_map(|id| {
                let port = &self.ports[id].view;
                (port.direction == PortDirection::Input).then_some((*id, port.data_type))
            })
            .collect();
        let mut audio_index = 0;
        for (port, kind) in inputs {
            if self.dynamic_ports() {
                let mut candidates: Vec<_> = self
                    .host_graph
                    .ports
                    .iter()
                    .filter(|candidate| {
                        candidate.physical
                            && candidate.data_type == kind
                            && candidate.direction == PortDirection::Output
                    })
                    .filter_map(|port| port.locator.clone())
                    .collect();
                candidates.sort_by_key(|id| host_port_order(id));
                let index = if kind == PortDataType::Audio {
                    let index = audio_index;
                    audio_index += 1;
                    index
                } else {
                    0
                };
                if let Some(host) = candidates.get(index) {
                    self.connect_port(port, host, true)?;
                }
                continue;
            }
            let mut candidates: Vec<_> = self
                .observed
                .connections
                .host_ports
                .values()
                .filter(|candidate| {
                    data_type(candidate.data_type) == kind
                        && candidate.direction == BackendPortDirection::Output
                })
                .map(|port| port.id.clone())
                .collect();
            candidates.sort_by_key(|id| host_port_order(id));
            let index = if kind == PortDataType::Audio {
                let index = audio_index;
                audio_index += 1;
                index
            } else {
                0
            };
            if let Some(host) = candidates.get(index) {
                self.connect_port(port, host, true)?;
            }
        }
        Ok(())
    }
    fn route_track_to_bus(
        &mut self,
        track: TrackId,
        name: &str,
    ) -> core::result::Result<(), CommandError> {
        let outputs: Vec<_> = self.tracks[&track]
            .view
            .port_ids
            .iter()
            .filter(|id| {
                let port = &self.ports[id].view;
                port.data_type == PortDataType::Audio && port.direction == PortDirection::Output
            })
            .copied()
            .collect();
        let Some(bus) = self
            .bus_order
            .iter()
            .filter_map(|id| self.buses.get(id))
            .find(|b| {
                b.view.structural_state != StructuralState::Removing
                    && b.view.name == name
                    && (outputs.len() == b.view.channels.len() || outputs.len() == 1)
            })
        else {
            return Ok(());
        };
        let channels = bus.view.channels.iter().map(|c| c.id).collect::<Vec<_>>();
        if outputs.len() == 1 {
            for channel in channels {
                self.route(outputs[0], channel, true)?;
            }
        } else {
            for (port, channel) in outputs.into_iter().zip(channels) {
                self.route(port, channel, true)?;
            }
        }
        Ok(())
    }
}

use super::projection::data_type;

pub(super) fn host_port_order(id: &str) -> (String, u64) {
    let suffix = id.rfind(|c: char| !c.is_ascii_digit()).map_or(0, |i| i + 1);
    (id[..suffix].to_owned(), id[suffix..].parse().unwrap_or(0))
}
