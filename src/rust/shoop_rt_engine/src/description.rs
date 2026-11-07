//! What an engine is prepared with: host ports, pools, signal settings and
//! control capacities.

use alloc::format;
use alloc::vec;
use alloc::vec::Vec;

use shoop_rt_protocol::{Name, MAX_STRIP_CHANNELS};

/// Most host ports of one kind and direction.
pub const MAX_HOST_PORTS: usize = 1024;

/// A pool segment: a number of prepared strip and loop slots. Slot indices count
/// through the segments in order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Segment {
    pub strips: u32,
    pub loops: u32,
}

/// Names of the host's fixed port slots.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HostPorts {
    pub audio_inputs: Vec<Name>,
    pub audio_outputs: Vec<Name>,
    pub midi_inputs: Vec<Name>,
    pub midi_outputs: Vec<Name>,
}

impl HostPorts {
    /// Numbered ports such as `audio_in_1`.
    pub fn numbered(audio_inputs: usize, audio_outputs: usize, midi: usize) -> Self {
        let names = |prefix: &str, count: usize| {
            (0..count)
                .map(|index| Name::new(&format!("{prefix}_{}", index + 1)).expect("short name"))
                .collect()
        };
        Self {
            audio_inputs: names("audio_in", audio_inputs),
            audio_outputs: names("audio_out", audio_outputs),
            midi_inputs: names("midi_in", midi),
            midi_outputs: names("midi_out", midi),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct EngineDescription {
    pub audio_render_mode: rt_runtime::AudioRenderMode,
    pub audio_bench_only: bool,
    pub history: shoop_rt_protocol::host::InputHistorySettings,
    pub composite_entries: usize,
    pub composite_segments: usize,
    pub timing_relations: usize,
    pub sample_rate: u32,
    pub processors: shoop_rt_protocol::ProcessorSettings,
    /// Prepare External dry/wet routing and its bounded wet edit storage.
    pub external_dry_wet: bool,
    /// Largest callback the host will render.
    pub max_frames: usize,
    /// Fixed graph tick. Changing it requires preparing a new engine.
    /// One frame provides sample-accurate fixed execution for offline/reference use.
    pub quantum: usize,
    pub host: HostPorts,
    pub segments: Vec<Segment>,
    /// Audio channels per strip direction; also the channels of every loop.
    pub strip_channels: u8,
    pub master_channels: u8,
    pub buses: u8,
    pub bus_channels: u8,
    /// Initial Master connections as (Master channel, host audio output index).
    /// They are part of the engine state and can be changed like any connection.
    pub master_connections: Vec<(u8, u16)>,
    /// Initial loop boundary smoothing; zero is none.
    pub smoothing_frames: u32,
    /// Largest loop boundary smoothing a client may set.
    pub smoothing_limit_frames: u32,
    /// Ramp for control changes and for paths that are removed or rerouted.
    pub ramp_frames: u32,
    /// Distance between the render's next frame and a scheduled structural
    /// change. Hosts whose control side runs concurrently with rendering use
    /// at least one callback so ramps start on time.
    pub control_lead_frames: u32,
    pub resource_pages: usize,
    pub page_bytes: usize,
    /// Admitted live-input MIDI capacity per fixed tick; output storage is derived.
    pub midi_events: usize,
    pub midi_bytes: usize,
    /// Loop transitions that may be pending at once, including delayed ones.
    pub pending_transitions: usize,
    pub reliable_credits: u16,
    pub bulk_credits: u16,
    /// Minimum frames between periodic telemetry records; zero sends them after
    /// every callback.
    pub telemetry_interval_frames: u32,
}

impl Default for EngineDescription {
    fn default() -> Self {
        Self {
            audio_render_mode: rt_runtime::AudioRenderMode::Current,
            audio_bench_only: false,
            history: shoop_rt_protocol::host::InputHistorySettings::default(),
            external_dry_wet: false,
            composite_entries: 256,
            composite_segments: 1024,
            timing_relations: 512,
            sample_rate: 48_000,
            processors: shoop_rt_protocol::ProcessorSettings::disabled(),
            max_frames: 512,
            quantum: 32,
            host: HostPorts::numbered(2, 2, 1),
            segments: vec![Segment {
                strips: 8,
                loops: 32,
            }],
            strip_channels: 2,
            master_channels: 2,
            buses: 8,
            bus_channels: 8,
            master_connections: vec![(0, 0), (1, 1)],
            smoothing_frames: 0,
            smoothing_limit_frames: 4800,
            ramp_frames: 240,
            control_lead_frames: 0,
            resource_pages: 4096,
            page_bytes: 4096,
            midi_events: 256,
            midi_bytes: 2048,
            pending_transitions: 128,
            reliable_credits: 256,
            bulk_credits: 8,
            telemetry_interval_frames: 800,
        }
    }
}

impl EngineDescription {
    pub fn strips(&self) -> usize {
        self.segments.iter().map(|s| s.strips as usize).sum()
    }
    pub fn loops(&self) -> usize {
        self.segments.iter().map(|s| s.loops as usize).sum()
    }

    pub(crate) fn validate(&self) -> Result<(), rt_runtime::Error> {
        self.processors
            .validate()
            .map_err(|_| rt_runtime::Error::InvalidConfig)?;
        use rt_runtime::Error;
        let host = &self.host;
        rt_runtime::Quantum::new(self.quantum)?;
        if self.composite_entries == 0
            || self.composite_segments == 0
            || self.timing_relations == 0
            || self.sample_rate == 0
            || self.max_frames == 0
            || self.strip_channels == 0
            || self.master_channels == 0
            || self.buses == 0
            || self.buses as usize > shoop_rt_protocol::MAX_BUSES
            || self.bus_channels == 0
            || self.bus_channels as usize > MAX_STRIP_CHANNELS
            || self.master_channels > self.bus_channels
            || self.segments.is_empty()
            || self.page_bytes == 0
            || self.page_bytes % 4 != 0
            || self.resource_pages == 0
            || self.midi_events == 0
            || self.midi_bytes < 3
            || self.pending_transitions == 0
            || self.reliable_credits == 0
            || self.bulk_credits == 0
            || self.smoothing_frames > self.smoothing_limit_frames
            || self.master_connections.iter().any(|&(channel, host)| {
                channel >= self.master_channels || host as usize >= self.host.audio_outputs.len()
            })
        {
            return Err(Error::InvalidConfig);
        }
        if [
            self.composite_entries,
            self.composite_segments,
            self.timing_relations,
        ]
        .iter()
        .any(|&n| n > u32::MAX as usize)
            || self.strip_channels as usize > MAX_STRIP_CHANNELS
            || self.master_channels as usize > MAX_STRIP_CHANNELS
            || [
                host.audio_inputs.len(),
                host.audio_outputs.len(),
                host.midi_inputs.len(),
                host.midi_outputs.len(),
            ]
            .iter()
            .any(|&count| count > MAX_HOST_PORTS)
            || self.strips() > u16::MAX as usize
            || self.loops() > u16::MAX as usize
            || self.max_frames > u32::MAX as usize
        {
            return Err(Error::Capacity);
        }
        crate::midi_layout::Layout::new(self)?;
        Ok(())
    }
}
