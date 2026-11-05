//! Engine messages: the handshake and resynchronization, outcomes, and telemetry.

use alloc::borrow::ToOwned;
use alloc::collections::BTreeSet;
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use shoop_backend::{
    BackendBusControl, BackendConfirmedLink, BackendConnectionFailure, BackendMixerFailure,
    BackendMixerLink, BackendMutationDetail, BackendMutationFailure, BackendMutationKind,
    BackendTrackControl, GLOBAL_FX_PORT_ID, MASTER_BUS_CHANNEL_IDS,
};
use shoop_rt_protocol::{
    BusControl, BusPort, DisconnectReason, EngineMessage, EnginePort, Limits, LoopMode, Outcome,
    Refusal, StateRecord, StripControl, Telemetry,
};
use shoop_rt_transport::carrier::Carrier;

use crate::mirror::{amplitude_db, reconciled_db, LoopView};
use crate::requests::{Pending, Purpose};
use crate::{EngineClock, Link, RtBackend};

fn refusal_text(refusal: Refusal) -> &'static str {
    match refusal {
        Refusal::Unsupported => "the engine does not support it",
        Refusal::Capacity => "the engine's prepared capacity is exhausted",
        Refusal::InvalidId => "the engine does not know its target",
        Refusal::InvalidValue => "the engine does not accept the value",
        Refusal::Busy => "a conflicting change was in progress",
        Refusal::Overloaded => "too many requests were pending",
        Refusal::NotConnected => "the engine is not connected",
        Refusal::Internal => "the engine failed internally",
    }
}

fn app_control(control: StripControl) -> BackendTrackControl {
    match control {
        StripControl::InputGain(value) => BackendTrackControl::InputGainDb(amplitude_db(value)),
        StripControl::InputBalance(value) => BackendTrackControl::InputBalance(value),
        StripControl::Monitoring(value) => BackendTrackControl::InputMonitoring(value),
        StripControl::OutputGain(value) => BackendTrackControl::OutputGainDb(amplitude_db(value)),
        StripControl::OutputBalance(value) => BackendTrackControl::OutputBalance(value),
        StripControl::OutputMute(value) => BackendTrackControl::OutputMute(value),
    }
}

impl<C: Carrier, K: EngineClock> RtBackend<C, K> {
    pub(crate) fn handle(&mut self, message: EngineMessage) {
        match message {
            EngineMessage::ProfilingGraph {
                generation,
                epoch,
                record,
            } => {
                self.profiling.graph(generation, epoch, record);
                if record == shoop_rt_protocol::profiling::GraphRecord::End {
                    self.update_profiling_groups();
                }
            }
            EngineMessage::ProfilingStatus(status) => self.profiling.status(status),
            EngineMessage::Profiling(packet) => self.profiling.receive(packet),
            EngineMessage::ProfilingNode {
                generation,
                key,
                name,
            } => self.profiling.name(generation, key, name.as_str()),
            EngineMessage::Welcome {
                capabilities,
                limits,
                driver,
                ..
            } => {
                if self.link != Link::Connecting {
                    return;
                }
                tracing::debug!(?limits, ?driver, "backend.rt.welcome");
                if usize::from(limits.master_channels) != MASTER_BUS_CHANNEL_IDS.len() {
                    self.lose(format!(
                        "the engine's Master has {} channels; the application needs {}",
                        limits.master_channels,
                        MASTER_BUS_CHANNEL_IDS.len()
                    ));
                    return;
                }
                let Some(connection) = crate::profiling_connection() else {
                    self.lose("profiling connection identities exhausted".into());
                    return;
                };
                self.profiling.set_connection(connection);
                self.capabilities = capabilities;
                self.set_limits(limits);
                self.driver = Some(driver);
                match self.client.resync() {
                    Ok(()) => {
                        self.link = Link::Resyncing;
                        self.resync = Some(Vec::new());
                    }
                    Err(error) => self.lose(format!("resynchronization failed: {error:?}")),
                }
            }
            EngineMessage::Rejected { version } => {
                self.lose(format!("the engine speaks protocol version {version}"))
            }
            EngineMessage::Result {
                correlation,
                outcome,
            } => {
                self.telemetry_fresh = false;
                if let Some(pending) = self.requests.in_flight.remove(&correlation) {
                    self.answered(pending, outcome);
                }
            }
            EngineMessage::Telemetry(record) => self.telemetry(record),
            EngineMessage::State(record) => self.state_record(record),
            EngineMessage::Bulk(chunk) => {
                let mirror = &self.mirror;
                self.reads.receive(chunk, |loop_id| {
                    mirror.loops.get(&loop_id).map_or(0, |entry| {
                        u32::try_from(entry.timing.raw_midi_frames).unwrap_or(u32::MAX)
                    })
                });
            }
            EngineMessage::LimitsChanged(limits) => {
                tracing::debug!(?limits, "backend.rt.limits_changed");
                self.set_limits(limits);
            }
            // The engine's answer to an earlier client's goodbye.
            EngineMessage::Disconnected(DisconnectReason::Shutdown)
                if self.link == Link::Connecting => {}
            EngineMessage::Disconnected(reason) => {
                self.lose(format!("the engine ended the connection ({reason:?})"))
            }
        }
    }

