//! Engine messages: the handshake and resynchronization, outcomes, and telemetry.

use std::collections::BTreeSet;

use shoop_backend::{
    BackendBusControl, BackendConfirmedLink, BackendConnectionFailure, BackendMixerFailure,
    BackendMixerLink, BackendMutationDetail, BackendMutationFailure, BackendMutationKind,
    BackendTrackControl, GLOBAL_FX_PORT_ID, MASTER_BUS_CHANNEL_IDS, MASTER_BUS_ID,
};
use shoop_rt_protocol::carrier::Carrier;
use shoop_rt_protocol::{
    BusControl, BusId, BusPort, DisconnectReason, EngineMessage, EnginePort, Limits, LoopMode,
    Outcome, Refusal, Request, StateRecord, StripControl, Telemetry,
};

use crate::mirror::{amplitude_db, LoopView};
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
                        u32::try_from(entry.view.length).unwrap_or(u32::MAX)
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
        self.mirror.strip_slots.resize(limits.strips as usize);
        self.mirror.loop_slots.resize(limits.loops as usize);
        self.reads.set_loop_channels(limits.strip_audio_channels);
        self.limits = Some(limits);
    }

    fn telemetry(&mut self, record: Telemetry) {
        match record {
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
            } => {
                let Some(id) = self.mirror.strip_tracks.get(&strip) else {
                    return;
                };
                if let Some(track) = self.mirror.tracks.get_mut(id) {
                    track.input_meter.record(input_peaks.as_slice());
                    track.output_meter.record(output_peaks.as_slice());
                    track.midi_in |= midi_in;
                    track.midi_out |= midi_out;
                }
            }
            Telemetry::Loop {
                loop_id,
                mode,
                next_mode,
                next_delay,
                length,
                position,
                peaks,
            } => {
                let Some(id) = self.mirror.slot_loops.get(&loop_id) else {
                    return;
                };
                if let Some(entry) = self.mirror.loops.get_mut(id) {
                    let previous = entry.view;
                    if (mode == LoopMode::Recording && previous.mode != LoopMode::Recording)
                        || length != previous.length
                    {
                        entry.content += 1;
                    }
                    entry.view = LoopView {
                        mode,
                        next_mode,
                        next_delay,
                        length,
                        position,
                    };
                    entry.meter.record(peaks.as_slice());
                }
            }
            Telemetry::Bus {
                bus: BusId::Master,
                peaks,
            } => self.mirror.master.meter.record(peaks.as_slice()),
        }
    }

    fn state_record(&mut self, record: StateRecord) {
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

    /// Adopts the engine's confirmed state. Engine structure the application does not know is
    /// removed; known structure takes the engine's values.
    fn apply_resync(&mut self, records: Vec<StateRecord>) {
        let mut host_ports = Vec::new();
        let mut strips = BTreeSet::new();
        let mut links = BTreeSet::new();
        let mut routes = BTreeSet::new();
        for record in &records {
            match *record {
                StateRecord::HostPort(port) => host_ports.push(port),
                StateRecord::Strip { strip, .. } => {
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
        self.mirror.host_ports = host_ports;
        for strip in &strips {
            if !self.mirror.strip_tracks.contains_key(strip) {
                self.requests
                    .push(Request::RemoveStrip { strip: *strip }, Purpose::Cleanup);
                self.mirror.strip_slots.release(strip.0);
            }
        }
        for record in &records {
            if let StateRecord::Loop { loop_id, strip, .. } = record {
                if !self.mirror.strip_tracks.contains_key(strip) {
                    self.mirror.loop_slots.release(loop_id.0);
                }
            }
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
                StateRecord::StripControl { strip, control } => {
                    let Some(id) = self.mirror.strip_tracks.get(&strip) else {
                        continue;
                    };
                    if let Some(track) = self.mirror.tracks.get_mut(id) {
                        track.controls.apply(app_control(control));
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
                StateRecord::Connection { port, host } => {
                    let (Some(port), Some(host)) =
                        (self.mirror.port_of(port), self.mirror.host_by_id(host))
                    else {
                        continue;
                    };
                    links.insert(BackendConfirmedLink {
                        application_port_id: port,
                        host_port_id: host.name.as_str().to_owned(),
                    });
                }
                StateRecord::Route {
                    port,
                    bus:
                        BusPort {
                            bus: BusId::Master,
                            index,
                        },
                } => {
                    let (Some(source_port_id), Some(destination_channel_id)) = (
                        self.mirror.port_of(EnginePort::Strip(port)),
                        MASTER_BUS_CHANNEL_IDS.get(usize::from(index)).copied(),
                    ) else {
                        continue;
                    };
                    routes.insert(BackendMixerLink {
                        source_port_id,
                        destination_channel_id,
                    });
                }
                StateRecord::Bus {
                    bus: BusId::Master,
                    control,
                } => match control {
                    BusControl::Gain(value) => self.mirror.master.gain_db = amplitude_db(value),
                    BusControl::Balance(value) => self.mirror.master.balance = value,
                    BusControl::Mute(value) => self.mirror.master.muted = value,
                },
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
        let Pending {
            sequence, purpose, ..
        } = pending;
        let refusal = match outcome {
            Outcome::Done => None,
            Outcome::Refused(refusal) => Some(refusal),
        };
        if let (Some(refusal), Some(replacement)) = (refusal, &mut self.replacement) {
            if (replacement.first..=replacement.last).contains(&sequence) {
                replacement
                    .refused
                    .push(format!("{purpose:?}: {}", refusal_text(refusal)));
            }
        }
        match (purpose, refusal) {
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
            (Purpose::RemoveStrip(id), None) => self.mirror.drop_track(id),
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
                if let Some(entry) = self.mirror.loops.get_mut(&id) {
                    entry.content += 1;
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
                            self.mirror.links.insert(link);
                        } else {
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
                        let message =
                            format!("a route to Master was refused: {}", refusal_text(refusal));
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
            (Purpose::BusControl(control), None) => {
                match control {
                    BackendBusControl::GainDb(value) => self.mirror.master.gain_db = value,
                    BackendBusControl::Balance(value) => self.mirror.master.balance = value,
                    BackendBusControl::Mute(value) => self.mirror.master.muted = value,
                }
                self.mirror.mixer_revision += 1;
            }
            (Purpose::BusControl(control), Some(refusal)) => self.failure(
                BackendMutationKind::BusControl,
                Some(MASTER_BUS_ID.raw()),
                Some(BackendMutationDetail::BusControl(control)),
                format!("a Master control was refused: {}", refusal_text(refusal)),
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
            (Purpose::Smoothing(milliseconds), Some(refusal)) => self.failure(
                BackendMutationKind::LoopControl,
                None,
                None,
                format!(
                    "loop boundary smoothing of {milliseconds} ms was refused: {}",
                    refusal_text(refusal)
                ),
            ),
            (Purpose::InjectMidi(_) | Purpose::Smoothing(_) | Purpose::Cleanup, _) => {}
        }
    }
}
