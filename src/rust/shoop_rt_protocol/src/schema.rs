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
        std::str::from_utf8(&self.bytes[..self.len as usize]).unwrap_or("")
    }
}

impl std::fmt::Debug for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StripPort {
    pub strip: StripId,
    pub kind: PortKind,
    pub index: u8,
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
        port: StripPort,
        host: HostPortId,
        connected: bool,
    },
    Route {
        strip: StripId,
        bus: BusId,
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
    Done,
    Refused(Refusal),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DriverKind {
    Dummy,
    Cpal,
    WebAudio,
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
    pub strips: u32,
    pub loops: u32,
    pub strip_audio_channels: u8,
    pub host_audio_inputs: u16,
    pub host_audio_outputs: u16,
    pub host_midi_inputs: u16,
    pub host_midi_outputs: u16,
    pub max_frames: u32,
    /// Unanswered reliable requests a client may have.
    pub reliable_credits: u16,
    /// Unanswered bulk reads a client may have.
    pub bulk_credits: u16,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Telemetry {
    Engine {
        frame: u64,
        xruns: u32,
        /// Callback time as a share of the callback period.
        load: f32,
        driver: DriverStatus,
    },
    Strip {
        strip: StripId,
        input_peaks: Peaks,
        output_peaks: Peaks,
        midi_in: bool,
        midi_out: bool,
    },
    Loop {
        loop_id: LoopSlotId,
        mode: LoopMode,
        next_mode: Option<LoopMode>,
        next_delay: Option<u32>,
        length: u64,
        position: u64,
        peaks: Peaks,
    },
    Bus {
        bus: BusId,
        peaks: Peaks,
    },
}

/// One record of a full-state resynchronization, sent between `Begin` and `End`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum StateRecord {
    Begin {
        epoch: u64,
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
        port: StripPort,
        host: HostPortId,
    },
    Route {
        strip: StripId,
        bus: BusId,
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
    Audio { channel: u8 },
    Midi,
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
