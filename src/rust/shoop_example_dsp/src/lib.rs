//! Prepared stereo live/playback mixing, delay and stable MIDI fan-in.
//! Component control is independent of the runtime's planner vocabulary.
#![forbid(unsafe_code)]

mod control;
mod meters;
mod processor;
mod types;

pub use control::{Client, Completion, RenderControl, Ticket};
pub use meters::Meters;
pub use processor::Dsp;
pub use types::{Config, Operation, Preset, PresetId, Receipt, Refusal, Request};

use rt_runtime::Error;
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicU64},
    Arc,
};

pub struct Prepared {
    pub client: Client,
    pub render: RenderControl,
    pub processor: Dsp,
    pub meters: Meters,
}

pub fn prepare(config: Config, presets: Vec<Preset>, initial: PresetId) -> Result<Prepared, Error> {
    let delay_slots = config.validate(&presets, initial)?;
    let initial_preset = presets[initial.slot() as usize];
    let mut delay = vec![[0.; 2]; delay_slots].into_boxed_slice();
    for frame in &mut delay {
        *std::hint::black_box(frame) = [0.; 2];
    }
    let mut presets = presets.into_boxed_slice();
    for preset in &mut presets {
        let value = *preset;
        *std::hint::black_box(preset) = value;
    }
    let catalog = presets
        .iter()
        .map(|preset| preset.id)
        .collect::<Vec<_>>()
        .into_boxed_slice();
    let mailbox = Arc::new(control::Mailbox {
        armed: AtomicBool::new(false),
        frame: AtomicU64::new(0),
        slot: AtomicU32::new(initial.slot()),
        revision: AtomicU32::new(1),
        acknowledged: AtomicBool::new(false),
        actual_frame: AtomicU64::new(0),
        previous_revision: AtomicU32::new(1),
        actual_revision: AtomicU32::new(1),
    });
    let meters = Meters::new();
    let (client, render) = control::prepare(config, catalog, Arc::clone(&mailbox))?;
    Ok(Prepared {
        client,
        render,
        processor: Dsp::new(
            config,
            mailbox,
            presets,
            initial_preset,
            delay,
            meters.clone(),
        ),
        meters,
    })
}
