//! Portable host setup vocabulary; execution and OS configuration belong to hosts.
use crate::{DriverKind, DriverStatus};

pub const HOST_VERSION: u16 = 1;
pub const MAX_HOST_TEXT_BYTES: usize = 256;
pub const MAX_SETUP_ITEMS: usize = 256;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Text {
    bytes: [u8; MAX_HOST_TEXT_BYTES],
    len: u16,
}
impl Text {
    pub fn new(value: &str) -> Option<Self> {
        if value.len() > MAX_HOST_TEXT_BYTES {
            return None;
        }
        let mut bytes = [0; MAX_HOST_TEXT_BYTES];
        bytes[..value.len()].copy_from_slice(value.as_bytes());
        Some(Self {
            bytes,
            len: value.len() as u16,
        })
    }
    pub fn as_str(&self) -> &str {
        core::str::from_utf8(&self.bytes[..self.len as usize]).expect("validated text")
    }
    pub fn clipped(value: &str) -> Self {
        let mut end = value.len().min(MAX_HOST_TEXT_BYTES);
        while !value.is_char_boundary(end) {
            end -= 1;
        }
        Self::new(&value[..end]).expect("bounded text")
    }
}
impl Default for Text {
    fn default() -> Self {
        Self::new("").unwrap()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DiscoverKind {
    Hosts,
    AudioInputs,
    AudioOutputs,
    MidiInputs,
    MidiOutputs,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceSelection {
    Default,
    None,
    Named(Text),
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EngineSettings {
    pub max_frames: u32,
    pub strip_channels: u32,
    pub master_channels: u32,
    pub smoothing_frames: u32,
    pub smoothing_limit_frames: u32,
    pub ramp_frames: u32,
    pub control_lead_frames: u32,
    pub resource_pages: u32,
    pub page_bytes: u32,
    pub midi_events: u32,
    pub midi_bytes: u32,
    pub pending_transitions: u32,
    pub reliable_credits: u32,
    pub bulk_credits: u32,
    pub telemetry_interval_frames: u32,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct DriverSettings {
    pub kind: DriverKind,
    pub host: Text,
    pub input: DeviceSelection,
    pub output: DeviceSelection,
    /// Zero asks the device to choose its default.
    pub sample_rate: u32,
    /// Zero asks the device to choose its buffer size.
    pub buffer_frames: u32,
    pub audio_inputs: u16,
    pub audio_outputs: u16,
    pub midi_inputs: u16,
    pub midi_outputs: u16,
    pub client_name: Text,
    pub dummy_input_ramp: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostRequest {
    Hello {
        version: u16,
    },
    Discover {
        kind: DiscoverKind,
        index: u16,
        host: Text,
    },
    ConfigureEngine(EngineSettings),
    AddSegment {
        strips: u32,
        loops: u32,
    },
    ConfigureDriver(DriverSettings),
    AddMidiInput(Text),
    AddMidiOutput(Text),
    Start,
    Stop,
    Status,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HostReply {
    Hello {
        version: u16,
    },
    Ok,
    Device {
        kind: DiscoverKind,
        index: u16,
        name: Text,
        default: bool,
        channels: u16,
        sample_rate: u32,
        min_buffer: u32,
        max_buffer: u32,
    },
    End,
    Ready(DriverStatus),
    Warning(Text),
    Error(Text),
    Stopped {
        midi_reset: bool,
    },
    Status {
        frame: u64,
        callbacks: u64,
    },
}
