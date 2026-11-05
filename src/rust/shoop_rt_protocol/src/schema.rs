//! Engine-side vocabulary: requests, outcomes, telemetry, state, bulk reads and the
//! driver. Every value has a bounded size so it can be encoded without allocation.

use crate::ids::{BusId, HostPortId, LoopSlotId, StripId};
use crate::{
    BULK_CHUNK_BYTES, BULK_CHUNK_SAMPLES, MAX_MIDI_BYTES, MAX_NAME_BYTES, MAX_STRIP_CHANNELS,
};

/// A bounded UTF-8 name.
#[derive(Clone, Copy, Eq, PartialEq)]
pub struct Name {
    len: u8,
    bytes: [u8; MAX_NAME_BYTES],
}

impl Name {
    pub fn new(text: &str) -> Option<Self> {
        let source = text.as_bytes();
        if source.len() > MAX_NAME_BYTES {
            return None;
        }
        let mut bytes = [0; MAX_NAME_BYTES];
        bytes[..source.len()].copy_from_slice(source);
        Some(Self {
            len: source.len() as u8,
            bytes,
        })
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl core::fmt::Debug for Name {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        self.as_str().fmt(f)
    }
}

/// A short inline MIDI message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MidiMessage {
    len: u8,
    bytes: [u8; MAX_MIDI_BYTES],
}

impl MidiMessage {
    pub fn new(source: &[u8]) -> Option<Self> {
        if source.is_empty() || source.len() > MAX_MIDI_BYTES {
            return None;
        }
        let mut bytes = [0; MAX_MIDI_BYTES];
        bytes[..source.len()].copy_from_slice(source);
        Some(Self {
            len: source.len() as u8,
            bytes,
        })
    }
    pub fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.len as usize]
    }
}

/// Per-channel peak levels since the previous telemetry record.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Peaks {
    len: u8,
    values: [f32; MAX_STRIP_CHANNELS],
}

