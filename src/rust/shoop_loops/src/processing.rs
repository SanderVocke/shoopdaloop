//! Direct tick-based primitive audio/MIDI loops and coordinated recording.
//!
//! The render owner applies copy-valued commands at tick boundaries. Immutable
//! playback indices are cached until content changes. Capture reserves complete
//! ticks before writing and publishes a coherent take at a boundary. Sync/program
//! compilation and application integration are separate migration milestones.

use crate::{audio, media, midi, LoopId, Mode};
use alloc::{vec, vec::Vec};
use rt_runtime::{
    graph::{GraphDescription, PortCounts},
    midi_state::MidiState,
    processing::{self, Block, CommandControl, CommandQueue, Processor, Quantum, State, Tick},
    resource::*,
    Error, Frame, NodeId, ResourceId, Revision,
};

#[derive(Clone, Copy, Debug)]
pub struct Binding {
    pub target: LoopId,
    pub playback: NodeId,
    pub capture: NodeId,
    pub channels: usize,
    pub midi: bool,
    pub gain: f32,
    pub smoothing: audio::Smoothing,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Command {
    Play {
        target: LoopId,
        position: u64,
    },
    Stop {
        target: LoopId,
    },
    Seek {
        target: LoopId,
        position: u64,
    },
    Record {
        target: LoopId,
        monitor_previous: bool,
    },
    Gain {
        target: LoopId,
        value: f32,
    },
    State {
        target: LoopId,
    },
}
impl Default for Command {
    fn default() -> Self {
        Self::State {
            target: LoopId::from_parts(0, 1).expect("initial identity"),
        }
    }
}
impl Command {
    fn target(self) -> LoopId {
        match self {
            Self::Play { target, .. }
            | Self::Stop { target }
            | Self::Seek { target, .. }
            | Self::Record { target, .. }
            | Self::Gain { target, .. }
            | Self::State { target } => target,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Status {
    pub target: LoopId,
    pub mode: Mode,
    pub position: u64,
    pub length: u64,
    pub capture_frames: u64,
    pub cycles: u64,
    pub content_revision: Revision,
    pub capture_revision: Option<Revision>,
    pub failure: Option<Error>,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Reply {
    #[default]
    Done,
    State(Status),
}

#[derive(Clone, Copy)]
struct Capture {
    take: CaptureTakeId,
    media: media::MediaId,
    frames: u64,
}
#[derive(Clone, Copy)]
struct Entry {
    binding: Binding,
    mode: Mode,
    position: u64,
    length: u64,
    cycles: u64,
    revision: Revision,
    content_revision: Revision,
    audio: Option<ReadIndex>,
    midi: Option<ReadIndex>,
    discontinuity: bool,
    monitor: bool,
    groups: [Option<CaptureGroupId>; 2],
    resources: [[Option<ResourceId>; 2]; 2],
    take: Option<Capture>,
    interval: Option<CaptureIntervalId>,
    next_bank: usize,
    failure: Option<Error>,
}
impl Entry {
    fn status(self, q: usize) -> Status {
        Status {
            target: self.binding.target,
            mode: self.mode,
            position: self.position * q as u64,
            length: self.length * q as u64,
            capture_frames: self.take.map_or(0, |t| t.frames),
            cycles: self.cycles,
            content_revision: self.content_revision,
            capture_revision: self.take.map(|_| self.revision),
            failure: self.failure,
        }
    }
    fn plays(self) -> bool {
        self.length > 0
            && (self.mode == Mode::Playing || (self.mode == Mode::Recording && self.monitor))
    }
}
pub struct Bank {
    quantum: Quantum,
    entries: Vec<Option<Entry>>,
    active: Vec<usize>,
    inputs: Vec<MidiState>,
    ledger: media::Ledger,
    commands: CommandQueue<Command, Reply>,
}
pub struct Prepared {
    pub bank: Bank,
    pub control: CommandControl<Command, Reply>,
    pub nodes: Vec<(NodeId, Node)>,
    pub resources: ResourceStore,
}
pub enum Node {
    Playback(Playback),
    Capture(Input),
}
pub struct Playback {
    slot: usize,
    binding: Binding,
    audio: Option<audio::AudioRenderer>,
    midi: Option<midi::MidiRenderer>,
}
pub struct Input {
    slot: usize,
    binding: Binding,
    audio: Option<audio::AudioCapture>,
    midi: Option<midi::MidiCapture>,
}

impl Bank {
    /// Prepare the resource pools before importing source media.
    pub fn resource_store(
        config: processing::PreparedConfig,
        loops: usize,
    ) -> Result<ResourceStore, Error> {
        let c = config.resources().limits();
        let staging = config
            .quantum()
            .frames()
            .checked_mul(64 * 4)
            .and_then(|v| v.checked_add(c.midi_bytes_per_port))
            .and_then(|v| v.checked_add(c.midi_events_per_port.checked_mul(12)?))
            .ok_or(Error::Capacity)?;
        ResourceStore::with_capture_groups(
            config.resources(),
            Some(MidiIndexConfig {
                versions: c.resource_slots,
                checkpoints_per_version: 4,
                replay_events: c.midi_events_per_port,
            }),
            CaptureGroupConfig {
                groups: loops.checked_mul(2).ok_or(Error::Capacity)?.max(1),
                members_per_group: 2,
                staging_bytes_per_group: staging,
            },
        )
    }
    /// Consumes the store and source leases; failures destroy prepared owners off-render.
    pub fn prepare(
        config: processing::PreparedConfig,
        mut resources: ResourceStore,
        bindings: Vec<Binding>,
        imports: Vec<media::Input>,
    ) -> Result<Prepared, Error> {
        if resources.config() != config.resources() {
            return Err(Error::InvalidConfig);
        }
        let c = config.resources().limits();
        let q = config.quantum();
        let slots = bindings
            .iter()
            .map(|b| b.target.slot() as usize + 1)
            .max()
            .unwrap_or(0);
        if slots > c.max_nodes
            || slots
                .checked_mul(size_of::<Entry>() + size_of::<MidiState>())
                .is_none_or(|n| n > i32::MAX as usize)
        {
            return Err(Error::Capacity);
        }
        let mut ledger = media::Ledger::prepare(
            slots.max(1),
            bindings
                .len()
                .checked_mul(4)
                .and_then(|n| n.checked_add(8))
                .ok_or(Error::Capacity)?,
            c.command_slots,
            1,
        )?;
        let mut entries = vec![None; slots];
        let mut active = Vec::with_capacity(bindings.len());
        let mut nodes = Vec::with_capacity(bindings.len() * 2);
        for b in bindings {
            let slot = b.target.slot() as usize;
            if entries[slot].is_some()
                || b.playback == b.capture
                || b.channels > 64
                || (!b.midi && b.channels == 0)
                || !b.gain.is_finite()
            {
                return Err(Error::InvalidConfig);
            }
            ledger.bind(b.target)?;
            let mut e = Entry {
                binding: b,
                mode: Mode::Stopped,
                position: 0,
                length: 0,
                cycles: 0,
                revision: Revision(0),
                content_revision: Revision(0),
                audio: None,
                midi: None,
                discontinuity: false,
                monitor: false,
                groups: [None; 2],
                resources: [[None; 2]; 2],
                take: None,
                interval: None,
                next_bank: 0,
                failure: None,
            };
            for bank in 0..2 {
                let mut members = Vec::new();
                if b.channels > 0 {
                    let r = resources.create()?;
                    e.resources[bank][0] = Some(r);
                    members.push(CaptureMember {
                        resource: r,
                        format: Format::Audio {
                            channels: b.channels as u32,
                        },
                        midi_events: 0,
                        midi_bytes: 0,
                    });
                }
                if b.midi {
                    let r = resources.create()?;
                    e.resources[bank][1] = Some(r);
                    members.push(CaptureMember {
                        resource: r,
                        format: Format::Midi,
                        midi_events: c
                            .midi_events_per_port
                            .max(rt_runtime::midi::ResetState::EVENTS),
                        midi_bytes: c
                            .midi_bytes_per_port
                            .max(rt_runtime::midi::ResetState::BYTES),
                    });
                }
                e.groups[bank] = Some(resources.bind_capture_group(&members)?);
            }
            nodes.push((
                b.playback,
                Node::Playback(Playback {
                    slot,
                    binding: b,
                    audio: if b.channels > 0 {
                        Some(audio::AudioRenderer::prepare(
                            b.channels,
                            q.frames(),
                            b.smoothing,
                        )?)
                    } else {
                        None
                    },
                    midi: if b.midi {
                        Some(midi::MidiRenderer::prepare(
                            q.frames(),
                            c.midi_bytes_per_port,
                        )?)
                    } else {
                        None
                    },
                }),
            ));
            nodes.push((
                b.capture,
                Node::Capture(Input {
                    slot,
                    binding: b,
                    audio: if b.channels > 0 {
                        Some(audio::AudioCapture::prepare(b.channels, q.frames())?)
                    } else {
                        None
                    },
                    midi: if b.midi {
                        Some(midi::MidiCapture::prepare(
                            q.frames(),
                            c.midi_events_per_port,
                            c.midi_bytes_per_port,
                        )?)
                    } else {
                        None
                    },
                }),
            ));
            entries[slot] = Some(e);
            active.push(slot);
        }
        for input in imports {
            let target = input.description.owner;
            let slot = target.slot() as usize;
            let e = entries
                .get_mut(slot)
                .and_then(Option::as_mut)
                .filter(|e| e.binding.target == target)
                .ok_or(Error::InvalidId)?;
            if ledger.playback(target)?.is_some()
                || input.description.channels as usize != e.binding.channels
                || input.description.midi != e.binding.midi
            {
                return Err(Error::InvalidShape);
            }
            let length = input.description.frames;
            if length != 0 {
                q.duration_ticks(length)?;
            }
            let revision = input.description.revision;
            let id = ledger
                .adopt(&mut resources.io(), input)
                .map_err(|e| e.error)?;
            ledger.set_playback(&mut resources.io(), target, Some(id))?;
            let view = ledger.view(id)?;
            e.audio = view.audio.map(|s| s.index);
            e.midi = view.midi.map(|s| s.index);
            e.length = length / q.frames() as u64;
            e.revision = revision;
            e.content_revision = revision;
        }
        let (control, commands) =
            processing::command_channel_with_budget(q, c.command_slots, c.budgets.commands)?;
        Ok(Prepared {
            bank: Self {
                quantum: q,
                entries,
                active,
                inputs: vec![MidiState::default(); slots],
                ledger,
                commands,
            },
            control,
            nodes,
            resources,
        })
    }
    pub fn status(&self, target: LoopId) -> Result<Status, Error> {
        self.entries
            .get(target.slot() as usize)
            .and_then(|e| *e)
            .filter(|e| e.binding.target == target)
            .map(|e| e.status(self.quantum.frames()))
            .ok_or(Error::InvalidId)
    }
    /// Quiescent teardown: settle transport before releasing media owners.
    pub fn shutdown(
        &mut self,
        control: &mut CommandControl<Command, Reply>,
        resources: &mut ResourceStore,
        frame: Frame,
    ) -> Result<(), Error> {
        self.commands.shutdown(control, frame)?;
        self.cleanup(resources, frame)
    }
    /// Call after stopping callbacks; release internal media and capture resources.
    pub fn cleanup(&mut self, resources: &mut ResourceStore, frame: Frame) -> Result<(), Error> {
        for &slot in &self.active {
            if let Some(e) = self.entries[slot].as_mut() {
                if let Some(interval) = e.interval {
                    resources
                        .planner_io(frame, frame)
                        .capture()
                        .cancel_interval(interval)?;
                    e.interval = None;
                }
                if e.take.is_some() {
                    freeze(
                        e,
                        &mut self.ledger,
                        resources,
                        frame,
                        Some(Error::Cancelled),
                        self.quantum,
                    )?;
                }
                e.mode = Mode::Stopped;
                for bank in 0..2 {
                    if let Some(group) = e.groups[bank] {
                        resources.release_capture_group(group)?;
                        e.groups[bank] = None;
                    }
                    for resource in &mut e.resources[bank] {
                        if let Some(r) = *resource {
                            resources.remove(r)?;
                            *resource = None;
                        }
                    }
                }
            }
        }
        self.ledger.release_all(&mut resources.io())
    }
}

fn freeze(
    e: &mut Entry,
    ledger: &mut media::Ledger,
    resources: &mut ResourceStore,
    at: Frame,
    incomplete: Option<Error>,
    q: Quantum,
) -> Result<(), Error> {
    let take = e.take.ok_or(Error::InvalidState)?;
    if let Some(interval) = e.interval {
        resources
            .planner_io(at, at)
            .capture()
            .cancel_interval(interval)?;
        e.interval = None;
    }
    let mut frozen = [None; 2];
    let members = usize::from(e.binding.channels > 0) + usize::from(e.binding.midi);
    resources.planner_io(at, at).capture().freeze(
        take.take,
        at,
        incomplete,
        &mut frozen[..members],
    )?;
    let mut i = 0;
    let mut source = || -> Result<media::Source, Error> {
        let snapshot = frozen[i].ok_or(Error::InvalidState)?;
        i += 1;
        Ok(media::Source {
            snapshot,
            index: resources.read_index(snapshot.id)?,
        })
    };
    let input = media::Input {
        description: media::Description {
            owner: e.binding.target,
            revision: e.revision,
            frames: take.frames,
            channels: e.binding.channels as u32,
            midi: e.binding.midi,
        },
        audio: if e.binding.channels > 0 {
            Some(source()?)
        } else {
            None
        },
        midi: if e.binding.midi {
            Some(source()?)
        } else {
            None
        },
    };
    ledger
        .publish(&mut resources.io(), take.media, input)
        .map_err(|e| e.error)?;
    ledger.set_latest(&mut resources.io(), e.binding.target, Some(take.media))?;
    ledger.set_playback(&mut resources.io(), e.binding.target, Some(take.media))?;
    ledger.retire(&mut resources.io(), take.media)?;
    let view = ledger.view(take.media)?;
    e.audio = view.audio.map(|s| s.index);
    e.midi = view.midi.map(|s| s.index);
    e.content_revision = e.revision;
    e.length = take.frames / q.frames() as u64;
    e.position = 0;
    e.discontinuity = true;
    e.take = None;
    e.failure = incomplete;
    Ok(())
}
fn apply(
    e: &mut Entry,
    ledger: &mut media::Ledger,
    initial: &MidiState,
    resources: &mut ResourceStore,
    tick: Tick,
    command: Command,
) -> Result<Reply, Error> {
    let q = tick.quantum.frames() as u64;
    match command {
        Command::State { .. } => return Ok(Reply::State(e.status(q as usize))),
        Command::Gain { value, .. } => {
            if !value.is_finite() {
                return Err(Error::InvalidParameter);
            }
            e.binding.gain = value;
        }
        Command::Record {
            monitor_previous, ..
        } => {
            if e.take.is_some() {
                return Err(Error::Busy);
            }
            let revision = e.revision.checked_next()?;
            let media = ledger.reserve(media::Description {
                owner: e.binding.target,
                revision,
                frames: 0,
                channels: e.binding.channels as u32,
                midi: e.binding.midi,
            })?;
            let bank = e.next_bank;
            let result = (|| {
                let mut io = resources.planner_io(tick.frame, tick.end()?);
                let take = io.capture().prepare(
                    e.groups[bank].ok_or(Error::InvalidState)?,
                    if e.binding.midi {
                        core::slice::from_ref(initial)
                    } else {
                        &[]
                    },
                )?;
                let interval = match io.capture().reserve_prepared(take, tick.frame, q as usize) {
                    Ok(v) => v,
                    Err(error) => {
                        io.capture().cancel_take(take)?;
                        return Err(error);
                    }
                };
                if let Err(error) = io.capture().activate(take, tick.frame) {
                    io.capture().cancel_interval(interval)?;
                    io.capture().cancel_take(take)?;
                    return Err(error);
                }
                Ok((take, interval))
            })();
            let (take, interval) = match result {
                Ok(v) => v,
                Err(error) => {
                    ledger.retire(&mut resources.io(), media)?;
                    return Err(error);
                }
            };
            e.take = Some(Capture {
                take,
                media,
                frames: 0,
            });
            e.interval = Some(interval);
            e.next_bank = 1 - bank;
            e.mode = Mode::Recording;
            e.monitor = monitor_previous;
            e.revision = revision;
            e.failure = None;
        }
        Command::Stop { .. } => {
            if e.take.is_some() {
                freeze(e, ledger, resources, tick.frame, None, tick.quantum)?;
            }
            e.mode = Mode::Stopped;
            e.position = 0;
            e.discontinuity = false;
        }
        Command::Play { position, .. } | Command::Seek { position, .. } => {
            let length = e.take.map_or(e.length * q, |t| t.frames);
            if position % q != 0 || position >= length {
                return Err(Error::InvalidFrame);
            }
            if matches!(command, Command::Seek { .. }) && e.take.is_some() {
                return Err(Error::Busy);
            }
            if e.take.is_some() {
                freeze(e, ledger, resources, tick.frame, None, tick.quantum)?;
            }
            e.position = position / q;
            e.discontinuity = true;
            if matches!(command, Command::Play { .. }) {
                e.mode = Mode::Playing;
            }
        }
    }
    Ok(Reply::Done)
}
impl State for Bank {
    fn validate_activation(&self, graph: &GraphDescription) -> Result<(), Error> {
        self.validate_graph(graph)
    }
    fn validate_graph(&self, graph: &GraphDescription) -> Result<(), Error> {
        for &slot in &self.active {
            let b = self.entries[slot].ok_or(Error::InvalidState)?.binding;
            if !graph.nodes.contains(&b.playback) || !graph.nodes.contains(&b.capture) {
                return Err(Error::InvalidGraph);
            }
        }
        Ok(())
    }
    fn begin_callback(&mut self, frame: Frame, _: &mut ResourceStore) -> Result<(), Error> {
        self.commands.begin_callback(frame)
    }
    fn begin_callback_with_budget(
        &mut self,
        frame: Frame,
        _: &mut ResourceStore,
        commands: usize,
    ) -> Result<(), Error> {
        self.commands
            .begin_callback_with_budget(frame, commands)
            .map(|_| ())
    }
    fn begin_tick(&mut self, tick: Tick, resources: &mut ResourceStore) -> Result<(), Error> {
        if tick.quantum != self.quantum {
            return Err(Error::InvalidConfig);
        }
        let entries = &mut self.entries;
        let ledger = &mut self.ledger;
        let inputs = &self.inputs;
        self.commands.dispatch(tick.frame, |command| {
            let target = command.target();
            let slot = target.slot() as usize;
            let e = entries
                .get_mut(slot)
                .and_then(Option::as_mut)
                .filter(|e| e.binding.target == target)
                .ok_or(Error::InvalidId)?;
            apply(e, ledger, &inputs[slot], resources, tick, command)
        })?;
        for &slot in &self.active {
            let e = self.entries[slot].as_mut().ok_or(Error::InvalidState)?;
            if let Some(take) = e.take.filter(|_| e.interval.is_none()) {
                match resources
                    .planner_io(tick.frame, tick.end()?)
                    .capture()
                    .reserve(take.take, tick.frame, tick.quantum.frames())
                {
                    Ok(interval) => e.interval = Some(interval),
                    Err(error @ (Error::Capacity | Error::Exhausted)) => {
                        freeze(
                            e,
                            &mut self.ledger,
                            resources,
                            tick.frame,
                            Some(error),
                            self.quantum,
                        )?;
                        e.mode = Mode::Stopped;
                    }
                    Err(error) => return Err(error),
                }
            }
        }
        Ok(())
    }
    fn end_tick(&mut self, tick: Tick, resources: &mut ResourceStore) -> Result<(), Error> {
        for &slot in &self.active {
            let e = self.entries[slot].as_mut().ok_or(Error::InvalidState)?;
            if let Some(interval) = e.interval {
                resources
                    .planner_io(tick.end()?, tick.end()?)
                    .capture()
                    .commit(interval)?;
                e.interval = None;
                let take = e.take.as_mut().ok_or(Error::InvalidState)?;
                take.frames = take
                    .frames
                    .checked_add(tick.quantum.frames() as u64)
                    .ok_or(Error::Exhausted)?;
            }
            e.discontinuity = false;
            if e.plays() {
                e.position += 1;
                if e.position == e.length {
                    e.position = 0;
                    e.cycles = e.cycles.checked_add(1).ok_or(Error::Exhausted)?;
                    e.discontinuity = true;
                }
            }
        }
        Ok(())
    }
}
impl Processor<Bank> for Node {
    fn validate_state(&self, id: NodeId, state: &Bank) -> Result<(), Error> {
        let (slot, b) = match self {
            Self::Playback(p) => (p.slot, p.binding),
            Self::Capture(p) => (p.slot, p.binding),
        };
        let expected = match self {
            Self::Playback(_) => b.playback,
            Self::Capture(_) => b.capture,
        };
        if id != expected {
            return Err(Error::InvalidId);
        }
        let e = state
            .entries
            .get(slot)
            .and_then(|e| *e)
            .ok_or(Error::InvalidId)?;
        if e.binding.target != b.target
            || e.binding.playback != b.playback
            || e.binding.capture != b.capture
            || e.binding.channels != b.channels
            || e.binding.midi != b.midi
        {
            return Err(Error::InvalidGraph);
        }
        Ok(())
    }
    fn ports(&self) -> PortCounts {
        match self {
            Self::Playback(p) => PortCounts {
                audio_outputs: p.binding.channels,
                midi_outputs: usize::from(p.binding.midi),
                ..Default::default()
            },
            Self::Capture(p) => PortCounts {
                audio_inputs: p.binding.channels,
                midi_inputs: usize::from(p.binding.midi),
                ..Default::default()
            },
        }
    }
    fn process(&mut self, block: &mut Block<'_>, state: &mut Bank) -> Result<(), Error> {
        match self {
            Self::Playback(p) => {
                let e = state.entries[p.slot].ok_or(Error::InvalidState)?;
                let playing = e.plays();
                let position = e.position * block.tick.quantum.frames() as u64;
                if let Some(audio) = &mut p.audio {
                    audio.render_ports(
                        &mut block.resources.render(),
                        audio::Playback {
                            source: e.audio.filter(|_| playing),
                            position,
                            gain: e.binding.gain,
                            balance: 0.,
                            discontinuity: e.discontinuity,
                        },
                        &mut block.audio,
                    )?;
                }
                if let Some(midi) = &mut p.midi {
                    let report = midi.render(
                        &mut block.resources.render(),
                        midi::Playback {
                            source: e.midi.filter(|_| playing),
                            position,
                            discontinuity: e.discontinuity,
                        },
                        block.tick.quantum.frames(),
                        &mut block.midi_outputs[0],
                    )?;
                    if let Some(error) = report.output_error {
                        return Err(error);
                    }
                }
                Ok(())
            }
            Self::Capture(p) => {
                let e = state.entries[p.slot].ok_or(Error::InvalidState)?;
                if let (Some(audio), Some(interval)) = (&mut p.audio, e.interval) {
                    audio.stage_ports(&mut block.resources.render(), interval, 0, &block.audio)?;
                }
                if let Some(midi) = &mut p.midi {
                    midi.process(
                        &mut block.resources.render(),
                        &block.midi_inputs[0],
                        block.tick.quantum.frames(),
                        e.interval.map(|i| (i, usize::from(p.binding.channels > 0))),
                    )?;
                    state.inputs[p.slot] = *midi.state();
                }
                Ok(())
            }
        }
    }
}

impl Prepared {
    /// Verify both physical consumers exist before installing the graph.
    pub fn runtime(
        self,
        config: processing::PreparedConfig,
        graph: &GraphDescription,
    ) -> Result<
        (
            CommandControl<Command, Reply>,
            processing::Runtime<Node, Bank>,
        ),
        Error,
    > {
        for (_, node) in &self.nodes {
            let b = match node {
                Node::Playback(p) => p.binding,
                Node::Capture(p) => p.binding,
            };
            if !graph.nodes.contains(&b.playback) || !graph.nodes.contains(&b.capture) {
                return Err(Error::InvalidGraph);
            }
        }
        let runtime =
            processing::Runtime::prepare(config, self.nodes, self.bank, self.resources, graph)?;
        Ok((self.control, runtime))
    }
}
