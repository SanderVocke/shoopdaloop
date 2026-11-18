//! Logical graph descriptions, independent of execution, transport and physical allocation.
#![no_std]
extern crate alloc;
use alloc::{borrow::Cow, vec::Vec};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct Id {
    pub slot: u32,
    pub generation: u32,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Owner {
    Strip(Id),
    Loop(Id),
    Bus(Id),
    Host,
    Global,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum SignalType {
    Audio,
    Midi,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum PortRole {
    Input,
    Output,
    Send,
    Return,
    Control,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct PortId {
    pub owner: Owner,
    pub signal: SignalType,
    pub role: PortRole,
    pub index: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Connection {
    pub source: PortId,
    pub destination: PortId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ChannelLayout {
    Direct {
        audio: u32,
        midi: bool,
    },
    DryWet {
        dry_audio: u32,
        wet_audio: u32,
        dry_midi: bool,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct StripControls {
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub output_muted: bool,
    pub input_monitoring: bool,
    pub automatic_monitoring: bool,
}
impl Default for StripControls {
    fn default() -> Self {
        Self {
            input_gain_db: 0.,
            input_balance: 0.,
            output_gain_db: 0.,
            output_balance: 0.,
            output_muted: false,
            input_monitoring: false,
            automatic_monitoring: false,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ProcessorView<'a> {
    pub type_id: Cow<'a, str>,
    pub parameters: Cow<'a, [f32]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StripView<'a> {
    pub id: Id,
    pub name: Cow<'a, str>,
    pub layout: ChannelLayout,
    pub controls: StripControls,
    pub default_dry_playback: bool,
    pub processor: Option<ProcessorView<'a>>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum LoopMode {
    Stopped,
    Playing,
    Recording,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
    Replacing,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CompositeKind {
    Regular,
    Explicit,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CompositeSpan {
    pub target: Id,
    pub start: u64,
    pub end: u64,
    pub occurrence_start: u64,
    pub ordinal: u32,
    pub mode: LoopMode,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CompositeView<'a> {
    pub kind: CompositeKind,
    pub version: u64,
    pub unit_frames: u64,
    pub iterations: u64,
    pub sample_rate: u32,
    pub quantum: u32,
    pub spans: Cow<'a, [CompositeSpan]>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoopView<'a> {
    pub boundary_smoothing_frames: u32,
    pub id: Id,
    pub strip: Option<Id>,
    pub layout: ChannelLayout,
    pub gain: f32,
    pub balance: f32,
    pub sync_source: Option<Id>,
    pub repeat_sync: bool,
    pub composite: Option<CompositeView<'a>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BusView<'a> {
    pub id: Id,
    pub name: Cow<'a, str>,
    pub channels: u32,
    pub master: bool,
    pub gain_db: f32,
    pub balance: f32,
    pub muted: bool,
    pub processor: Option<ProcessorView<'a>>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PortView<'a> {
    pub id: PortId,
    /// Channel position, independent of the port's stable logical identity.
    pub channel: u32,
    pub name: Cow<'a, str>,
}
impl PortId {
    /// Direction in the signal graph; a host capture port is a source.
    pub fn is_source(self) -> bool {
        matches!((self.owner, self.role), (Owner::Host, PortRole::Input))
            || (!matches!(self.owner, Owner::Host)
                && matches!(self.role, PortRole::Output | PortRole::Send))
    }
}
/// Iterators describe one consistent desired graph; their order is not execution order.
pub trait GraphView {
    fn strips(&self) -> impl Iterator<Item = StripView<'_>> + '_;
    fn loops(&self) -> impl Iterator<Item = LoopView<'_>> + '_;
    fn buses(&self) -> impl Iterator<Item = BusView<'_>> + '_;
    fn ports(&self) -> impl Iterator<Item = PortView<'_>> + '_;
    fn connections(&self) -> impl Iterator<Item = Connection> + '_;
}
/// An immutable transfer snapshot, never a second independently edited graph model.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Graph {
    pub strips: Vec<StripView<'static>>,
    pub loops: Vec<LoopView<'static>>,
    pub buses: Vec<BusView<'static>>,
    pub ports: Vec<PortView<'static>>,
    pub connections: Vec<Connection>,
}
impl ProcessorView<'_> {
    fn owned(self) -> ProcessorView<'static> {
        ProcessorView {
            type_id: Cow::Owned(self.type_id.into_owned()),
            parameters: Cow::Owned(self.parameters.into_owned()),
        }
    }
    fn borrowed(&self) -> ProcessorView<'_> {
        ProcessorView {
            type_id: Cow::Borrowed(&self.type_id),
            parameters: Cow::Borrowed(&self.parameters),
        }
    }
}
impl StripView<'_> {
    fn owned(self) -> StripView<'static> {
        StripView {
            name: Cow::Owned(self.name.into_owned()),
            processor: self.processor.map(ProcessorView::owned),
            ..self
        }
    }
    fn borrowed(&self) -> StripView<'_> {
        StripView {
            name: Cow::Borrowed(&self.name),
            processor: self.processor.as_ref().map(ProcessorView::borrowed),
            id: self.id,
            layout: self.layout,
            controls: self.controls,
            default_dry_playback: self.default_dry_playback,
        }
    }
}
impl LoopView<'_> {
    fn owned(self) -> LoopView<'static> {
        LoopView {
            composite: self.composite.map(|c| CompositeView {
                spans: Cow::Owned(c.spans.into_owned()),
                ..c
            }),
            ..self
        }
    }
    fn borrowed(&self) -> LoopView<'_> {
        LoopView {
            composite: self.composite.as_ref().map(|c| CompositeView {
                spans: Cow::Borrowed(&c.spans),
                kind: c.kind,
                version: c.version,
                unit_frames: c.unit_frames,
                iterations: c.iterations,
                sample_rate: c.sample_rate,
                quantum: c.quantum,
            }),
            id: self.id,
            strip: self.strip,
            layout: self.layout,
            gain: self.gain,
            balance: self.balance,
            sync_source: self.sync_source,
            repeat_sync: self.repeat_sync,
            boundary_smoothing_frames: self.boundary_smoothing_frames,
        }
    }
}
impl BusView<'_> {
    fn owned(self) -> BusView<'static> {
        BusView {
            name: Cow::Owned(self.name.into_owned()),
            processor: self.processor.map(ProcessorView::owned),
            ..self
        }
    }
    fn borrowed(&self) -> BusView<'_> {
        BusView {
            name: Cow::Borrowed(&self.name),
            processor: self.processor.as_ref().map(ProcessorView::borrowed),
            id: self.id,
            channels: self.channels,
            master: self.master,
            gain_db: self.gain_db,
            balance: self.balance,
            muted: self.muted,
        }
    }
}
impl GraphView for Graph {
    fn strips(&self) -> impl Iterator<Item = StripView<'_>> {
        self.strips.iter().map(StripView::borrowed)
    }
    fn loops(&self) -> impl Iterator<Item = LoopView<'_>> {
        self.loops.iter().map(LoopView::borrowed)
    }
    fn buses(&self) -> impl Iterator<Item = BusView<'_>> {
        self.buses.iter().map(BusView::borrowed)
    }
    fn ports(&self) -> impl Iterator<Item = PortView<'_>> {
        self.ports.iter().map(|p| PortView {
            channel: p.channel,
            id: p.id,
            name: Cow::Borrowed(&p.name),
        })
    }
    fn connections(&self) -> impl Iterator<Item = Connection> {
        self.connections.iter().copied()
    }
}
impl Graph {
    pub fn capture(view: &impl GraphView) -> Self {
        Self {
            strips: view.strips().map(StripView::owned).collect(),
            loops: view.loops().map(LoopView::owned).collect(),
            buses: view.buses().map(BusView::owned).collect(),
            ports: view
                .ports()
                .map(|p| PortView {
                    id: p.id,
                    channel: p.channel,
                    name: Cow::Owned(p.name.into_owned()),
                })
                .collect(),
            connections: view.connections().collect(),
        }
    }
    pub fn encode(&self) -> Result<Vec<u8>, bincode::error::EncodeError> {
        bincode::serde::encode_to_vec(self, bincode::config::standard())
    }
    pub fn decode(bytes: &[u8]) -> Result<Self, bincode::error::DecodeError> {
        let (graph, used) = bincode::serde::decode_from_slice(
            bytes,
            bincode::config::standard().with_limit::<{ MAX_TRANSFER_BYTES }>(),
        )?;
        if used != bytes.len() {
            return Err(bincode::error::DecodeError::Other("trailing graph data"));
        }
        Ok(graph)
    }
}
/// Control-to-ARM transfer bound, independent of executable memory dimensions.
pub const MAX_TRANSFER_BYTES: usize = 1024 * 1024;

impl<T: GraphView> GraphView for &T {
    fn strips(&self) -> impl Iterator<Item = StripView<'_>> {
        (**self).strips()
    }
    fn loops(&self) -> impl Iterator<Item = LoopView<'_>> {
        (**self).loops()
    }
    fn buses(&self) -> impl Iterator<Item = BusView<'_>> {
        (**self).buses()
    }
    fn ports(&self) -> impl Iterator<Item = PortView<'_>> {
        (**self).ports()
    }
    fn connections(&self) -> impl Iterator<Item = Connection> {
        (**self).connections()
    }
}
