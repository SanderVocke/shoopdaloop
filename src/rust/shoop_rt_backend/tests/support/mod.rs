#![allow(dead_code)] // Shared support is compiled independently by each integration test.

use std::time::Duration;

use anyhow::Result;
use shoop_backend::contracts::{timing_ramp, ContractProfile, TimingHarness};
use shoop_backend::Backend;
use shoop_rt_backend::{EngineClock, RtBackend};
use shoop_rt_engine::testing::{OfflineClient, OfflineRender};
use shoop_rt_engine::{ControlEndpoint, EngineDescription, HostPorts, Segment};
use shoop_rt_protocol::carrier::InProcess;

/// The adapter over the in-process carrier, the engine and the offline host.
pub type Stack = RtBackend<InProcess<ControlEndpoint>, OfflineClock>;

/// 1 kHz with small callbacks; telemetry after every callback.
pub fn description() -> EngineDescription {
    EngineDescription {
        sample_rate: 1000,
        max_frames: 16,
        host: HostPorts::numbered(2, 2, 1),
        segments: vec![Segment {
            strips: 16,
            loops: 48,
        }],
        strip_channels: 2,
        master_channels: 2,
        master_connections: Vec::new(),
        ramp_frames: 4,
        resource_pages: 2048,
        page_bytes: 256,
        midi_events: 64,
        midi_bytes: 512,
        pending_transitions: 64,
        reliable_credits: 32,
        bulk_credits: 2,
        telemetry_interval_frames: 0,
        ..EngineDescription::default()
    }
}

/// The offline host's port names.
pub fn profile() -> ContractProfile {
    ContractProfile {
        audio_capture_port: "audio_in_1".to_owned(),
        audio_playback_port: "audio_out_1".to_owned(),
        midi_capture_port: Some("midi_in_1".to_owned()),
        midi_playback_port: Some("midi_out_1".to_owned()),
    }
}

/// Renders when the adapter lets time pass or waits; waiting renders `block` frames.
pub struct OfflineClock {
    pub offline: OfflineRender,
    /// The signal on host audio input 0 at an absolute frame; other inputs are silent.
    pub capture: fn(u64) -> f32,
    pub block: usize,
}

fn silence(_: u64) -> f32 {
    0.0
}

impl OfflineClock {
    pub fn render(&mut self, frames: usize) {
        let capture = self.capture;
        self.offline
            .run(
                frames,
                |port, frame| {
                    if port == 0 {
                        capture(frame)
                    } else {
                        0.0
                    }
                },
            )
            .expect("offline rendering failed");
    }

    pub fn frame(&self) -> u64 {
        self.offline.next_frame()
    }
}

impl EngineClock for OfflineClock {
    fn advance(&mut self, elapsed: Duration) {
        let capture = self.capture;
        self.offline
            .run_for(
                elapsed,
                |port, frame| {
                    if port == 0 {
                        capture(frame)
                    } else {
                        0.0
                    }
                },
            )
            .expect("offline rendering failed");
    }

    fn wait(&mut self) -> bool {
        self.render(self.block);
        true
    }
}

pub fn parts(description: &EngineDescription) -> (OfflineClock, OfflineClient) {
    let (offline, client) = OfflineRender::new(description).expect("engine preparation");
    (
        OfflineClock {
            offline,
            capture: silence,
            block: description.max_frames,
        },
        client,
    )
}

pub fn stack(description: &EngineDescription) -> Stack {
    let (clock, client) = parts(description);
    RtBackend::connect(client, clock).expect("connection")
}

/// The stack at 1 kHz, one frame per step, with the timing ramp on the capture input.
pub struct RtTimingHarness {
    backend: Stack,
    profile: ContractProfile,
}

impl RtTimingHarness {
    pub fn new() -> Self {
        let (mut clock, client) = parts(&description());
        clock.capture = timing_ramp;
        clock.block = 1;
        Self {
            backend: RtBackend::connect(client, clock).expect("connection"),
            profile: profile(),
        }
    }
}

impl TimingHarness for RtTimingHarness {
    fn backend(&mut self) -> &mut dyn Backend {
        &mut self.backend
    }

    fn profile(&self) -> &ContractProfile {
        &self.profile
    }

    fn frame(&self) -> u64 {
        self.backend.clock().frame()
    }

    fn step(&mut self) -> Result<()> {
        self.backend.pump();
        self.backend.clock_mut().render(1);
        Ok(())
    }

    /// The engine reports loop state at the start of the last rendered callback.
    fn observation_lag(&self) -> u64 {
        1
    }
}