    fn set_limits(&mut self, limits: Limits) {
        self.mirror.bus_slots.resize(limits.buses as usize);
        self.mirror.strip_slots.resize(limits.strips as usize);
        self.mirror.loop_slots.resize(limits.loops as usize);
        self.reads.set_loop_channels(limits.strip_audio_channels);
        self.limits = Some(limits);
    }

    fn telemetry(&mut self, record: Telemetry) {
        match record {
            Telemetry::Preview { request_id } => self.preview_playing = request_id,
            Telemetry::Engine {
                frame,
                callbacks,
                xruns,
                load,
                input_peak,
                output_peak,
                driver,
            } => {
                let status = &mut self.status;
                status.frame = frame;
                status.callbacks = callbacks;
                status.load = load;
                status.xruns_reported.get_or_insert(xruns);
                status.xruns = xruns;
                if !status.peaks_fresh {
                    status.input_peak = 0.0;
                    status.output_peak = 0.0;
                    status.peaks_fresh = true;
                }
                status.input_peak = status.input_peak.max(input_peak.abs());
                status.output_peak = status.output_peak.max(output_peak.abs());
                self.driver = Some(driver);
                self.telemetry_fresh = true;
            }
            Telemetry::Strip {
                strip,
                input_peaks,
                output_peaks,
                midi_in,
                midi_out,
                latest_input_midi,
            } => {
                let Some(id) = self.mirror.strip_tracks.get(&strip) else {
                    return;
                };
                if let Some(track) = self.mirror.tracks.get_mut(id) {
                    track.input_meter.record(input_peaks.as_slice());
                    track.output_meter.record(output_peaks.as_slice());
                    track.midi_in |= midi_in;
                    track.midi_out |= midi_out;
                    track.latest_input_midi = latest_input_midi.and_then(|message| {
                        let packet = message.as_bytes();
                        if packet.len() > 4 {
                            return None;
                        }
                        let mut bytes = [0; 4];
                        bytes[..packet.len()].copy_from_slice(packet);
                        Some(shoop_backend::BackendLatestMidiMessage {
                            bytes,
                            len: packet.len() as u8,
                        })
                    });
                }
            }
            Telemetry::Loop {
                loop_id,
                mode,
                next_mode,
                next_delay,
                content_revision,
                recording_armed,
                timing_revision,
                length,
                position,
                peaks,
            } => {
                let Some(id) = self.mirror.slot_loops.get(&loop_id) else {
                    return;
                };
                if let Some(entry) = self.mirror.loops.get_mut(id) {
                    let previous = entry.view;
                    if timing_revision != entry.timing.revision {
                        return;
                    }
                    if content_revision < entry.rt_content_revision {
                        return;
                    }
                    if (mode == LoopMode::Recording && previous.mode != LoopMode::Recording)
                        || length != previous.length
                        || content_revision != entry.rt_content_revision
                    {
                        entry.content += 1;
                        self.canonical_content.remove(id);
                        self.prepared_content.remove(id);
                        self.canonical_imports.remove(id);
                    }
                    if content_revision != entry.timing.content_revision {
                        let track = &self.mirror.tracks[&entry.track];
                        entry.timing.content_revision = content_revision;
                        for window in entry
                            .timing
                            .windows
                            .audio
                            .iter_mut()
                            .chain(core::iter::once(&mut entry.timing.windows.midi))
                        {
                            window.raw_frames = None;
                        }
                        entry.timing.length = length;
                        entry.timing.raw_audio_frames =
                            if track.audio_channels > 0 { length } else { 0 };
                        entry.timing.raw_midi_frames = if track.midi { length } else { 0 };
                    }
                    entry.view = LoopView {
                        recording_armed,
                        mode,
                        next_mode,
                        next_delay,
                        length,
                        position,
                    };
                    entry.rt_content_revision = content_revision;
                    entry.meter.record(peaks.as_slice());
                }
            }
            Telemetry::Bus { bus, peaks } => {
                if let Some(entry) = self.mirror.buses.values_mut().find(|b| b.slot == bus) {
                    entry.meter.record(peaks.as_slice());
                }
            }
        }
    }

