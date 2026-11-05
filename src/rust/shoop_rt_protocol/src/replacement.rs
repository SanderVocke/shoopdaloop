//! A private candidate graph, separate from the engine's confirmed live model.

use crate::{
    BusControl, BusPort, EnginePort, HostPortId, LoopSlotId, StripId, StripPort, StripShape,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StripPreset {
    pub input_gain: f32,
    pub input_balance: f32,
    pub monitoring: bool,
    pub output_gain: f32,
    pub output_balance: f32,
    pub output_mute: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ReplacementEdit {
    Bus {
        bus: crate::BusId,
        channels: u8,
    },
    BusControl {
        bus: crate::BusId,
        control: BusControl,
    },
    Strip {
        strip: StripId,
        shape: StripShape,
        controls: StripPreset,
    },
    ProcessedStrip {
        strip: StripId,
        shape: StripShape,
        controls: StripPreset,
        kind: crate::ProcessorKind,
        default_dry: bool,
        state: Option<crate::processors::Control>,
    },
    Loop {
        loop_id: LoopSlotId,
        strip: StripId,
        sync: Option<LoopSlotId>,
        frames: u64,
        dry_audio: Option<u64>,
        recorded_state: Option<crate::RecordedProcessorState>,
        audio: Option<u64>,
        midi: Option<u64>,
        gain: f32,
        balance: f32,
    },
    Connect {
        port: EnginePort,
        host: HostPortId,
    },
    Route {
        port: StripPort,
        bus: BusPort,
    },
}
