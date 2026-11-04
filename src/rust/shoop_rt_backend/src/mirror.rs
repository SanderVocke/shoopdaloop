//! The application's view of the engine: identities reserved on the control side, values
//! the engine confirmed, and the latest telemetry.

use alloc::borrow::ToOwned;
use alloc::collections::{BTreeMap, BTreeSet};
use alloc::format;
use alloc::string::String;
use alloc::vec::Vec;

use shoop_backend::{
    BackendBusChannelId, BackendBusChannelState, BackendBusControl, BackendBusCreation,
    BackendBusId, BackendBusState, BackendConfirmedLink, BackendHostPortDescriptor, BackendLoopId,
    BackendLoopMode, BackendLoopState, BackendMixerLink, BackendPortDataType,
    BackendPortDescriptor, BackendPortDirection, BackendPortId, BackendPortOwner, BackendPortRole,
    BackendTrackControl, BackendTrackId, BackendTrackState, BackendTrackTopology,
    GLOBAL_FX_PORT_ID, MASTER_BUS_CHANNEL_IDS, MASTER_BUS_CHANNEL_LABELS, MASTER_BUS_ID,
    MASTER_BUS_NAME, MASTER_BUS_OUTPUT_PORT_IDS,
};
use shoop_rt_protocol::{
    BusId, BusPort, Direction, EnginePort, HostPort, HostPortKind, LoopMode, LoopSlotId, PortKind,
    SlotId, StripId, StripPort,
};

/// Reported for a meter without signal.
pub(crate) const SILENT_DB: f32 = -200.0;

pub(crate) fn amplitude_db(amplitude: f32) -> f32 {
    if amplitude > 0.0 {
        20.0 * libm::log10f(amplitude)
    } else {
        SILENT_DB
    }
}

pub(crate) fn db_amplitude(db: f32) -> f32 {
    libm::powf(10.0, db / 20.0)
}

/// Peaks received since the previous snapshot; without new telemetry the previous reading
/// holds.
#[derive(Clone, Debug, Default)]
pub(crate) struct Meter {
    pending: Option<Vec<f32>>,
    reported: Vec<f32>,
}

impl Meter {
    pub fn record(&mut self, peaks: &[f32]) {
        match &mut self.pending {
            Some(pending) => {
                if pending.len() < peaks.len() {
                    pending.resize(peaks.len(), 0.0);
                }
                for (held, peak) in pending.iter_mut().zip(peaks) {
                    *held = held.max(peak.abs());
                }
            }
            None => self.pending = Some(peaks.iter().map(|peak| peak.abs()).collect()),
        }
    }