    fn recorded_snapshot(
        &mut self,
        loop_id: shoop_rt_protocol::LoopSlotId,
        revision: u64,
        state: Option<shoop_rt_protocol::RecordedProcessorState>,
    ) {
        let Some(id) = self.mirror.slot_loops.get(&loop_id).copied() else {
            return;
        };
        if let Some(recorded) = state {
            let track = &self.mirror.tracks[&self.mirror.loops[&id].track];
            let valid = crate::processor_controls::track_kind(track) == Some(recorded.kind)
                && shoop_processor_protocol::state::State::new(recorded.kind).is_ok_and(
                    |mut state| {
                        state.values = recorded.values;
                        state.validate().is_ok()
                    },
                );
            if !valid {
                self.lose("engine recorded processor state is invalid".to_owned());
                return;
            }
        }
        self.mirror.loops.get_mut(&id).unwrap().recorded = state.map(|state| (revision, state));
    }

    fn state_record(&mut self, record: StateRecord) {
        if self.resync.is_none() {
            if let StateRecord::RecordedProcessorState {
                loop_id,
                content_revision,
                state,
            } = record
            {
                self.recorded_snapshot(loop_id, content_revision, state);
            }
            if let StateRecord::ProcessorState {
                strip,
                slot,
                snapshot,
            } = record
            {
                self.processor_snapshot(strip, slot, snapshot);
            }
            if let StateRecord::StripControl { strip, control } = record {
                if let Some(id) = self.mirror.strip_tracks.get(&strip).copied() {
                    if let Some(track) = self.mirror.tracks.get_mut(&id) {
                        track.controls.apply(app_control(control));
                    }
                }
            }
            return;
        }
        let Some(records) = &mut self.resync else {
            return;
        };
        match record {
            StateRecord::Begin { .. } => records.clear(),
            StateRecord::End { epoch } => {
                let records = self.resync.take().unwrap_or_default();
                tracing::debug!(epoch, records = records.len(), "backend.rt.resync");
                self.apply_resync(records);
                if self.link == Link::Resyncing {
                    self.link = Link::Ready;
                }
            }
            record => records.push(record),
        }
    }

    fn processor_snapshot(
        &mut self,
        strip: shoop_rt_protocol::StripId,
        slot: shoop_rt_protocol::ProcessorSlot,
        snapshot: shoop_rt_protocol::processors::Snapshot,
    ) {
        let Some(id) = self.mirror.strip_tracks.get(&strip) else {
            return;
        };
        let Some(track) = self.mirror.tracks.get_mut(id) else {
            return;
        };
        if !matches!(
            track.topology,
            shoop_backend::BackendTrackTopology::DryWetProcessor { .. }
        ) || track.removing
            || track
                .processor_slot
                .is_some_and(|expected| expected != slot)
        {
            return;
        }
        track.processor_slot = Some(slot);
        track.processor_snapshot = Some(snapshot);
    }

