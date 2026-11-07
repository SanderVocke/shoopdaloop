use rt_runtime::{processor::ProcessorOperation, Error, Frame};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresetId {
    slot: u32,
    generation: u32,
}

impl PresetId {
    pub fn from_parts(slot: u32, generation: u32) -> Result<Self, Error> {
        if generation == 0 {
            return Err(Error::InvalidId);
        }
        Ok(Self { slot, generation })
    }
    pub fn slot(self) -> u32 {
        self.slot
    }
    pub fn generation(self) -> u32 {
        self.generation
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Preset {
    pub id: PresetId,
    pub live_gain: [f32; 2],
    pub playback_gain: [f32; 2],
    pub swap_channels: bool,
    pub delay_frames: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Config {
    pub max_frames: usize,
    pub max_delay_frames: usize,
    pub max_presets: usize,
    pub midi_events_per_input: usize,
    pub packet_bytes: usize,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_frames: 64,
            max_delay_frames: 64,
            max_presets: 4,
            midi_events_per_input: 2048,
            packet_bytes: 512,
        }
    }
}

impl Config {
    pub(crate) fn validate(self, presets: &[Preset], initial: PresetId) -> Result<usize, Error> {
        if self.max_frames == 0
            || self.max_presets == 0
            || self.midi_events_per_input == 0
            || self.packet_bytes == 0
            || presets.is_empty()
        {
            return Err(Error::InvalidConfig);
        }
        let ceiling = i32::MAX as usize;
        let delay_slots = self
            .max_delay_frames
            .checked_add(1)
            .ok_or(Error::Capacity)?;
        if self.max_frames > ceiling
            || self.packet_bytes > ceiling
            || self.midi_events_per_input > ceiling / 2
            || self.max_presets > ceiling / size_of::<Preset>()
            || delay_slots > ceiling / size_of::<[f32; 2]>()
            || presets.len() > self.max_presets
        {
            return Err(Error::Capacity);
        }
        for (slot, preset) in presets.iter().enumerate() {
            if preset.id.slot() as usize != slot {
                return Err(Error::InvalidId);
            }
            if preset.delay_frames > self.max_delay_frames {
                return Err(Error::Capacity);
            }
            if preset
                .live_gain
                .iter()
                .chain(&preset.playback_gain)
                .any(|x| !x.is_finite())
            {
                return Err(Error::InvalidParameter);
            }
        }
        if presets
            .get(initial.slot() as usize)
            .is_none_or(|p| p.id != initial)
        {
            return Err(Error::InvalidId);
        }
        Ok(delay_slots)
    }
}

/// Typed control of a [`crate::Dsp`]. Schedule it as a processor action at the
/// frame where the selection must apply.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Operation {
    SelectPreparedPreset {
        preset: PresetId,
        expected_revision: u32,
    },
}
impl ProcessorOperation for Operation {
    type Result = Result<Receipt, Refusal>;
}

/// A selection applied at its requested sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Receipt {
    pub requested_frame: Frame,
    pub frame: Frame,
    pub previous_revision: u32,
    pub revision: u32,
    pub preset: PresetId,
}

/// A selection that changed nothing.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Refusal {
    /// Delivered after its requested frame; never clamped to a later sample.
    Late {
        requested: Frame,
        decision_frame: Frame,
    },
    InvalidPreset {
        preset: PresetId,
    },
    StaleRevision {
        expected: u32,
        actual: u32,
    },
    RevisionExhausted {
        frame: Frame,
    },
}