impl Peaks {
    pub fn new(source: &[f32]) -> Option<Self> {
        if source.len() > MAX_STRIP_CHANNELS {
            return None;
        }
        let mut values = [0.; MAX_STRIP_CHANNELS];
        values[..source.len()].copy_from_slice(source);
        Some(Self {
            len: source.len() as u8,
            values,
        })
    }
    pub fn as_slice(&self) -> &[f32] {
        &self.values[..self.len as usize]
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoopMode {
    Stopped,
    Playing,
    Recording,
    PlayingDryThroughWet,
    RecordingDryIntoWet,
}
impl LoopMode {
    pub fn records(self) -> bool {
        matches!(self, Self::Recording | Self::RecordingDryIntoWet)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StripControl {
    /// Linear input gain.
    InputGain(f32),
    InputBalance(f32),
    Monitoring(bool),
    /// Linear output gain.
    OutputGain(f32),
    OutputBalance(f32),
    OutputMute(bool),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BusControl {
    /// Linear gain.
    Gain(f32),
    Balance(f32),
    Mute(bool),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortKind {
    AudioIn,
    AudioOut,
    MidiIn,
    MidiOut,
    AudioSend,
    AudioReturn,
    MidiSend,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StripPort {
    pub strip: StripId,
    pub kind: PortKind,
    pub index: u8,
}

/// An audio channel of a bus: an output channel when connected to host ports,
/// an input channel when routed to.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BusPort {
    pub bus: BusId,
    pub index: u8,
}

/// An engine port that can be connected to host ports.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EnginePort {
    GlobalFxControl,
    Strip(StripPort),
    Bus(BusPort),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StripShape {
    pub audio_inputs: u8,
    pub audio_outputs: u8,
    pub midi_input: bool,
    pub midi_output: bool,
}

/// A reliable request. The client chooses slot IDs; the engine refuses an ID whose
/// slot is occupied, unprepared or not newer than its last generation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Request {
    ProcessorControl {
        strip: StripId,
        control: crate::processors::Control,
    },
    CreateProcessedStrip {
        strip: StripId,
        shape: StripShape,
        kind: crate::ProcessorKind,
    },
    ContentFamilyBegin {
        id: u64,
        root: LoopSlotId,
        count: u32,
        sample_rate: u32,
        quantum: u32,
    },
    ContentFamilyEdit {
        id: u64,
        edit: crate::ContentFamilyEdit,
    },
    ContentFamilyCommit {
        id: u64,
    },
    ContentFamilyCancel {
        id: u64,
    },
    PreviewStart {
        request_id: u64,
        bus: BusId,
        content: u64,
        frames: u64,
    },
    PreviewStop,
    CreateBus {
        bus: BusId,
        channels: u8,
    },
    RemoveBus {
        bus: BusId,
    },
    CancelBulk,
    ReplacementBegin {
        id: u64,
    },
    ReplacementEdit {
        id: u64,
        edit: crate::ReplacementEdit,
    },
    ReplacementPrepare {
        id: u64,
    },
    ReplacementCommit {
        id: u64,
    },
    ReplacementCancel {
        id: u64,
    },
    ContentBegin(crate::ContentDescription),
    ContentWrite {
        id: u64,
        offset: u64,
        data: crate::ContentChunk,
    },
    ContentFinish {
        id: u64,
    },
    ContentInitial {
        id: u64,
        message: MidiMessage,
    },
    ContentApply {
        loop_id: LoopSlotId,
        frames: u64,
        audio: Option<u64>,
        midi: Option<u64>,
    },
    ContentRelease {
        id: u64,
    },
    CreateStrip {
        strip: StripId,
        shape: StripShape,
    },
    RemoveStrip {
        strip: StripId,
    },
    StripControl {
        strip: StripId,
        control: StripControl,
    },
    StripDefaultPlayback {
        strip: StripId,
        dry_through_wet: bool,
    },
    CreateLoop {
        loop_id: LoopSlotId,
        strip: StripId,
    },
    RemoveLoop {
        loop_id: LoopSlotId,
    },
    LoopSync {
        loop_id: LoopSlotId,
        source: Option<LoopSlotId>,
    },
    /// `delay: Some(n)` applies at the (n + 1)-th pulse of the loop's sync source;
    /// a nearer planned transition supersedes later ones.
    LoopTransition {
        loop_id: LoopSlotId,
        mode: LoopMode,
        delay: Option<u32>,
    },
    LoopClear {
        loop_id: LoopSlotId,
    },
    LoopGain {
        loop_id: LoopSlotId,
        gain: f32,
    },
    LoopBalance {
        loop_id: LoopSlotId,
        balance: f32,
    },
    Connect {
        port: EnginePort,
        host: HostPortId,
        connected: bool,
    },
    /// Route a strip audio output channel (`port.kind` is `AudioOut`) to a bus
    /// channel.
    Route {
        port: StripPort,
        bus: BusPort,
        connected: bool,
    },
    BusControl {
        bus: BusId,
        control: BusControl,
    },
    InjectMidi {
        strip: StripId,
        message: MidiMessage,
    },
    SetSmoothing {
        frames: u32,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    Unsupported,
    Capacity,
    InvalidId,
    InvalidValue,
    Busy,
    Overloaded,
    NotConnected,
    Internal,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Outcome {
    ContentPrepared { revision: u64 },
    ContentApplied { revision: u64 },
    Done,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverKind {
    Dummy,
    Cpal,
    WebAudio,
    Jack,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverRunState {
    Starting,
    Running,
    Stopped,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverStatus {
    pub kind: DriverKind,
    pub state: DriverRunState,
    pub sample_rate: u32,
    pub buffer_frames: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostPortKind {
    Audio,
    Midi,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    /// Signal flows from the host into the engine.
    Input,
    /// Signal flows from the engine to the host.
    Output,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HostPort {
    pub id: HostPortId,
    pub kind: HostPortKind,
    pub direction: Direction,
    pub name: Name,
    /// Preferred default playback destination.
    pub preferred: bool,
}

/// Capability flags announced by the engine.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Capabilities(pub u32);

impl Capabilities {
    pub const MIDI_INJECTION: Self = Self(1);
    pub const BULK_MIDI: Self = Self(1 << 1);
    pub const GROWABLE_LIMITS: Self = Self(1 << 2);
    pub const CONTENT_UPLOAD: Self = Self(1 << 3);
    pub const COMPLETE_REPLACEMENT: Self = Self(1 << 4);
    /// Accept sixty-four queued upload records through thirty-two reusable buffers.
    pub const WIDE_CONTENT_UPLOAD: Self = Self(1 << 5);
    pub const DRY_WET: Self = Self(1 << 6);
    pub const TRACK_PROCESSORS: Self = Self(1 << 7);
    pub const GLOBAL_FX_CONTROL: Self = Self(1 << 8);
    pub const EXTERNAL_DRY_WET: Self = Self(1 << 9);

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

/// Current capacities. They may change during a connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Limits {
    /// Maximum content-upload bytes per request, limited by the prepared page size.
    pub content_chunk_bytes: u16,
    pub strips: u32,
    pub loops: u32,
    pub strip_audio_channels: u8,
    pub master_channels: u8,
    pub buses: u8,
    pub bus_channels: u8,
    pub host_audio_inputs: u16,
    pub host_audio_outputs: u16,
    pub host_midi_inputs: u16,
    pub host_midi_outputs: u16,
    pub max_frames: u32,
    /// Prepared graph tick, independent of host capacity.
    pub quantum: u32,
    /// Constant host stream delay introduced by tick buffering.
    pub processing_latency_frames: u32,
    /// Largest loop boundary smoothing a client may set.
    pub smoothing_limit_frames: u32,
    /// Unanswered reliable requests a client may have.
    pub reliable_credits: u16,
    /// Unanswered bulk reads a client may have.
    pub bulk_credits: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Telemetry {
    Preview {
        request_id: u64,
    },
    Engine {
        frame: u64,
        /// Callbacks rendered since preparation.
        callbacks: u64,
        xruns: u32,
        /// Callback time as a share of the callback period.
        load: f32,
        /// Largest absolute sample on any host audio input and output since the
        /// previous engine record.
        input_peak: f32,
        output_peak: f32,
        driver: DriverStatus,
    },
    Strip {
        strip: StripId,
        input_peaks: Peaks,
        output_peaks: Peaks,
        midi_in: bool,
        midi_out: bool,
        latest_input_midi: Option<MidiMessage>,
    },
    Loop {
        loop_id: LoopSlotId,
        mode: LoopMode,
        next_mode: Option<LoopMode>,
        next_delay: Option<u32>,
        content_revision: u64,
        length: u64,
        position: u64,
        peaks: Peaks,
    },
    Bus {
        bus: BusId,
        peaks: Peaks,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorSlot {
    pub index: u16,
    pub generation: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorPreset {
    pub bank: u16,
    pub program: u8,
    pub name: Name,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessorLimits {
    pub effective: crate::processors::Effective,
    pub reserved_bytes: u64,
    pub channels: u8,
    pub builtin_latency_frames: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RecordedProcessorState {
    /// First source sample of the take, after controls and MIDI at this offset.
    pub source_frame: u64,
    pub kind: crate::ProcessorKind,
    pub values: [f32; crate::processors::STATE_SCALARS],
}

/// One record of a full-state resynchronization, sent between `Begin` and `End`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StateRecord {
    RecordedProcessorState {
        loop_id: LoopSlotId,
        content_revision: u64,
        state: Option<RecordedProcessorState>,
    },
    ProcessorState {
        strip: StripId,
        slot: ProcessorSlot,
        snapshot: crate::processors::Snapshot,
    },
    StripDefaultPlayback {
        strip: StripId,
        dry_through_wet: bool,
    },
    StripProcessor {
        strip: StripId,
        kind: crate::ProcessorKind,
        slot: Option<ProcessorSlot>,
    },
    Processors(ProcessorLimits),
    ProcessorPreset(ProcessorPreset),
    BusShape {
        bus: BusId,
        channels: u8,
    },
    BusGenerations {
        generation: u32,
    },
    Begin {
        epoch: u64,
    },
    /// Every slot generation used so far is at most these floors; a client must
    /// name a slot with a newer generation.
    Generations {
        strips: u32,
        loops: u32,
    },
    HostPort(HostPort),
    Strip {
        strip: StripId,
        shape: StripShape,
    },
    StripControl {
        strip: StripId,
        control: StripControl,
    },
    Loop {
        loop_id: LoopSlotId,
        strip: StripId,
        sync: Option<LoopSlotId>,
        gain: f32,
        balance: f32,
    },
    Connection {
        port: EnginePort,
        host: HostPortId,
    },
    Route {
        port: StripPort,
        bus: BusPort,
    },
    Bus {
        bus: BusId,
        control: BusControl,
    },
    End {
        epoch: u64,
    },
}

/// MIDI chunks carry the recorded records, each a little-endian `u64` frame from
/// the start of the media, a little-endian `u32` length and the message bytes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BulkSelector {
    Audio {
        channel: u8,
    },
    /// Every audio channel of the loop, interleaved by frame. Offsets and totals
    /// count frames; a chunk carries whole frames.
    AudioFrames,
    Midi,
    /// Length-prefixed short packets restoring one channel's initial projection.
    MidiInitial {
        channel: u8,
    },
}

/// Read one chunk of a loop's current media, starting at `offset` (frames for
/// audio, bytes for MIDI).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BulkRead {
    pub id: u32,
    pub loop_id: LoopSlotId,
    pub selector: BulkSelector,
    pub offset: u64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BulkPayload {
    Audio {
        len: u16,
        samples: [f32; BULK_CHUNK_SAMPLES],
    },
    Midi {
        len: u16,
        bytes: [u8; BULK_CHUNK_BYTES],
    },
    Refused(Refusal),
}

impl BulkPayload {
    pub fn audio(samples: &[f32]) -> Option<Self> {
        if samples.len() > BULK_CHUNK_SAMPLES {
            return None;
        }
        let mut chunk = [0.; BULK_CHUNK_SAMPLES];
        chunk[..samples.len()].copy_from_slice(samples);
        Some(Self::Audio {
            len: samples.len() as u16,
            samples: chunk,
        })
    }
    pub fn midi(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > BULK_CHUNK_BYTES {
            return None;
        }
        let mut chunk = [0; BULK_CHUNK_BYTES];
        chunk[..bytes.len()].copy_from_slice(bytes);
        Some(Self::Midi {
            len: bytes.len() as u16,
            bytes: chunk,
        })
    }
}

/// The answer to one [`BulkRead`]. `total` is the media's full length in the
/// selector's unit; reading ends when `offset + len >= total`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BulkChunk {
    pub id: u32,
    pub offset: u64,
    pub total: u64,
    pub payload: BulkPayload,
}
