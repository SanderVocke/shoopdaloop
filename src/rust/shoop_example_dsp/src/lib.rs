//! Prepared stereo live/playback mixing, delay and stable MIDI fan-in.
//! Preset selection is the processor's own typed operation, which the runtime
//! schedules; it is independent of any planner's vocabulary.
#![forbid(unsafe_code)]

mod meters;
mod processor;
mod types;

pub use meters::Meters;
pub use processor::Dsp;
pub use types::{Config, Operation, Preset, PresetId, Receipt, Refusal};

use rt_runtime::Error;

pub struct Prepared {
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
    let meters = Meters::new();
    Ok(Prepared {
        processor: Dsp::new(config, presets, initial_preset, delay, meters.clone()),
        meters,
    })
}