    pub fn take_db(&mut self, channels: usize) -> Vec<f32> {
        if let Some(pending) = self.pending.take() {
            self.reported = pending.into_iter().map(amplitude_db).collect();
        }
        self.reported.resize(channels, SILENT_DB);
        self.reported.clone()
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Controls {
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub input_monitoring: bool,
}

impl Default for Controls {
    fn default() -> Self {
        Self {
            output_gain_db: 0.0,
            output_balance: 0.0,
            output_muted: false,
            input_gain_db: 0.0,
            input_balance: 0.0,
            input_monitoring: false,
        }
    }
}

impl Controls {
    pub fn apply(&mut self, control: BackendTrackControl) {
        match control {
            BackendTrackControl::OutputGainDb(value) => self.output_gain_db = value,
            BackendTrackControl::OutputBalance(value) => self.output_balance = value,
            BackendTrackControl::OutputMute(value) => self.output_muted = value,
            BackendTrackControl::InputGainDb(value) => self.input_gain_db = value,
            BackendTrackControl::InputBalance(value) => self.input_balance = value,
            BackendTrackControl::InputMonitoring(value) => self.input_monitoring = value,
        }
    }
}

pub(crate) struct Track {
    pub port_name_base: String,
    pub strip: StripId,
    pub audio_channels: u32,
    pub midi: bool,
    /// Per audio channel an input and an output, then MIDI input and output.
    pub ports: Vec<BackendPortId>,
    pub loops: Vec<BackendLoopId>,
    pub controls: Controls,
    pub confirmed: bool,
    pub removing: bool,
    pub input_meter: Meter,
    pub output_meter: Meter,
    pub midi_in: bool,
    pub midi_out: bool,
}

impl Track {
    /// The position of a strip port in `ports`.
    pub fn port_index(&self, kind: PortKind, index: u8) -> Option<usize> {
        let index = usize::from(index);
        let audio = self.audio_channels as usize;
        match kind {
            PortKind::AudioIn if index < audio => Some(2 * index),
            PortKind::AudioOut if index < audio => Some(2 * index + 1),
            PortKind::MidiIn if index == 0 && self.midi => Some(2 * audio),
            PortKind::MidiOut if index == 0 && self.midi => Some(2 * audio + 1),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LoopView {
    pub mode: LoopMode,
    pub next_mode: Option<LoopMode>,
    pub next_delay: Option<u32>,
    pub length: u64,
    pub position: u64,
}

impl Default for LoopView {
    fn default() -> Self {
        Self {
            mode: LoopMode::Stopped,
            next_mode: None,
            next_delay: None,
            length: 0,
            position: 0,
        }
    }
}

pub(crate) struct Loop {
    pub timing: shoop_rt_protocol::TimingState,
    pub slot: LoopSlotId,
    pub track: BackendTrackId,
    pub confirmed: bool,
    pub sync: Option<BackendLoopId>,
    pub gain: f32,
    pub balance: f32,
    pub view: LoopView,
    pub meter: Meter,
    /// Changes whenever the recorded content may have changed.
    pub content: u64,
    pub rt_content_revision: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PortTarget {
    Strip {
        track: BackendTrackId,
        kind: PortKind,
        index: u8,
    },
    Bus(BackendBusId, u8),
    GlobalFx,
}

pub(crate) struct Port {
    pub descriptor: BackendPortDescriptor,
    pub target: PortTarget,
}

pub(crate) struct Bus {
    pub slot: BusId,
    pub name: String,
    pub channels: Vec<BackendBusChannelState>,
    pub confirmed: bool,
    pub removing: bool,
    pub gain_db: f32,
    pub balance: f32,
    pub muted: bool,
    pub meter: Meter,
}

impl Bus {
    pub fn apply(&mut self, control: BackendBusControl) {
        match control {
            BackendBusControl::GainDb(v) => self.gain_db = v,
            BackendBusControl::Balance(v) => self.balance = v,
            BackendBusControl::Mute(v) => self.muted = v,
        }
    }
}

/// Slot reservations with the generation last used for each slot.
#[derive(Clone, Default)]
pub(crate) struct Slots {
    generations: Vec<u32>,
    used: Vec<bool>,
    /// Generations the engine may have used on any slot.
    floor: u32,
}

impl Slots {
    pub fn resize(&mut self, count: usize) {
        if count > self.used.len() {
            self.generations.resize(count, 0);
            self.used.resize(count, false);
        }
    }

    pub fn free(&self) -> usize {
        self.used
            .iter()
            .zip(&self.generations)
            .filter(|(used, generation)| !**used && (**generation).max(self.floor) < u32::MAX)
            .count()
    }

    pub fn capacity(&self) -> usize {
        self.used.len()
    }

    pub fn reserve(&mut self) -> Option<SlotId> {
        let index = self
            .used
            .iter()
            .zip(&self.generations)
            .position(|(used, generation)| !used && (*generation).max(self.floor) < u32::MAX)?;
        self.used[index] = true;
        self.generations[index] = self.generations[index].max(self.floor) + 1;
        Some(SlotId::new(index as u32, self.generations[index]))
    }

    /// The engine used no generation above `floor` on any slot.
    pub fn raise_floor(&mut self, floor: u32) {
        self.floor = self.floor.max(floor);
    }

    pub fn release(&mut self, id: SlotId) {
        if let Some(used) = self.used.get_mut(id.index as usize) {
            *used = false;
        }
    }

    /// Records an identity the engine already uses.
    pub fn adopt(&mut self, id: SlotId) {
        let index = id.index as usize;
        self.resize(index + 1);
        self.used[index] = true;
        self.generations[index] = self.generations[index].max(id.generation);
    }

    /// Takes a released slot back if no one reserved it since.
    pub fn restore(&mut self, id: SlotId) {
        let index = id.index as usize;
        if self.generations.get(index) == Some(&id.generation) {
            self.used[index] = true;
        }
    }
}

pub(crate) struct Mirror {
    pub tracks: BTreeMap<BackendTrackId, Track>,
    pub loops: BTreeMap<BackendLoopId, Loop>,
    pub ports: BTreeMap<BackendPortId, Port>,
    pub strip_tracks: BTreeMap<StripId, BackendTrackId>,
    pub slot_loops: BTreeMap<LoopSlotId, BackendLoopId>,
    pub links: BTreeSet<BackendConfirmedLink>,
    pub desired_links: BTreeSet<BackendConfirmedLink>,
    pub routes: BTreeSet<BackendMixerLink>,
    pub buses: BTreeMap<BackendBusId, Bus>,
    pub bus_slots: Slots,
    next_bus: u64,
    next_channel: u64,
    pub host_ports: Vec<HostPort>,
    pub strip_slots: Slots,
    pub loop_slots: Slots,
    pub connection_revision: u64,
    pub mixer_revision: u64,
    next_track: u64,
    next_loop: u64,
    next_port: u64,
}

impl Mirror {
    pub fn replacement_image(&self) -> Self {
        let mut image = Self::new();
        image.buses.clear();
        image.ports.retain(|_, p| p.target == PortTarget::GlobalFx);
        image.bus_slots = self.bus_slots.clone();
        image.bus_slots.used.fill(false);
        image.next_bus = self.next_bus;
        image.next_channel = self.next_channel;
        image.strip_slots = self.strip_slots.clone();
        image.loop_slots = self.loop_slots.clone();
        image.strip_slots.used.fill(false);
        image.loop_slots.used.fill(false);
        image.host_ports = self.host_ports.clone();
        image.next_track = self.next_track;
        image.next_loop = self.next_loop;
        image.next_port = self.next_port;
        image.connection_revision = self.connection_revision + 1;
        image.mixer_revision = self.mixer_revision + 1;
        image
    }
    pub fn reserve_image_identities(&mut self, image: &Self) {
        for (old, next) in [
            (&mut self.bus_slots, &image.bus_slots),
            (&mut self.strip_slots, &image.strip_slots),
            (&mut self.loop_slots, &image.loop_slots),
        ] {
            for (old, next) in old.generations.iter_mut().zip(&next.generations) {
                *old = (*old).max(*next);
            }
        }
        self.next_bus = self.next_bus.max(image.next_bus);
        self.next_channel = self.next_channel.max(image.next_channel);
        self.next_track = self.next_track.max(image.next_track);
        self.next_loop = self.next_loop.max(image.next_loop);
        self.next_port = self.next_port.max(image.next_port);
    }
    pub fn new() -> Self {
        let mut ports = BTreeMap::new();
        ports.insert(
            GLOBAL_FX_PORT_ID,
            Port {
                descriptor: BackendPortDescriptor {
                    id: GLOBAL_FX_PORT_ID,
                    owner: BackendPortOwner::GlobalFxControl,
                    name: "Global FX Control MIDI In".to_owned(),
                    data_type: BackendPortDataType::Midi,
                    direction: BackendPortDirection::Input,
                    role: BackendPortRole::MidiInput,
                },
                target: PortTarget::GlobalFx,
            },
        );
        for (index, id) in MASTER_BUS_OUTPUT_PORT_IDS.into_iter().enumerate() {
            ports.insert(
                id,
                Port {
                    descriptor: BackendPortDescriptor {
                        id,
                        owner: BackendPortOwner::Bus(MASTER_BUS_ID),
                        name: format!("master_out_{}", index + 1),
                        data_type: BackendPortDataType::Audio,
                        direction: BackendPortDirection::Output,
                        role: BackendPortRole::AudioOutput,
                    },
                    target: PortTarget::Bus(MASTER_BUS_ID, index as u8),
                },
            );
        }
        Self {
            tracks: BTreeMap::new(),
            loops: BTreeMap::new(),
            ports,
            strip_tracks: BTreeMap::new(),
            slot_loops: BTreeMap::new(),
            links: BTreeSet::new(),
            desired_links: BTreeSet::new(),
            routes: BTreeSet::new(),
            buses: [(
                MASTER_BUS_ID,
                Bus {
                    slot: BusId::MASTER,
                    name: MASTER_BUS_NAME.to_owned(),
                    channels: MASTER_BUS_CHANNEL_IDS
                        .iter()
                        .zip(MASTER_BUS_CHANNEL_LABELS)
                        .zip(MASTER_BUS_OUTPUT_PORT_IDS)
                        .map(|((id, label), port)| BackendBusChannelState {
                            id: *id,
                            label: label.to_owned(),
                            output_port_id: port,
                        })
                        .collect(),
                    confirmed: true,
                    removing: false,
                    gain_db: 0.,
                    balance: 0.,
                    muted: false,
                    meter: Meter::default(),
                },
            )]
            .into_iter()
            .collect(),
            bus_slots: {
                let mut slots = Slots::default();
                slots.adopt(BusId::MASTER.0);
                slots
            },
            next_bus: 2,
            next_channel: 3,
            host_ports: Vec::new(),
            strip_slots: Slots::default(),
            loop_slots: Slots::default(),
            connection_revision: 1,
            mixer_revision: 1,
            next_track: 1,
            next_loop: 1,
            next_port: 1,
        }
    }

    pub fn next_track_id(&mut self) -> BackendTrackId {
        let id = BackendTrackId::from_raw(self.next_track);
        self.next_track += 1;
        id
    }

    pub fn next_loop_id(&mut self) -> BackendLoopId {
        let id = BackendLoopId::from_raw(self.next_loop);
        self.next_loop += 1;
        id
    }

    pub fn next_port_id(&mut self) -> BackendPortId {
        let id = BackendPortId::from_raw(self.next_port);
        self.next_port += 1;
        id
    }

    /// A track that exists and is not being removed.
    pub fn live_track(&self, id: BackendTrackId) -> Option<&Track> {
        self.tracks.get(&id).filter(|track| !track.removing)
    }

    /// A loop whose track exists and is not being removed.
    pub fn live_loop(&self, id: BackendLoopId) -> Option<&Loop> {
        self.loops
            .get(&id)
            .filter(|entry| self.live_track(entry.track).is_some())
    }

    /// A port whose owner exists and is not being removed.
    pub fn live_port(&self, id: BackendPortId) -> Option<&Port> {
        self.ports.get(&id).filter(|port| match port.target {
            PortTarget::Strip { track, .. } => self.live_track(track).is_some(),
            PortTarget::Bus(id, _) => self.buses.get(&id).is_some_and(|bus| !bus.removing),
            PortTarget::GlobalFx => true,
        })
    }

    pub fn engine_port(&self, target: PortTarget) -> Option<EnginePort> {
        match target {
            PortTarget::Strip { track, kind, index } => {
                let strip = self.tracks.get(&track)?.strip;
                Some(EnginePort::Strip(StripPort { strip, kind, index }))
            }
            PortTarget::Bus(id, index) => Some(EnginePort::Bus(BusPort {
                bus: self.buses.get(&id)?.slot,
                index,
            })),
            PortTarget::GlobalFx => None,
        }
    }

    /// The application port of an engine port.
    pub fn port_of(&self, port: EnginePort) -> Option<BackendPortId> {
        match port {
            EnginePort::Strip(port) => {
                let track = self.tracks.get(self.strip_tracks.get(&port.strip)?)?;
                track
                    .port_index(port.kind, port.index)
                    .and_then(|index| track.ports.get(index).copied())
            }
            EnginePort::Bus(BusPort { bus, index }) => self
                .buses
                .values()
                .find(|b| b.slot == bus)?
                .channels
                .get(index as usize)
                .map(|c| c.output_port_id),
        }
    }

    pub fn host_by_name(&self, name: &str) -> Option<&HostPort> {
        self.host_ports
            .iter()
            .find(|port| port.name.as_str() == name)
    }

    pub fn host_by_id(&self, id: shoop_rt_protocol::HostPortId) -> Option<&HostPort> {
        self.host_ports.iter().find(|port| port.id == id)
    }

    /// Forgets a removed track with its loops, ports, links and routes.
    pub fn drop_track(&mut self, id: BackendTrackId) {
        let Some(track) = self.tracks.remove(&id) else {
            return;
        };
        if self.strip_tracks.get(&track.strip) == Some(&id) {
            self.strip_tracks.remove(&track.strip);
        }
        for loop_id in &track.loops {
            self.drop_loop(*loop_id);
        }
        for port in &track.ports {
            self.ports.remove(port);
        }
        let links = self.links.len();
        self.links
            .retain(|link| !track.ports.contains(&link.application_port_id));
        self.desired_links
            .retain(|link| !track.ports.contains(&link.application_port_id));
        let routes = self.routes.len();
        self.routes
            .retain(|link| !track.ports.contains(&link.source_port_id));
        self.connection_revision += 1;
        if routes != self.routes.len() || links != self.links.len() {
            self.mixer_revision += 1;
        }
    }

    pub fn drop_loop(&mut self, id: BackendLoopId) {
        let Some(entry) = self.loops.remove(&id) else {
            return;
        };
        if self.slot_loops.get(&entry.slot) == Some(&id) {
            self.slot_loops.remove(&entry.slot);
        }
        if let Some(track) = self.tracks.get_mut(&entry.track) {
            track.loops.retain(|candidate| *candidate != id);
        }
        for other in self.loops.values_mut() {
            if other.sync == Some(id) {
                other.sync = None;
            }
        }
    }

    pub fn track_state(track: &mut Track) -> BackendTrackState {
        let channels = track.audio_channels as usize;
        let controls = track.controls;
        BackendTrackState {
            topology: BackendTrackTopology::Direct {
                audio_channels: track.audio_channels,
                midi: track.midi,
            },
            audio_channels: track.audio_channels,
            midi: track.midi,
            output_gain_db: controls.output_gain_db,
            output_balance: controls.output_balance,
            output_muted: controls.output_muted,
            input_gain_db: controls.input_gain_db,
            input_balance: controls.input_balance,
            input_monitoring: controls.input_monitoring,
            input_peaks: track.input_meter.take_db(channels),
            output_peaks: track.output_meter.take_db(channels),
            input_midi_activity: core::mem::take(&mut track.midi_in),
            output_midi_activity: core::mem::take(&mut track.midi_out),
            ..Default::default()
        }
    }

    pub fn loop_state(entry: &mut Loop, channels: u32, midi: bool) -> BackendLoopState {
        let view = entry.view;
        let timing = entry.timing;
        let channel_timings = (0..channels as usize)
            .map(|channel| {
                let window = timing.windows.audio[channel];
                shoop_backend::BackendChannelTimingState {
                    channel,
                    is_midi: false,
                    frames: window.raw_frames.unwrap_or(timing.raw_audio_frames),
                    start_offset: window.start_offset,
                    capture_alignment_frames: window.capture_alignment,
                    preplay: window.preplay,
                }
            })
            .chain(midi.then(|| {
                let window = timing.windows.midi;
                shoop_backend::BackendChannelTimingState {
                    channel: 0,
                    is_midi: true,
                    frames: window.raw_frames.unwrap_or(timing.raw_midi_frames),
                    start_offset: window.start_offset,
                    capture_alignment_frames: window.capture_alignment,
                    preplay: window.preplay,
                }
            }))
            .collect();
        BackendLoopState {
            timing_revision: timing.revision,
            channel_timings,
            capture_alignment_frames: if channels > 0 {
                timing.windows.audio[0].capture_alignment
            } else {
                timing.windows.midi.capture_alignment
            },
            content_revision: entry.content,
            sync_source: entry.sync,
            mode: app_mode(view.mode),
            length: u32::try_from(view.length).unwrap_or(u32::MAX),
            position: u32::try_from(view.position).unwrap_or(u32::MAX),
            next_mode: view.next_mode.map(app_mode),
            next_transition_delay: view.next_delay,
            stereo: channels == 2,
            gain: entry.gain,
            balance: entry.balance,
            audio_peaks: entry.meter.take_db(channels as usize),
            ..Default::default()
        }
    }

    pub fn bus_states(&mut self) -> BTreeMap<BackendBusId, BackendBusState> {
        self.buses
            .iter_mut()
            .filter(|(_, b)| b.confirmed)
            .map(|(id, bus)| {
                (
                    *id,
                    BackendBusState {
                        id: *id,
                        name: bus.name.clone(),
                        channels: bus.channels.clone(),
                        gain_db: bus.gain_db,
                        balance: bus.balance,
                        muted: bus.muted,
                        output_peaks_db: bus.meter.take_db(bus.channels.len()),
                        fx: None,
                    },
                )
            })
            .collect()
    }
    pub fn channel(&self, target: BusPort) -> Option<BackendBusChannelId> {
        self.buses
            .values()
            .find(|bus| bus.slot == target.bus)?
            .channels
            .get(target.index as usize)
            .map(|c| c.id)
    }
    pub fn bus_for_channel(&self, id: BackendBusChannelId) -> Option<BusPort> {
        self.buses.values().filter(|b| !b.removing).find_map(|bus| {
            bus.channels
                .iter()
                .position(|c| c.id == id)
                .map(|index| BusPort {
                    bus: bus.slot,
                    index: index as u8,
                })
        })
    }
    pub fn create_bus(&mut self, name: String, channels: u8) -> Option<BackendBusCreation> {
        self.next_bus.checked_add(1)?;
        self.next_channel.checked_add(u64::from(channels))?;
        self.next_port.checked_add(u64::from(channels))?;
        let slot = BusId(self.bus_slots.reserve()?);
        let id = BackendBusId::from_raw(self.next_bus);
        self.next_bus = self.next_bus.checked_add(1)?;
        let mut outputs = Vec::new();
        for (index, label) in shoop_backend::default_bus_channel_labels(channels as usize)
            .ok()?
            .into_iter()
            .enumerate()
        {
            let channel = BackendBusChannelId::from_raw(self.next_channel);
            self.next_channel = self.next_channel.checked_add(1)?;
            let port = self.next_port_id();
            self.ports.insert(
                port,
                Port {
                    descriptor: BackendPortDescriptor {
                        id: port,
                        owner: BackendPortOwner::Bus(id),
                        name: format!("bus_{}_out_{}", id.raw(), index + 1),
                        data_type: BackendPortDataType::Audio,
                        direction: BackendPortDirection::Output,
                        role: BackendPortRole::AudioOutput,
                    },
                    target: PortTarget::Bus(id, index as u8),
                },
            );
            outputs.push(BackendBusChannelState {
                id: channel,
                label,
                output_port_id: port,
            });
        }
        self.buses.insert(
            id,
            Bus {
                slot,
                name,
                channels: outputs.clone(),
                confirmed: false,
                removing: false,
                gain_db: 0.,
                balance: 0.,
                muted: false,
                meter: Meter::default(),
            },
        );
        Some(BackendBusCreation {
            bus_id: id,
            channels: outputs,
        })
    }
    pub fn drop_bus(&mut self, id: BackendBusId) {
        if let Some(bus) = self.buses.remove(&id) {
            for channel in &bus.channels {
                self.ports.remove(&channel.output_port_id);
            }
            self.links.retain(|l| {
                !bus.channels
                    .iter()
                    .any(|c| c.output_port_id == l.application_port_id)
            });
            self.desired_links.retain(|l| {
                !bus.channels
                    .iter()
                    .any(|c| c.output_port_id == l.application_port_id)
            });
            self.routes.retain(|r| {
                !bus.channels
                    .iter()
                    .any(|c| c.id == r.destination_channel_id)
            });
            self.connection_revision += 1;
            self.mixer_revision += 1;
        }
    }

    /// Application ports of confirmed tracks, buses and the global FX port.
    pub fn application_ports(&self) -> BTreeMap<BackendPortId, BackendPortDescriptor> {
        self.ports
            .iter()
            .filter(|(_, port)| match port.target {
                PortTarget::Strip { track, .. } => {
                    self.tracks.get(&track).is_some_and(|track| track.confirmed)
                }
                PortTarget::Bus(id, _) => self.buses.get(&id).is_some_and(|bus| bus.confirmed),
                PortTarget::GlobalFx => true,
            })
            .map(|(id, port)| (*id, port.descriptor.clone()))
            .collect()
    }

    pub fn host_descriptors(&self) -> BTreeMap<String, BackendHostPortDescriptor> {
        self.host_ports
            .iter()
            .filter(|port| {
                !port
                    .name
                    .as_str()
                    .starts_with(shoop_rt_protocol::HOST_IO_PREFIX)
            })
            .map(|port| {
                let id = port.name.as_str().to_owned();
                (
                    id.clone(),
                    BackendHostPortDescriptor {
                        id: id.clone(),
                        name: id,
                        data_type: host_data_type(port.kind),
                        direction: host_direction(port.direction),
                        preferred_playback: port.preferred,
                    },
                )
            })
            .collect()
    }
}

pub(crate) fn app_mode(mode: LoopMode) -> BackendLoopMode {
    match mode {
        LoopMode::Stopped => BackendLoopMode::Stopped,
        LoopMode::Playing => BackendLoopMode::Playing,
        LoopMode::Recording => BackendLoopMode::Recording,
    }
}

pub(crate) fn host_data_type(kind: HostPortKind) -> BackendPortDataType {
    match kind {
        HostPortKind::Audio => BackendPortDataType::Audio,
        HostPortKind::Midi => BackendPortDataType::Midi,
    }
}

/// Host ports are described from the application's side: a capture source is an output.
pub(crate) fn host_direction(direction: Direction) -> BackendPortDirection {
    match direction {
        Direction::Input => BackendPortDirection::Output,
        Direction::Output => BackendPortDirection::Input,
    }
}