    /// Reconciles known mappings without deleting engine-owned session objects.
    fn apply_resync(&mut self, records: Vec<StateRecord>) {
        self.processors = None;
        self.processor_presets.clear();
        let mut host_ports = Vec::new();
        let mut strips = BTreeSet::new();
        let mut processors = BTreeSet::new();
        let mut links = BTreeSet::new();
        let mut routes = BTreeSet::new();
        for record in &records {
            match *record {
                StateRecord::Processors(limits) => self.processors = Some(limits),
                StateRecord::ProcessorPreset(preset) => self.processor_presets.push(preset),

                StateRecord::BusGenerations { generation } => {
                    self.mirror.bus_slots.raise_floor(generation)
                }
                StateRecord::BusShape { bus, .. } => self.mirror.bus_slots.adopt(bus.0),
                StateRecord::HostPort(port) => host_ports.push(port),
                StateRecord::StripProcessor { strip, .. } => {
                    processors.insert(strip);
                }
                StateRecord::Strip { strip, shape } => {
                    if let Some(track) = self
                        .mirror
                        .strip_tracks
                        .get(&strip)
                        .and_then(|id| self.mirror.tracks.get(id))
                    {
                        let direct = matches!(
                            track.topology,
                            shoop_backend::BackendTrackTopology::Direct { .. }
                        );
                        let expected = shoop_rt_protocol::StripShape {
                            audio_inputs: track.topology.dry_audio_channels() as u8,
                            audio_outputs: track.topology.wet_audio_channels() as u8,
                            midi_input: track.midi,
                            midi_output: direct && track.midi,
                        };
                        if shape != expected {
                            self.lose("engine track shape differs from its owner".to_owned());
                            return;
                        }
                    }
                    self.mirror.strip_slots.adopt(strip.0);
                    strips.insert(strip);
                }
                StateRecord::Loop { loop_id, .. } => self.mirror.loop_slots.adopt(loop_id.0),
                StateRecord::Generations { strips, loops } => {
                    self.mirror.strip_slots.raise_floor(strips);
                    self.mirror.loop_slots.raise_floor(loops);
                }
                _ => {}
            }
        }
        let buses = records
            .iter()
            .filter_map(|r| {
                if let StateRecord::BusShape { bus, .. } = r {
                    Some(*bus)
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();
        if buses
            .iter()
            .any(|id| !self.mirror.buses.values().any(|b| b.slot == *id))
        {
            self.lose("engine buses belong to another service".to_owned());
            return;
        }
        let vanished = self
            .mirror
            .buses
            .iter()
            .filter(|(_, b)| b.confirmed && !buses.contains(&b.slot))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in vanished {
            self.mirror.drop_bus(id);
        }
        self.mirror.host_ports = host_ports;
        if strips
            .iter()
            .any(|strip| !self.mirror.strip_tracks.contains_key(strip))
        {
            self.lose("the engine already has a live session owned by another service".to_owned());
            return;
        }
        if self
            .mirror
            .tracks
            .values()
            .filter(|track| strips.contains(&track.strip))
            .any(|track| {
                matches!(
                    track.topology,
                    shoop_backend::BackendTrackTopology::Direct { .. }
                ) == processors.contains(&track.strip)
            })
        {
            self.lose("engine track processor ownership record is inconsistent".to_owned());
            return;
        }
        let vanished = self
            .mirror
            .tracks
            .iter()
            .filter(|(_, track)| track.confirmed && !strips.contains(&track.strip))
            .map(|(id, _)| *id)
            .collect::<Vec<_>>();
        for id in vanished {
            self.mirror.drop_track(id);
        }
        for record in records {
            match record {
                StateRecord::RecordedProcessorState {
                    loop_id,
                    content_revision,
                    state,
                } => self.recorded_snapshot(loop_id, content_revision, state),
                StateRecord::ProcessorState {
                    strip,
                    slot,
                    snapshot,
                } => self.processor_snapshot(strip, slot, snapshot),
                StateRecord::StripDefaultPlayback {
                    strip,
                    dry_through_wet,
                } => {
                    if let Some(id) = self.mirror.strip_tracks.get(&strip) {
                        if let Some(track) = self.mirror.tracks.get_mut(id) {
                            track.default_playback_mode = if dry_through_wet {
                                shoop_backend::BackendDefaultPlaybackMode::DryThroughWet
                            } else {
                                shoop_backend::BackendDefaultPlaybackMode::Regular
                            };
                        }
                    }
                }
                StateRecord::StripProcessor { strip, kind, slot } => {
                    let Some(id) = self.mirror.strip_tracks.get(&strip) else {
                        continue;
                    };
                    let Some(track) = self.mirror.tracks.get_mut(id) else {
                        continue;
                    };
                    let matches = match &track.topology {
                        shoop_backend::BackendTrackTopology::Direct { .. } => false,
                        shoop_backend::BackendTrackTopology::DryWetExternal { .. } => {
                            kind == shoop_rt_protocol::ProcessorKind::External
                        }
                        shoop_backend::BackendTrackTopology::DryWetProcessor {
                            processor_type,
                            ..
                        } => match kind {
                            shoop_rt_protocol::ProcessorKind::BuiltInFx => {
                                processor_type == shoop_app_api::TrackProcessorTypeId::BUILTIN_FX
                            }
                            shoop_rt_protocol::ProcessorKind::OxiSynth => {
                                processor_type == shoop_app_api::TrackProcessorTypeId::OXISYNTH
                            }
                            shoop_rt_protocol::ProcessorKind::External => false,
                        },
                    };
                    if !matches {
                        self.lose("engine track processor differs from its owner".to_owned());
                        return;
                    }
                    track.processor_slot = slot;
                }
                StateRecord::StripControl { strip, control } => {
                    let Some(id) = self.mirror.strip_tracks.get(&strip) else {
                        continue;
                    };
                    if let Some(track) = self.mirror.tracks.get_mut(id) {
                        let control = match control {
                            StripControl::InputGain(value) => BackendTrackControl::InputGainDb(
                                reconciled_db(track.controls.input_gain_db, value),
                            ),
                            StripControl::OutputGain(value) => BackendTrackControl::OutputGainDb(
                                reconciled_db(track.controls.output_gain_db, value),
                            ),
                            _ => app_control(control),
                        };
                        track.controls.apply(control);
                    }
                }
                StateRecord::Loop {
                    loop_id,
                    sync,
                    gain,
                    balance,
                    ..
                } => {
                    let sync = sync.and_then(|slot| self.mirror.slot_loops.get(&slot).copied());
                    let Some(id) = self.mirror.slot_loops.get(&loop_id) else {
                        continue;
                    };
                    if let Some(entry) = self.mirror.loops.get_mut(id) {
                        entry.sync = sync;
                        entry.gain = gain;
                        entry.balance = balance;
                    }
                }
                StateRecord::LoopTiming { loop_id, timing } => {
                    if let Some(id) = self.mirror.slot_loops.get(&loop_id).copied() {
                        self.apply_timing(id, timing);
                    }
                }
                StateRecord::Connection { port, host } => {
                    let (Some(port), Some(host)) =
                        (self.mirror.port_of(port), self.mirror.host_by_id(host))
                    else {
                        continue;
                    };
                    links.insert(BackendConfirmedLink {
                        application_port_id: port,
                        host_port_id: self.mirror.host_key(host).to_owned(),
                    });
                }
                StateRecord::Route {
                    port,
                    bus: BusPort { bus, index },
                } => {
                    let (Some(source_port_id), Some(destination_channel_id)) = (
                        self.mirror.port_of(EnginePort::Strip(port)),
                        self.mirror.channel(BusPort { bus, index }),
                    ) else {
                        continue;
                    };
                    routes.insert(BackendMixerLink {
                        source_port_id,
                        destination_channel_id,
                    });
                }
                StateRecord::Bus { bus, control } => {
                    if let Some(entry) = self.mirror.buses.values_mut().find(|b| b.slot == bus) {
                        entry.apply(match control {
                            BusControl::Gain(v) => {
                                BackendBusControl::GainDb(reconciled_db(entry.gain_db, v))
                            }
                            BusControl::Balance(v) => BackendBusControl::Balance(v),
                            BusControl::Mute(v) => BackendBusControl::Mute(v),
                        });
                    }
                }
                _ => {}
            }
        }
        links.extend(
            self.mirror
                .links
                .iter()
                .filter(|link| link.application_port_id == GLOBAL_FX_PORT_ID)
                .cloned(),
        );
        self.mirror.links = links;
        self.mirror.routes = routes;
        self.mirror.connection_revision += 1;
        self.mirror.mixer_revision += 1;
    }

    fn failure(
        &mut self,
        kind: BackendMutationKind,
        entity: Option<u64>,
        detail: Option<BackendMutationDetail>,
        message: String,
    ) {
        tracing::warn!(?kind, entity, message = %message, "backend.rt.request_refused");
        self.failure_sequence += 1;
        self.mutation_failures.push(BackendMutationFailure {
            driver_generation: 1,
            sequence: self.failure_sequence,
            operation_key: None,
            kind,
            entity,
            detail,
            message,
        });
    }

    fn answered(&mut self, pending: Pending, outcome: Outcome) {
        if pending.purpose == Purpose::CancelReads {
            return;
        }
        if pending.purpose == Purpose::Preview {
            if let Some(preview) = &mut self.preview {
                preview
                    .job
                    .answered(pending.request, outcome, self.status.callbacks);
            }
            return;
        }
        if pending.purpose == Purpose::Persistence {
            if let Some(replacement) = &mut self.replacement {
                replacement
                    .job
                    .answered(pending.request, outcome, self.status.callbacks);
            }
            return;
        }
        let Pending {
            sequence, purpose, ..
        } = pending;
        self.mutation_completions
            .push(shoop_backend::BackendMutationCompletion {
                sequence,
                result: match outcome {
                    Outcome::Done | Outcome::TimingApplied(_) => Ok(()),
                    Outcome::ContentApplied { .. } | Outcome::ContentPrepared { .. } => {
                        Err("unexpected content revision outcome".to_owned())
                    }
                    Outcome::Refused(refusal) => Err(refusal_text(refusal).to_owned()),
                },
            });
        let refusal = match outcome {
            Outcome::Done | Outcome::TimingApplied(_) => None,
            Outcome::ContentApplied { .. } | Outcome::ContentPrepared { .. } => {
                Some(Refusal::Internal)
            }
            Outcome::Refused(refusal) => Some(refusal),
        };
        match (purpose, refusal) {
            (Purpose::Timing(id), None) => {
                if let Outcome::TimingApplied(state) = outcome {
                    self.apply_timing(id, state);
                }
            }
            (Purpose::Timing(id), Some(refusal)) => self.failure(
                BackendMutationKind::LoopControl,
                Some(id.raw()),
                None,
                format!("loop timing was refused: {}", refusal_text(refusal)),
            ),
            (Purpose::CreateBus(id), None) => {
                if let Some(bus) = self.mirror.buses.get_mut(&id) {
                    bus.confirmed = true;
                    self.mirror.mixer_revision += 1;
                    self.mirror.connection_revision += 1;
                }
            }
            (Purpose::CreateBus(id), Some(refusal)) => {
                if let Some(bus) = self.mirror.buses.get(&id) {
                    self.mirror.bus_slots.release(bus.slot.0);
                }
                self.mirror.drop_bus(id);
                self.failure(
                    BackendMutationKind::BusStructure,
                    Some(id.raw()),
                    Some(BackendMutationDetail::BusCreation),
                    format!("bus creation refused: {}", refusal_text(refusal)),
                );
            }
            (Purpose::RemoveBus(id), None) => {
                if let Some(bus) = self.mirror.buses.get(&id) {
                    self.mirror.bus_slots.release(bus.slot.0);
                }
                self.mirror.drop_bus(id);
            }
            (Purpose::RemoveBus(id), Some(refusal)) => {
                if let Some(bus) = self.mirror.buses.get_mut(&id) {
                    bus.removing = false;
                }
                self.failure(
                    BackendMutationKind::BusStructure,
                    Some(id.raw()),
                    Some(BackendMutationDetail::BusRemoval),
                    format!("bus removal refused: {}", refusal_text(refusal)),
                );
            }
            (Purpose::CreateStrip(id), None) => {
                if let Some(track) = self.mirror.tracks.get_mut(&id) {
                    track.confirmed = true;
                    self.mirror.connection_revision += 1;
                }
            }
            (Purpose::CreateStrip(id), Some(refusal)) => {
                if let Some(track) = self.mirror.tracks.get(&id) {
                    self.mirror.strip_slots.release(track.strip.0);
                    let loops = track.loops.clone();
                    for loop_id in loops {
                        if let Some(entry) = self.mirror.loops.get(&loop_id) {
                            self.mirror.loop_slots.release(entry.slot.0);
                        }
                        self.reads.forget(loop_id);
                    }
                    self.mirror.drop_track(id);
                    self.failure(
                        BackendMutationKind::TrackStructure,
                        Some(id.raw()),
                        Some(BackendMutationDetail::TrackCreation),
                        format!("creating a track was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::RemoveStrip(id), None) => {
                if let Some(track) = self.mirror.tracks.get(&id) {
                    for loop_id in &track.loops {
                        self.canonical_content.remove(loop_id);
                        self.prepared_content.remove(loop_id);
                        self.canonical_imports.remove(loop_id);
                    }
                }
                self.mirror.drop_track(id);
            }
            (Purpose::RemoveStrip(id), Some(refusal)) => {
                if let Some(track) = self.mirror.tracks.get_mut(&id) {
                    track.removing = false;
                    self.mirror.strip_slots.restore(track.strip.0);
                    let loops = track.loops.clone();
                    for loop_id in loops {
                        if let Some(entry) = self.mirror.loops.get(&loop_id) {
                            self.mirror.loop_slots.restore(entry.slot.0);
                        }
                    }
                    self.failure(
                        BackendMutationKind::TrackStructure,
                        Some(id.raw()),
                        Some(BackendMutationDetail::TrackRemoval),
                        format!("removing a track was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::CreateLoop(id), None) => {
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.confirmed = true;
                }
            }
            (Purpose::CreateLoop(id), Some(refusal)) => {
                if let Some(entry) = self.mirror.loops.get(&id) {
                    let track = entry.track;
                    self.mirror.loop_slots.release(entry.slot.0);
                    self.mirror.drop_loop(id);
                    self.reads.forget(id);
                    if self.mirror.tracks.contains_key(&track) {
                        self.failure(
                            BackendMutationKind::TrackStructure,
                            Some(track.raw()),
                            Some(BackendMutationDetail::LoopCreation { loop_id: id }),
                            format!("adding a loop was refused: {}", refusal_text(refusal)),
                        );
                    }
                }
            }
            (Purpose::ProcessorControl(_, _), None) => {}
            (Purpose::ProcessorControl(id, control), Some(refusal)) => {
                if self.mirror.tracks.contains_key(&id) {
                    self.failure(
                        BackendMutationKind::TrackFxControl,
                        Some(id.raw()),
                        Some(BackendMutationDetail::TrackFxControl(control)),
                        format!("processor control was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::DefaultPlayback(id, mode), None) => {
                if let Some(track) = self.mirror.tracks.get_mut(&id) {
                    track.default_playback_mode = mode;
                }
            }
            (Purpose::DefaultPlayback(id, mode), Some(refusal)) => {
                if self.mirror.tracks.contains_key(&id) {
                    self.failure(
                        BackendMutationKind::TrackControl,
                        Some(id.raw()),
                        Some(BackendMutationDetail::TrackDefaultPlaybackMode(mode)),
                        format!(
                            "default playback mode was refused: {}",
                            refusal_text(refusal)
                        ),
                    );
                }
            }
            (Purpose::StripControl(id, control), None) => {
                if let Some(track) = self.mirror.tracks.get_mut(&id) {
                    track.controls.apply(control);
                }
            }
            (Purpose::StripControl(id, control), Some(refusal)) => {
                if self.mirror.tracks.contains_key(&id) {
                    self.failure(
                        BackendMutationKind::TrackControl,
                        Some(id.raw()),
                        Some(BackendMutationDetail::TrackControl(control)),
                        format!("a track control was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::LoopSync(id, source), None) => {
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.sync = source;
                }
            }
            (Purpose::LoopGain(id, gain), None) => {
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.gain = gain;
                }
            }
            (Purpose::LoopBalance(id, balance), None) => {
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.balance = balance;
                }
            }
            (Purpose::Clear(id), None) => {
                self.canonical_content.remove(&id);
                self.prepared_content.remove(&id);
                self.canonical_imports.remove(&id);
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.content += 1;
                    entry.timing.windows = Default::default();
                    entry.timing.length = 0;
                    entry.timing.raw_audio_frames = 0;
                    entry.timing.raw_midi_frames = 0;
                }
            }
            (
                purpose @ (Purpose::LoopSync(id, _)
                | Purpose::Transition(id)
                | Purpose::Clear(id)
                | Purpose::LoopGain(id, _)
                | Purpose::LoopBalance(id, _)),
                Some(refusal),
            ) => {
                if self.mirror.loops.contains_key(&id) {
                    let (detail, what) = match purpose {
                        Purpose::LoopGain(_, gain) => {
                            (Some(BackendMutationDetail::LoopGain(gain)), "a loop gain")
                        }
                        Purpose::LoopBalance(_, balance) => (
                            Some(BackendMutationDetail::LoopBalance(balance)),
                            "a loop balance",
                        ),
                        Purpose::LoopSync(..) => (None, "a loop sync source"),
                        Purpose::Clear(_) => (None, "clearing a loop"),
                        _ => (None, "a loop transition"),
                    };
                    self.failure(
                        BackendMutationKind::LoopControl,
                        Some(id.raw()),
                        detail,
                        format!("{what} was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::Transition(_), None) => {}
            (
                Purpose::Connect {
                    port,
                    host,
                    connected,
                },
                refusal,
            ) => {
                if !self.mirror.ports.contains_key(&port) {
                    return;
                }
                match refusal {
                    None => {
                        let link = BackendConfirmedLink {
                            application_port_id: port,
                            host_port_id: host,
                        };
                        if connected {
                            self.mirror.desired_links.insert(link.clone());
                            self.mirror.links.insert(link);
                        } else {
                            self.mirror.desired_links.remove(&link);
                            self.mirror.links.remove(&link);
                        }
                    }
                    Some(refusal) => {
                        let message = format!(
                            "{} {host} was refused: {}",
                            if connected {
                                "connecting to"
                            } else {
                                "disconnecting from"
                            },
                            refusal_text(refusal)
                        );
                        tracing::warn!(message = %message, "backend.rt.connection_refused");
                        self.connection_failures.push(BackendConnectionFailure {
                            port_id: port,
                            external_port: host,
                            desired_connected: connected,
                            message,
                        });
                    }
                }
                self.mirror.connection_revision += 1;
            }
            (Purpose::Route { link, connected }, refusal) => {
                if !self.mirror.ports.contains_key(&link.source_port_id) {
                    return;
                }
                match refusal {
                    None => {
                        if connected {
                            self.mirror.routes.insert(link);
                        } else {
                            self.mirror.routes.remove(&link);
                        }
                    }
                    Some(refusal) => {
                        let message = format!("a bus route was refused: {}", refusal_text(refusal));
                        tracing::warn!(message = %message, "backend.rt.route_refused");
                        self.mixer_failures.push(BackendMixerFailure {
                            link,
                            desired_connected: connected,
                            message,
                        });
                    }
                }
                self.mirror.mixer_revision += 1;
            }
            (Purpose::BusControl(id, control), None) => {
                if let Some(bus) = self.mirror.buses.get_mut(&id) {
                    bus.apply(control);
                }
                self.mirror.mixer_revision += 1;
            }
            (Purpose::BusControl(id, control), Some(refusal)) => self.failure(
                BackendMutationKind::BusControl,
                Some(id.raw()),
                Some(BackendMutationDetail::BusControl(control)),
                format!("bus control refused: {}", refusal_text(refusal)),
            ),
            (Purpose::InjectMidi(id), Some(refusal)) => {
                if self.mirror.tracks.contains_key(&id) {
                    self.failure(
                        BackendMutationKind::MidiInput,
                        Some(id.raw()),
                        None,
                        format!("MIDI input was refused: {}", refusal_text(refusal)),
                    );
                }
            }
            (Purpose::Profiling, Some(refusal)) => self.failure(
                shoop_backend::BackendMutationKind::AudioProcessing,
                None,
                None,
                format!("could not change profiling: {}", refusal_text(refusal)),
            ),
            (Purpose::Smoothing(milliseconds), Some(refusal)) => self.failure(
                BackendMutationKind::LoopControl,
                None,
                None,
                format!(
                    "loop boundary smoothing of {milliseconds} ms was refused: {}",
                    refusal_text(refusal)
                ),
            ),
            (
                Purpose::InjectMidi(_)
                | Purpose::Profiling
                | Purpose::Smoothing(_)
                | Purpose::Preview
                | Purpose::Persistence
                | Purpose::CancelReads,
                _,
            ) => {}
        }
    }
}
