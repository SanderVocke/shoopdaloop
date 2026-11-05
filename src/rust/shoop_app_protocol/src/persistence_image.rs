//! Core application state and immutable media, independent of file formats and engines.
use alloc::string::String;
use alloc::vec::Vec;

use alloc::sync::Arc;
use serde::{Deserialize, Serialize};
use shoop_app_api::*;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Controls {
    pub input_gain_db: f32,
    pub input_balance: f32,
    pub monitoring: bool,
    pub output_gain_db: f32,
    pub output_balance: f32,
    pub muted: bool,
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum ChannelMode {
    #[default]
    Direct,
    Dry,
    Wet,
}
impl ChannelMode {
    pub fn role(self) -> &'static str {
        match self {
            Self::Direct => "direct",
            Self::Dry => "dry",
            Self::Wet => "wet",
        }
    }
    pub fn label(self, index: usize) -> String {
        let role = match self {
            Self::Direct => "Audio",
            Self::Dry => "Dry",
            Self::Wet => "Wet",
        };
        alloc::format!("{role} {}", index + 1)
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Channel {
    #[serde(default)]
    pub mode: ChannelMode,
    pub object: Option<u32>,
    pub frames: u64,
    pub gain: f32,
    pub start_offset: i64,
    pub capture_alignment: i64,
    pub preplay: u64,
    pub recording_started_at: Option<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MidiChannel {
    pub channel: Channel,
    pub initial: Vec<Vec<u8>>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Loop {
    #[serde(default)]
    pub recorded_processor_state: Option<String>,
    pub id: LoopId,
    pub name: String,
    pub sync: bool,
    #[serde(default)]
    pub sync_source: Option<LoopId>,
    #[serde(default)]
    pub repeat_sync: bool,
    #[serde(default)]
    pub composite: Option<crate::composite::Content>,
    pub frames: u64,
    pub gain: f32,
    pub balance: f32,
    pub audio: Vec<Channel>,
    pub midi: Vec<MidiChannel>,
}
impl Loop {
    pub fn audio_labels(&self) -> Vec<String> {
        let mut counts = [0; 3];
        self.audio
            .iter()
            .map(|channel| {
                let index = match channel.mode {
                    ChannelMode::Direct => 0,
                    ChannelMode::Dry => 1,
                    ChannelMode::Wet => 2,
                };
                let label = channel.mode.label(counts[index]);
                counts[index] += 1;
                label
            })
            .collect()
    }
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Port {
    pub state: ApplicationPortState,
    pub external_connections: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Track {
    #[serde(default)]
    pub topology: TrackTopology,
    #[serde(default)]
    pub default_playback_mode: DefaultPlaybackMode,
    #[serde(default)]
    pub processor_state: Option<String>,
    #[serde(default)]
    pub builtin_fx_midi_cc_assignments: Vec<BuiltInFxMidiCcAssignment>,
    #[serde(default)]
    pub oxisynth_midi_cc_assignments: Vec<OxiSynthMidiCcAssignment>,
    pub id: TrackId,
    pub name: String,
    pub port_name_base: String,
    pub sync: bool,
    pub audio_channels: u32,
    pub midi: bool,
    pub controls: Controls,
    pub ports: Vec<Port>,
    pub loops: Vec<Loop>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct BusChannel {
    pub id: BusChannelId,
    pub label: String,
    pub port: Port,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Bus {
    pub id: BusId,
    pub name: String,
    pub gain_db: f32,
    pub balance: f32,
    pub muted: bool,
    pub channels: Vec<BusChannel>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Session {
    pub sample_rate: u32,
    pub global: GlobalControlState,
    pub tracks: Vec<Track>,
    pub buses: Vec<Bus>,
    pub bus_display_order: Vec<BusId>,
    pub preview_bus: Option<BusId>,
    pub global_ports: Vec<Port>,
    pub routes: Vec<MixerRouteState>,
    pub selected: Vec<LoopId>,
    pub target: Option<LoopId>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct AudioUpdate {
    pub channel: u32,
    pub content: Channel,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MidiUpdate {
    pub channel: u32,
    pub content: MidiChannel,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FamilyMember {
    pub loop_id: LoopId,
    pub sync_source: Option<LoopId>,
    pub expected_revision: u64,
    pub expected_frames: u64,
    pub fence_content: bool,
    pub restart: bool,
    pub content: Option<Loop>,
}
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum Manifest {
    Session(Session),
    Loop {
        sample_rate: u32,
        content: Loop,
    },
    Update {
        sample_rate: u32,
        frames: Option<u64>,
        audio: Vec<AudioUpdate>,
        midi: Vec<MidiUpdate>,
    },
    FamilyUpdate {
        format: ConversionTarget,
        root: LoopId,
        members: Vec<FamilyMember>,
    },
}
#[derive(Clone, Debug, PartialEq)]
pub enum Payload {
    Audio(Arc<Vec<f32>>),
    Midi(Arc<Vec<MidiEventState>>),
}
impl Payload {
    pub fn bytes(&self) -> Option<u64> {
        match self {
            Self::Audio(samples) => (samples.len() as u64).checked_mul(4),
            Self::Midi(events) => events
                .iter()
                .try_fold(0u64, |n, event| n.checked_add(12 + event.data.len() as u64)),
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct Image {
    pub manifest: Manifest,
    /// Object zero is metadata; these payloads have indices starting at one.
    pub payloads: Vec<Payload>,
}
mod validation;
pub use validation::{ValidatedImage, Validation};
